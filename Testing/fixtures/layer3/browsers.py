"""Native store browser acceptance through visible accessibility controls."""
import datetime as dt
import hashlib
import json
from pathlib import Path
import time
import urllib.request
import urllib.error
import pyatspi
from accessibility import click, dump, find, visible_document_text, walk, apps, app_name
from guest import WORK, host_action, rpc, run, wait

STORES = {
    'firefox': 'https://addons.mozilla.org/en-US/firefox/addon/blockuntu/',
    'chrome': 'https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc',
    'chromium': 'https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc',
}
APPS = {'firefox': 'Firefox', 'chrome': 'Google Chrome', 'chromium': 'Chromium'}
BROWSER_CONFIG = {
    'firefox': {
        'binary': '/opt/firefox/firefox',
        'component': 'firefox_extension',
        'policy': '/etc/firefox/policies/policies.json',
        'package_source': 'Mozilla native release archive',
    },
    'chrome': {
        'binary': 'google-chrome',
        'component': 'chrome_extension',
        'policy': '/etc/opt/chrome/policies/managed/blockuntu.json',
        'package_source': 'google-chrome-stable native package',
    },
    'chromium': {
        'binary': 'chromium',
        'component': 'chromium_extension',
        'policy': '/etc/chromium/policies/managed/blockuntu.json',
        'package_source': 'CachyOS native Chromium package',
    },
}


def optional_click(name, app, role='button', seconds=2):
    try:
        find(name, role, app, seconds=seconds, visible=True)
    except TimeoutError:
        return False
    click(name, app=app, role=role)
    return True


def dismiss_google_consent(app):
    """Act on the consent button even when it is below the visible viewport."""
    def page_ready():
        page = visible_document_text(app)
        return ('consent' if 'Before you continue to Google' in page else
                'store' if 'BlocKuntu - Chrome Web Store' in page else None)
    if wait(page_ready, 60) == 'store':
        return False
    button = find('Reject all', 'button', app, seconds=15)
    assert button.getState().contains(pyatspi.STATE_SENSITIVE), 'Store consent button is disabled'
    from ui_controls import focus_app
    focus_app(app)
    assert button.queryComponent().grabFocus(), 'Store consent button did not receive focus'
    wait(lambda: button.getState().contains(pyatspi.STATE_FOCUSED) and
         button.getState().contains(pyatspi.STATE_SHOWING), 15)
    # Chrome can acknowledge AT-SPI activation while leaving this page open.
    # Submit the focused control through native input and observe the store.
    host_action('keys', keys=['KEY_ENTER'])
    wait(lambda: 'BlocKuntu - Chrome Web Store' in visible_document_text(app), 60)
    return True


def add_from_chrome_store(browser, app):
    wait(lambda: 'BlocKuntu - Chrome Web Store' in visible_document_text(app), 60)
    if browser != 'chromium':
        click('Add to Chrome', app=app)
        return
    # The Web Store can show Chromium a Chrome promotion banner while its
    # extension button remains actionable below the viewport.
    assert 'BlocKuntu - Chrome Web Store' in visible_document_text(app), 'BlocKuntu store page is not visible'
    button = find('Add to Chrome', 'button', app, seconds=30)
    assert button.getState().contains(pyatspi.STATE_SENSITIVE), 'Store install button is unavailable'
    assert button.queryAction().doAction(0), 'Store install action failed'


def firefox_managed_store_source(profile, extension_id):
    """Verify that policy installed this profile's add-on from published AMO."""
    policy = json.loads(Path(BROWSER_CONFIG['firefox']['policy']).read_text())
    setting = policy['policies']['ExtensionSettings'][extension_id]
    source = setting['install_url']
    assert (setting['installation_mode'] == 'force_installed' and
            source == 'https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi'), \
        'Firefox managed install is not pinned to published AMO'
    addons = json.loads((profile / 'extensions.json').read_text())['addons']
    matching = [addon for addon in addons if addon['id'] == extension_id]
    assert len(matching) == 1 and matching[0].get('sourceURI') == source, \
        'Firefox profile add-on source differs from the managed AMO URL'
    return {'installation_path': 'managed_store_policy', 'install_url': source,
            'permission_prompt': 'not_applicable_for_force_install'}



def confirm_install(name, app):
    """Confirm the permission prompt and observe dismissal before heartbeat timing.

    Browser security delays can ignore a successful AT-SPI action even when
    the button reports enabled. Retry only this exact visible confirmation.
    """
    # The store may need to download the extension before it creates the
    # browser-owned permission prompt. That time precedes the heartbeat SLA.
    find(name, 'button', app, seconds=60, visible=True)
    click(name, app=app)
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        time.sleep(1)
        try:
            find(name, 'button', app, seconds=1, visible=True)
        except TimeoutError:
            return
        click(name, app=app)
    raise TimeoutError(f'{app} installation permission prompt did not close')


def start(browser, profile, urls):
    if browser == 'firefox':
        command = [BROWSER_CONFIG[browser]['binary'], '--no-remote', '--profile', str(profile)]
    else:
        command = [BROWSER_CONFIG[browser]['binary'], '--user-data-dir=' + str(profile), '--no-first-run',
                   '--password-store=basic', '--force-renderer-accessibility']
    run('systemd-run', '--user', '--collect', '--unit=blockuntu-test-' + browser, *command, *urls)


def stop(browser):
    run('systemctl', '--user', 'stop', 'blockuntu-test-' + browser, check=False)


def wait_for_allowlist_expiry(seconds=225, rpc_timeout=None):
    """Observe natural expiry with the RPC budget, bounded by the overall wait."""
    if rpc_timeout is None:
        rpc_timeout = json.loads((WORK / 'smoke-plan.json').read_text())['deadlines_seconds']['rpc']
    deadline = time.monotonic() + seconds
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(f'Website allowlist did not expire within {seconds:g}s')
        response = rpc('evaluate_url', {
            'url': 'http://outside.blockuntu.test:18080/outside', 'probe': True,
        }, timeout=min(rpc_timeout, remaining))
        if response.get('decision') == 'allow':
            return
        remaining = deadline - time.monotonic()
        if remaining > 0:
            time.sleep(min(1, remaining))


def navigate(browser, url, *, window=None):
    app = APPS[browser]
    name, role = (('Search with Google or enter address', 'combo box') if browser == 'firefox'
                  else ('Address and search bar', 'entry'))
    if window is None:
        from ui_controls import focus_app
        focus_app(app)
        # A new browser can steal focus back to its document during startup.
        # Observe native address-bar selection before typing the fixture URL.
        host_action('keys', keys=['KEY_LEFTCTRL', 'KEY_L'])
        node = find(name, role, app, visible=True)
    else:
        # Regular and private windows expose identical address-bar labels.
        # Keep native input bound to the window whose document is asserted.
        if not window.getState().contains(pyatspi.STATE_ACTIVE):
            # Closing the GUI settings leaves the GUI in front. Chrome's
            # address-bar grabFocus alone does not raise its native window.
            from ui_controls import focus_app
            focus_app(app)
        node = wait(lambda: next((n for n in walk(window) if n.name == name and
                    n.getRoleName() == role and n.getState().contains(pyatspi.STATE_SHOWING)), None), 15)
    assert node.queryComponent().grabFocus(), 'Browser address bar did not receive focus'
    if window is not None:
        wait(lambda: window.getState().contains(pyatspi.STATE_ACTIVE) and
             node.getState().contains(pyatspi.STATE_FOCUSED), 15)
    else:
        wait(lambda: node.getState().contains(pyatspi.STATE_FOCUSED), 15)
    # Firefox accepts EditableText without committing its address-bar model;
    # Chrome does not implement it. Native keys exercise real navigation in both.
    host_action('type_url', url=url)


def page(browser, needle, seconds=45):
    try:
        return wait(lambda: needle in visible_document_text(APPS[browser]), seconds)
    except TimeoutError as error:
        raise TimeoutError(f'{browser} did not show {needle!r} within {seconds}s') from error


def select_chromium_store(browser):
    if browser not in ('chrome', 'chromium'):
        raise ValueError('Store-tab selection requires Chrome or Chromium')
    from ui_controls import focus_app
    focus_app(APPS[browser])
    # Chrome can open an extra welcome/update tab and steal the store's focus.
    # Preserve the first fixture tab and observe navigation to the exact store.
    host_action('keys', keys=['KEY_LEFTCTRL', 'KEY_2'])
    navigate(browser, STORES[browser])
    dismiss_google_consent(APPS[browser])


def select_tab(browser, predicate):
    def selected():
        for application in apps():
            if app_name(application) != APPS[browser]:
                continue
            for node in walk(application):
                if node.getRoleName() == 'page tab' and predicate(node.name):
                    if not node.getState().contains(pyatspi.STATE_SELECTED):
                        assert node.queryAction().doAction(0), 'Could not select browser tab'
                    return node if node.getState().contains(pyatspi.STATE_SELECTED) else None
        return None
    return wait(selected, 30)


def heartbeat(browser):
    status = rpc('extension_status', {'component': BROWSER_CONFIG[browser]['component']})
    return status if status.get('state') == 'active' and status.get('current_session_heartbeat') is True else None


def setup(guest):
    if guest == 'cachyos':
        package = run('pacman', '-Q', 'firefox').stdout.strip()
        assert Path('/usr/bin/firefox').is_file(), 'Native Firefox package binary missing'
        BROWSER_CONFIG['firefox'].update(binary='/usr/bin/firefox',
            package_source='CachyOS native package: ' + package)
    elif not Path('/opt/firefox/firefox').exists():
        archive = WORK / 'firefox.tar.xz'
        answer = run('curl', '--fail', '--location', '--max-time', '180',
                     'https://download.mozilla.org/?product=firefox-latest-ssl&os=linux64&lang=en-US',
                     '-o', str(archive), check=False, timeout=200)
        (WORK / 'firefox-download.log').write_text(answer.stdout + answer.stderr)
        if answer.returncode:
            raise RuntimeError('Native Firefox download failed')
        run('sudo', '-n', 'tar', '-xJf', str(archive), '-C', '/opt')
        (WORK / 'firefox-archive.sha256').write_text(hashlib.sha256(archive.read_bytes()).hexdigest())
    if guest == 'ubuntu':
        # Chrome is provided by the existing Ubuntu template.
        run('dpkg-query', '-W', 'google-chrome-stable')
    elif guest == 'fedora' and run('rpm', '-q', 'google-chrome-stable', check=False).returncode:
        package = WORK / 'google-chrome-stable.rpm'
        answer = run('curl', '--fail', '--location', '--max-time', '180',
                     'https://dl.google.com/linux/direct/google-chrome-stable_current_x86_64.rpm',
                     '-o', str(package), check=False, timeout=200)
        (WORK / 'chrome-download.log').write_text(answer.stdout + answer.stderr)
        if answer.returncode:
            raise RuntimeError('Native Google Chrome download failed')
        (WORK / 'chrome-package.sha256').write_text(hashlib.sha256(package.read_bytes()).hexdigest())
        run('sudo', '-n', 'dnf', 'install', '-y', str(package), timeout=300)
    elif guest == 'cachyos':
        run('sudo', '-n', 'pacman', '-S', '--noconfirm', '--needed', 'chromium', timeout=300)
    run('systemd-run', '--user', '--collect', '--unit=blockuntu-test-site', '/usr/bin/python3',
        str(WORK / 'server.py'), '--bind', '127.0.0.1', '--port', '18080')
    def site_ready():
        try:
            with urllib.request.urlopen('http://127.0.0.1:18080/healthz', timeout=2) as response:
                return response.status == 200
        except (urllib.error.URLError, TimeoutError):
            return False
    wait(site_ready, 15)


def run_p32_website_matchers(browser, run_id, browser_case):
    """Browser-specific routes keep the two native profiles independent."""
    app = APPS[browser]
    base = f'http://web.blockuntu.test:18080/{browser}'
    prefix = f'{run_id}-{browser}-p32'

    def add_rule(suffix, name, tier, pattern):
        rule = {'id': prefix + '-' + suffix, 'name': name, 'tier': tier,
                'enabled': True, 'patterns': [pattern], 'schedule_ids': []}
        rpc('upsert_site_list', {'rule': rule})
        assert any(item['id'] == rule['id'] for item in rpc('config_snapshot')['rules'])
        return rule

    def allowed(url, route):
        navigate(browser, url)
        page(browser, f'BlocKuntu test route {route}')
        assert f'BlocKuntu test route {route}' in visible_document_text(app)

    def blocked(url, rule):
        navigate(browser, url)
        page(browser, 'Blocked by BlocKuntu')
        assert rule['name'] in visible_document_text(app), 'Blocked page names the wrong rule'

    def prefix_match():
        child = f'/{browser}/prefix/blocked/child'
        near = f'/{browser}/prefix/blockedish'
        allowed(base + '/prefix/blocked/child', child)
        rule = add_rule('url-prefix', 'P3.2 URL prefix ' + browser, 'hard',
                        {'kind': 'url_prefix', 'value': base + '/prefix/blocked/'})
        blocked(base + '/prefix/blocked/child', rule)
        allowed(base + '/prefix/blockedish', near)
        host_action('screenshot', name=browser + '-p32-url-prefix')
        return {'rule_id': rule['id'], 'baseline_accessible': True,
                'positive_blocked': True, 'near_miss_accessible': True}
    browser_case('VM-WEB-003', prefix_match)

    def contains_match():
        token = 'blockuntu-marker-' + browser
        route = f'/{browser}/contains'
        positive = base + '/contains?marker=' + token
        negative = base + '/contains?marker=safe'
        allowed(positive, route)
        rule = add_rule('url-contains', 'P3.2 URL contains ' + browser, 'hard',
                        {'kind': 'url_contains', 'value': token})
        blocked(positive, rule)
        allowed(negative, route)
        host_action('screenshot', name=browser + '-p32-url-contains')
        return {'rule_id': rule['id'], 'baseline_accessible': True,
                'positive_blocked': True, 'near_miss_accessible': True}
    browser_case('VM-WEB-004', contains_match)

    def path_match():
        child = f'/{browser}/path/blocked/child'
        near = f'/{browser}/path/blockedish'
        allowed(base + '/path/blocked/child', child)
        rule = add_rule('path-prefix', 'P3.2 path prefix ' + browser, 'hard',
                        {'kind': 'path_prefix', 'value': f'web.blockuntu.test/{browser}/path/blocked/'})
        blocked(base + '/path/blocked/child', rule)
        allowed(base + '/path/blockedish', near)
        host_action('screenshot', name=browser + '-p32-path-prefix')
        return {'rule_id': rule['id'], 'baseline_accessible': True,
                'positive_blocked': True, 'near_miss_accessible': True}
    browser_case('VM-WEB-005', path_match)

    def open_tab_revalidation():
        route = f'/{browser}/longrunning'
        allowed(base + '/longrunning', route)
        rule = add_rule('open-tab', 'P3.2 open tab ' + browser, 'scheduled_block',
                        {'kind': 'exact_url', 'value': base + '/longrunning'})
        session = rpc('start_detox', {'name': 'P3.2 open tab ' + browser,
                                     'duration_minutes': 3, 'site_rule_ids': [rule['id']]})
        page(browser, 'Blocked by BlocKuntu', 45)
        assert rule['name'] in visible_document_text(app), 'Existing tab was not revalidated for its rule'
        host_action('screenshot', name=browser + '-p32-open-tab')
        return {'rule_id': rule['id'], 'detox_session_id': session['session']['id'],
                'open_tab_blocked': True}
    browser_case('VM-WEB-006', open_tab_revalidation)

    def spa_navigation():
        rule = add_rule('spa', 'P3.2 SPA navigation ' + browser, 'hard',
                        {'kind': 'exact_url', 'value': base + '/spa/target'})
        allowed(base + '/spa', f'/{browser}/spa')
        if browser == 'firefox':
            # Firefox's AT-SPI web-button action can report success without
            # dispatching a DOM click. Activate the visible focused button
            # through the guest keyboard, as a user would.
            button = find('Navigate with pushState', 'button', app, visible=True)
            assert button.queryComponent().grabFocus(), 'SPA control did not receive focus'
            host_action('keys', keys=['KEY_ENTER'])
        else:
            click('Navigate with pushState', app=app)
        page(browser, 'Blocked by BlocKuntu', 45)
        assert rule['name'] in visible_document_text(app), 'SPA navigation was not revalidated for its rule'
        host_action('screenshot', name=browser + '-p32-spa')
        return {'rule_id': rule['id'], 'history_navigation_blocked': True}
    browser_case('VM-WEB-007', spa_navigation)


def run_p32_domain(browser, run_id, browser_case):
    """Prove browser blocking and DNS-to-managed-hosts transition together."""
    app = APPS[browser]
    host = f'{browser}.web.blockuntu.test'
    subdomain = 'www.' + host
    near = f'near-{browser}.web.blockuntu.test'
    names = (host, subdomain, near)

    def address(name):
        answer = run('getent', 'ahostsv4', name, check=False)
        return answer.stdout.split()[0] if answer.returncode == 0 else None

    def assert_allowed(name):
        navigate(browser, f'http://{name}:18080/free')
        wait(lambda: name in visible_document_text(app) and
             'BlocKuntu test route /free' in visible_document_text(app), 45)

    def assert_blocked(name, rule):
        navigate(browser, f'http://{name}:18080/free')
        wait(lambda: all(token in visible_document_text(app) for token in
                         ('Blocked by BlocKuntu', name, rule['name'])), 45)

    def domain_match():
        baseline = {name: address(name) for name in names}
        assert all(value == '127.0.0.1' for value in baseline.values()), baseline
        assert_allowed(host)
        assert_allowed(subdomain)
        assert_allowed(near)
        rule = {'id': f'{run_id}-{browser}-p32-domain',
                'name': 'P3.2 domain ' + browser, 'tier': 'hard',
                'mode': 'blocklist', 'enabled': True,
                'patterns': [{'kind': 'domain', 'value': host, 'match_subdomains': True}],
                'schedule_ids': []}
        rpc('upsert_site_list', {'rule': rule})
        assert any(item['id'] == rule['id'] for item in rpc('config_snapshot')['rules'])
        def transitioned():
            addresses = {name: address(name) for name in names}
            return addresses if addresses == {host: '0.0.0.0', subdomain: '0.0.0.0',
                                               near: '127.0.0.1'} else None
        blocked_addresses = wait(transitioned, 45)
        status = rpc('enforcement_status')['hosts_file']
        assert status['managed_block_compliant'], status
        assert_blocked(host, rule)
        assert_blocked(subdomain, rule)
        assert_allowed(near)
        host_action('screenshot', name=browser + '-p32-domain')
        return {'rule_id': rule['id'], 'baseline_resolution': True,
                'managed_hosts_blocked': True, 'positive_blocked': True,
                'subdomain_blocked': True, 'near_miss_accessible': True,
                'baseline_addresses': baseline, 'blocked_addresses': blocked_addresses,
                'host_status_compliant': True}
    browser_case('VM-WEB-001', domain_match)


def run_cases(case, result, run_id, guest='ubuntu', p32_case=None, p32_ids=None, p32_browsers=None):
    setup(guest)
    metadata = {}
    browsers = ['firefox', 'chromium'] if guest == 'cachyos' else ['firefox', 'chrome']
    for browser in browsers:
        app = APPS[browser]
        profile = WORK / (browser + '-profile')
        profile.mkdir(exist_ok=False)
        if browser == 'firefox':
            (profile / 'user.js').write_text('user_pref("browser.shell.checkDefaultBrowser", false);\n')
        config = BROWSER_CONFIG[browser]
        metadata[browser] = {'store_url': STORES[browser], 'profile': str(profile),
            'browser_version': run(config['binary'], '--version').stdout.strip(),
            'package_source': config['package_source'], 'policy_path': config['policy']}
        def browser_case(cid, action):
            passed = case(cid, action, browser=browser)
            if not passed:
                name = browser + '-' + cid.lower() + '-failure'
                (WORK / (name + '.json')).write_text(json.dumps(dump(app), indent=2))
                host_action('screenshot', name=name)
            return passed

        try:
            def onboarding():
                assert not (profile / 'extensions.json').exists(), 'Onboarding profile already has extensions'
                start(browser, profile, ['http://web.blockuntu.test:18080/free', STORES[browser]])
                if browser == 'firefox':
                    optional_click('Continue', app, seconds=15)
                    select_tab(browser, lambda name: 'BlocKuntu' in name and 'test route' not in name)
                    try:
                        find('Add to Firefox', 'link', app, seconds=5, visible=True)
                    except TimeoutError:
                        find('Remove', 'link', app, seconds=30, visible=True)
                        current = wait(lambda: heartbeat(browser), 30)
                        metadata[browser].update(firefox_managed_store_source(profile, current['extension_id']))
                    else:
                        click('Add to Firefox', app=app, role='link')
                        confirm_install('Add', app)
                        optional_click('OK', app, seconds=3)
                        metadata[browser]['installation_path'] = 'store_prompt'
                else:
                    select_chromium_store(browser)
                    add_from_chrome_store(browser, app)
                    confirm_install('Add extension', app)
                status = wait(lambda: heartbeat(browser), 30)
                metadata[browser].update(extension_id=status['extension_id'], extension_version=status['extension_version'],
                                         installed_at=dt.datetime.now(dt.timezone.utc).isoformat())
                # Observe the ordinary tab opened before extension installation.
                select_tab(browser, lambda name: 'BlocKuntu test route /free' in name)
                if browser != 'firefox':
                    host_action('keys', keys=['KEY_LEFTCTRL', 'KEY_1'])
                page(browser, 'BlocKuntu test route /free')
                observed_until = time.monotonic() + status['startup_grace_seconds']
                while time.monotonic() < observed_until:
                    assert heartbeat(browser), 'Browser lost its current-session heartbeat during startup grace'
                    time.sleep(1)
                host_action('screenshot', name=browser + '-onboarded')
                return {'heartbeat': status, 'ordinary_tab_preserved': True, **metadata[browser]}

            installed = browser_case('VM-BR-001', onboarding)
            if not installed:
                (WORK / (browser + '-failure.json')).write_text(json.dumps(dump(app), indent=2))
                host_action('screenshot', name=browser + '-onboarding-failure')
                continue

            def restart():
                old = heartbeat(browser)
                stop(browser)
                component = BROWSER_CONFIG[browser]['component']
                wait(lambda: not rpc('extension_status', {'component': component})['browser_running'], 30)
                # The daemon records session end on its 10-second process scan.
                # Keep the browser absent through a complete scan before restart.
                absent_until = time.monotonic() + 15
                while time.monotonic() < absent_until:
                    assert not rpc('extension_status', {'component': component})['browser_running']
                    time.sleep(1)
                start(browser, profile, ['http://web.blockuntu.test:18080/free'])
                def fresh_heartbeat():
                    status = heartbeat(browser)
                    return status if status and status['session_started_at'] != old['session_started_at'] else None
                current = wait(fresh_heartbeat, 30)
                assert current['extension_version'] == old['extension_version'], 'Extension updated during suite'
                policy = json.loads(Path(metadata[browser]['policy_path']).read_text())
                assert current['extension_id'] in json.dumps(policy), 'Policy does not manage the tested extension'
                navigate(browser, 'about:policies' if browser == 'firefox' else 'chrome://policy')
                page(browser, 'ExtensionSettings' if browser == 'firefox' else 'ExtensionInstallForcelist')
                return {'heartbeat': current, 'policy': policy, 'browser_policy_page_checked': True}
            if not browser_case('VM-BR-002', restart):
                continue

            rule_ids = {rule['id'] for rule in rpc('config_snapshot')['rules']}
            if not {run_id + '-gui-exact', run_id + '-gui-allow'} <= rule_ids:
                result['cases'].extend({'id': cid, 'browser': browser, 'status': 'blocked',
                    'reason': 'GUI-imported website fixtures are missing'}
                    for cid in ('VM-WEB-002', 'VM-WAL-004'))
                continue

            def exact_rule():
                reboot_evidence = {}
                if guest == 'ubuntu':
                    from reboot_policy import URL, NAME
                    record = json.loads((WORK / 'reboot-policy.json').read_text())
                    assert record['boot_id'] != Path('/proc/sys/kernel/random/boot_id').read_text().strip()
                    navigate(browser, URL)
                    page(browser, 'Blocked by BlocKuntu')
                    assert NAME in visible_document_text(app), 'Persisted website did not block after reboot'
                    host_action('screenshot', name=browser + '-reboot-blocked')
                    reboot_evidence = {'persisted_site_blocked': True, 'reboot_site_rule_id': record['site_rule_id']}
                navigate(browser, 'http://web.blockuntu.test:18080/exact/blocked/child')
                page(browser, 'BlocKuntu test route /exact/blocked/child')
                navigate(browser, 'http://web.blockuntu.test:18080/exact/blocked')
                page(browser, 'Blocked by BlocKuntu')
                assert 'Layer 3 GUI imported rule' in visible_document_text(app), 'Blocked page does not identify the expected rule'
                host_action('screenshot', name=browser + '-blocked-exact')
                return {'positive_blocked': True, 'near_miss_accessible': True, **reboot_evidence}
            if not browser_case('VM-WEB-002', exact_rule):
                continue

            if p32_case is not None and (p32_browsers is None or browser in p32_browsers):
                def p32_browser_case(cid, action, scenario='baseline'):
                    passed = p32_case(cid, action, browser=browser, scenario=scenario)
                    if not passed:
                        name = browser + '-' + cid.lower() + '-failure'
                        (WORK / (name + '.json')).write_text(json.dumps(dump(app), indent=2))
                        host_action('screenshot', name=name)
                    return passed
                if set(p32_ids or ()) == {'VM-BR-004', 'VM-WAL-013'}:
                    from private_browsing import run_cases as private_cases
                    private_cases(browser, profile, run_id, p32_browser_case)
                elif set(p32_ids or ()) <= {'VM-ACT-001', 'VM-ACT-002', 'VM-WEB-008', 'VM-WAL-012'}:
                    from lifecycle import run_cases as lifecycle_cases
                    lifecycle_cases(browser, run_id, p32_browser_case, set(p32_ids))
                elif set(p32_ids or ()) == {'VM-WEB-001'}:
                    run_p32_domain(browser, run_id, p32_browser_case)
                else:
                    run_p32_website_matchers(browser, run_id, p32_browser_case)

            def allowlist():
                navigate(browser, 'http://outside.blockuntu.test:18080/outside')
                page(browser, 'BlocKuntu test route /outside')
                session = rpc('start_detox', {'name': 'Acceptance website allowlist ' + browser, 'duration_minutes': 3,
                                             'site_rule_ids': [run_id + '-gui-allow']})
                page(browser, 'Blocked by BlocKuntu')
                assert 'allowlist' in visible_document_text(app).lower(), 'Expected allowlist-specific blocked page'
                navigate(browser, 'http://allowed.blockuntu.test:18080/allowed')
                page(browser, 'BlocKuntu test route /allowed')
                start_time = time.monotonic()
                while time.monotonic() - start_time < 30:
                    assert 'BlocKuntu test route /allowed' in visible_document_text(app)
                    time.sleep(1)
                host_action('screenshot', name=browser + '-allowlist-positive')
                # Let Detox expire naturally before the other browser's store setup.
                # The three-minute session is bounded even after an assertion failure.
                return {'session': session, 'outside_rejected': True, 'listed_allowed': True}
            browser_case('VM-WAL-004', allowlist)
            wait_for_allowlist_expiry()
        finally:
            stop(browser)
            (WORK / 'browser-metadata.json').write_text(json.dumps(metadata, indent=2))
