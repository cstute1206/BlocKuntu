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
    assert button.queryAction().doAction(0), 'Could not reject store consent'
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


def navigate(browser, url):
    app = APPS[browser]
    name, role = (('Search with Google or enter address', 'combo box') if browser == 'firefox'
                  else ('Address and search bar', 'entry'))
    node = find(name, role, app)
    assert node.queryComponent().grabFocus(), 'Browser address bar did not receive focus'
    # Firefox accepts EditableText without committing its address-bar model;
    # Chrome does not implement it. Native keys exercise real navigation in both.
    host_action('type_url', url=url)


def page(browser, needle, seconds=45):
    try:
        return wait(lambda: needle in visible_document_text(APPS[browser]), seconds)
    except TimeoutError as error:
        raise TimeoutError(f'{browser} did not show {needle!r} within {seconds}s') from error


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


def run_cases(case, result, run_id, guest='ubuntu'):
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
                    select_tab(browser, lambda name: bool(name) and 'test route' not in name)
                    dismiss_google_consent(app)
                    add_from_chrome_store(browser, app)
                    confirm_install('Add extension', app)
                status = wait(lambda: heartbeat(browser), 30)
                metadata[browser].update(extension_id=status['extension_id'], extension_version=status['extension_version'],
                                         installed_at=dt.datetime.now(dt.timezone.utc).isoformat())
                # Observe the ordinary tab opened before extension installation.
                select_tab(browser, lambda name: 'BlocKuntu test route /free' in name)
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
            browser_case('VM-BR-002', restart)

            rule_ids = {rule['id'] for rule in rpc('config_snapshot')['rules']}
            if not {run_id + '-gui-exact', run_id + '-gui-allow'} <= rule_ids:
                result['cases'].extend({'id': cid, 'browser': browser, 'status': 'blocked',
                    'reason': 'GUI-imported website fixtures are missing'}
                    for cid in ('VM-WEB-002', 'VM-WAL-004'))
                continue

            def exact_rule():
                navigate(browser, 'http://web.blockuntu.test:18080/exact/blocked/child')
                page(browser, 'BlocKuntu test route /exact/blocked/child')
                navigate(browser, 'http://web.blockuntu.test:18080/exact/blocked')
                page(browser, 'Blocked by BlocKuntu')
                assert 'Layer 3 GUI imported rule' in visible_document_text(app), 'Blocked page does not identify the expected rule'
                host_action('screenshot', name=browser + '-blocked-exact')
                return {'positive_blocked': True, 'near_miss_accessible': True}
            browser_case('VM-WEB-002', exact_rule)

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
            wait(lambda: rpc('evaluate_url', {'url': 'http://outside.blockuntu.test:18080/outside', 'probe': True}).get('decision') == 'allow', 225)
        finally:
            stop(browser)
            (WORK / 'browser-metadata.json').write_text(json.dumps(metadata, indent=2))
