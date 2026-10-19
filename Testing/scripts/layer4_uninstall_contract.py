"""Bounded native uninstall actions and acceptance evidence; no VM side effects."""
from layer4_platform import GUESTS, browsers_for
import datetime as dt
import json
import os
from pathlib import Path
import re

SCENARIOS = ('remove', 'purge', 'authorized')
CASES = {
    'remove': ('P41-BASELINE', 'DES-UNINSTALL-001'),
    'purge': ('P41-BASELINE', 'DES-UNINSTALL-001'),
    'authorized': ('P41-BASELINE', 'DES-UNINSTALL-004', 'DES-PROTECT-001',
                   'DES-PROTECT-002', 'DES-PROTECT-003', 'DES-UNINSTALL-005',
                   'DES-UNINSTALL-006', 'DES-UNINSTALL-007'),
}


def cases_for(guest):
    """Preserve shared journeys while identifying native refusal observations."""
    browsers_for(guest)
    refusal = {'ubuntu': 'DES-UNINSTALL-001', 'fedora': 'DES-UNINSTALL-002',
               'cachyos': 'DES-UNINSTALL-003'}[guest]
    return {scenario: tuple(refusal if cid == 'DES-UNINSTALL-001' else cid for cid in ids)
            for scenario, ids in CASES.items()}
SECRET = re.compile(r'BLOCKUNTU-(?:UNINSTALL-RECOVERY|TIER1-EDIT)(?:-[A-Z0-9]+)+|'
                    r'(?<=BLOCKUNTU_PACKAGE_REMOVAL_LEASE=)[a-f0-9]{64}|'
                    r'(?<="package_removal_lease": ")[a-f0-9]{64}')


def redact(value):
    value = SECRET.sub('[REDACTED TEST CREDENTIAL]', str(value))
    return re.sub(r'("(?:package_removal_lease|lease_token)"\s*:\s*")[a-f0-9]{64}',
                  r'\1[REDACTED TEST CREDENTIAL]', value)


def save(path, value):
    """Publish complete, redacted checkpoints atomically with owner-only access."""
    path = Path(path)
    temporary = path.with_suffix(path.suffix + '.tmp')
    data = redact(json.dumps(value, indent=2)) + '\n'
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, 'w') as stream:
        os.fchmod(stream.fileno(), 0o600)
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)


def input_keys(value):
    if not isinstance(value, str) or not re.fullmatch(
            r'BLOCKUNTU-UNINSTALL-RECOVERY(?:-[A-Z0-9]+)+', value):
        raise ValueError('Input outside uninstall credential format')
    return [('KEY_MINUS',) if c == '-' else
            ('KEY_LEFTSHIFT', 'KEY_' + c) if c.isalpha() else
            ('KEY_' + c,) for c in value]


def firefox_sentinel_preservation(before, after):
    """Attribute removal only when the immediate pre-uninstall witness exists."""
    def value(text):
        document = json.loads(text or '{}')
        return document.get('policies', {}).get('DisableTelemetry')
    pre, post = value(before), value(after)
    return {'before': pre, 'after': post, 'covered': pre is True,
            'status': ('not_covered' if pre is not True else
                       'preserved' if post is True else 'lost_during_uninstall')}


def handoff_heartbeats(samples, browsers):
    """Use the daemon transition, not the observer's later receipt time."""
    witnessed = [sample for sample in samples if sample.get('state') == 'uninstalling']
    transitions = [sample.get('handoff') for sample in witnessed
                   if isinstance(sample.get('handoff'), dict) and sample['handoff'].get('mode') == 'uninstalling']
    if not transitions:
        raise ValueError('Missing daemon uninstall transition witness')
    first = dt.datetime.fromisoformat(transitions[0]['started_at'].replace('Z', '+00:00'))
    if any(dt.datetime.fromisoformat(item['started_at'].replace('Z', '+00:00')) != first for item in transitions):
        raise ValueError('Uninstall transition changed during the handoff')
    result = {}
    for browser in browsers:
        pulses = [sample.get('browsers', {}).get(browser, {}) for sample in witnessed]
        result[browser] = any(pulse.get('current_session_heartbeat') is True and
            pulse.get('last_seen_at') and
            dt.datetime.fromisoformat(pulse['last_seen_at'].replace('Z', '+00:00')) >= first
            for pulse in pulses)
    return result


def installed_packages_preserved(before, after):
    """A rejected native transaction must leave the complete package set intact."""
    for inventory in (before, after):
        if (not isinstance(inventory, dict) or inventory.get('guest') not in ('fedora', 'cachyos') or
                inventory.get('returncode') != 0 or not isinstance(inventory.get('packages'), list) or
                not inventory['packages'] or
                any(not isinstance(row, str) or len(row.split()) != 2 for row in inventory['packages']) or
                inventory['packages'] != sorted(set(inventory['packages']))):
            raise ValueError('Missing or invalid complete package inventory')
    return before['guest'] == after['guest'] and before['packages'] == after['packages']


def native_purge_witness(logs, not_before, version):
    """Current or rotated logs must prove this invocation, not template history."""
    start = dt.datetime.fromisoformat(not_before) - dt.timedelta(seconds=2)
    end = dt.datetime.now().astimezone() + dt.timedelta(seconds=2)
    evidence = {'purge': [], 'not_installed': []}
    for source in logs:
        for line in source.get('events', []):
            fields = line.split()
            try:
                stamp = dt.datetime.strptime(' '.join(fields[:2]), '%Y-%m-%d %H:%M:%S').astimezone()
            except ValueError:
                continue
            if not start <= stamp <= end:
                continue
            kind = ('purge' if len(fields) >= 5 and fields[2] == 'purge' and
                    fields[3].split(':')[0] == 'blockuntu' and fields[4] == version else
                    'not_installed' if len(fields) >= 5 and fields[2:4] == ['status', 'not-installed'] and
                    fields[4].split(':')[0] == 'blockuntu' else None)
            if kind:
                evidence[kind].append({'path': source['path'], 'event': line})
    return evidence if all(evidence.values()) else None


def native_removal_witness(logs, not_before, version, guest):
    if guest == 'ubuntu':
        return native_purge_witness(logs, not_before, version)
    start = dt.datetime.fromisoformat(not_before) - dt.timedelta(seconds=2)
    end = dt.datetime.now().astimezone() + dt.timedelta(seconds=2)
    found = []
    for source in logs:
        for line in source.get('events', []):
            if guest == 'cachyos':
                match = re.match(r'\[([^]]+)\] \[ALPM\] removed blockuntu \(([^)]+)\)$', line)
                if not match or match[2] != version:
                    continue
                stamp = dt.datetime.fromisoformat(match[1])
            elif guest == 'fedora':
                match = re.match(r'(\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:[+-]\d{2}:?\d{2}|Z)?)', line)
                package = r'\bblockuntu-(?:0:)?' + re.escape(version) + r'(?:\.|\b)'
                completed = (re.search(r'(?i)\b(?:erased|removed)\b.*' + package, line) or
                             re.search(r'RPM callback (?:uninstall|erase) stop.*' + package, line))
                if not match or not completed:
                    continue
                stamp = dt.datetime.fromisoformat(match[1].replace(',', '.')).astimezone()
            else:
                raise ValueError('Unsupported native transaction witness')
            if start <= stamp <= end:
                found.append({'path': source['path'], 'event': line})
    return {'removal': found} if found else None


def validate_action(request, scenario):
    if scenario not in SCENARIOS:
        raise ValueError('Unknown uninstall scenario')
    if not isinstance(request, dict) or not isinstance(request.get('id'), str) or not re.fullmatch(r'[a-f0-9]{32}', request['id']):
        raise ValueError('Invalid uninstall action ID')
    fields = {'type_credential': {'id', 'action', 'slot'},
              'keys': {'id', 'action', 'keys'},
              'type_url': {'id', 'action', 'url'},
              'screenshot': {'id', 'action', 'name'},
              'focus_gui': {'id', 'action'},
              'checkpoint': {'id', 'action', 'name'},
              'fault': {'id', 'action', 'name'}}
    action = request.get('action')
    if not isinstance(action, str) or action not in fields or set(request) != fields[action]:
        raise ValueError('Malformed uninstall action')
    if action == 'focus_gui':
        if scenario != 'authorized':
            raise ValueError('GUI focus is only available in the authorized journey')
    elif action == 'type_credential':
        if scenario != 'authorized' or request['slot'] not in ('invalid', 'uninstall'):
            raise ValueError('Unknown uninstall credential slot')
    elif action == 'keys':
        if request['keys'] not in ([k] for k in ('KEY_ENTER', 'KEY_ESC', 'KEY_HOME', 'KEY_DOWN', 'KEY_TAB')) and request['keys'] not in (
                ['KEY_LEFTALT', 'KEY_TAB'], ['KEY_LEFTCTRL', 'KEY_L'],
                ['KEY_LEFTCTRL', 'KEY_1'], ['KEY_LEFTCTRL', 'KEY_2']):
            raise ValueError('Unknown uninstall key combination')
    elif action == 'type_url':
        if scenario != 'authorized' or request['url'] not in (
                'about:policies', 'about:addons', 'chrome://policy', 'chrome://extensions',
                'https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc',
                'http://web.blockuntu.test:18080/free',
                'http://web.blockuntu.test:18080/firefox/exact/blocked',
                'http://web.blockuntu.test:18080/chrome/exact/blocked',
                'http://web.blockuntu.test:18080/chromium/exact/blocked'):
            raise ValueError('URL outside uninstall selection')
    elif action == 'fault':
        if scenario == 'authorized' or request['name'] != 'direct-' + scenario:
            raise ValueError('Fault does not belong to this clone scenario')
    elif action == 'checkpoint':
        if request['name'] not in ('baseline', 'before-uninstall', 'after-uninstall'):
            raise ValueError('Unknown uninstall checkpoint')
    elif request['name'] not in ('p41-baseline', 'p41-post-remove', 'p41-post-purge',
                                 'p41-firefox-recovered', 'p41-chrome-recovered', 'p41-chromium-recovered'):
        raise ValueError('Unknown uninstall screenshot')
    return request


def verify_guest(result, scenario, guest='ubuntu'):
    if scenario not in CASES or result.get('scenario') != scenario or (guest not in GUESTS or result.get('guest') != guest):
        raise ValueError('Wrong uninstall scenario/guest')
    if result.get('status') != 'pass':
        failed = next((r for r in result.get('cases', []) if r.get('status') == 'fail'), {})
        raise ValueError('Uninstall journey failed: ' + str(failed.get('id', 'prerequisite')) + ': ' +
                         redact(failed.get('error', result.get('error', 'no complete checkpoint'))))
    if not 1000 <= result.get('ordinary_uid', 0) < 65534:
        raise ValueError('Uninstall operations ran as a privileged user')
    rows = result.get('cases', [])
    if [r.get('id') for r in rows] != list(cases_for(guest)[scenario]) or any(r.get('status') != 'pass' for r in rows):
        raise ValueError('Missing, reordered or failed uninstall cases')
    if any(r.get('variant') != scenario for r in rows):
        raise ValueError('Wrong uninstall observation variant')
    required = {
        'P41-BASELINE': ('installed', 'clock_healthy', 'hosts_blocked', 'process_blocked', 'near_miss_alive', 'collector_alive'),
        'DES-UNINSTALL-001': ('command_rejected', 'package_installed', 'hosts_blocked', 'process_blocked', 'near_miss_alive', 'no_lease'),
        'DES-UNINSTALL-004': ('gui_wrong_phrase_rejected', 'no_lease', 'policy_unchanged'),
        'DES-PROTECT-001': ('schedule_active', 'gui_uninstall_rejected', 'settings_locked', 'rpc_uninstall_rejected', 'no_lease'),
        'DES-PROTECT-002': ('detox_active', 'gui_uninstall_rejected', 'settings_locked', 'rpc_uninstall_rejected', 'no_lease'),
        'DES-PROTECT-003': ('natural_expiry', 'no_active_sessions', 'protected_access_open', 'settings_available', 'clock_healthy'),
        'DES-UNINSTALL-005': ('gui_initiated', 'uninstalling_witnessed', 'both_extensions_notified', ('native_dpkg_purge' if guest == 'ubuntu' else 'native_package_removal'), 'package_absent', 'gui_closed', 'collector_alive'),
        'DES-UNINSTALL-006': ('managed_state_removed', 'unrelated_hosts_preserved', 'firefox_setting_preserved', 'chromium_policy_preserved'),
        'DES-UNINSTALL-007': ('same_browsers_alive', 'firefox_management_accessible', browsers_for(guest)[1] + '_management_accessible', 'firefox_http_accessible', browsers_for(guest)[1] + '_http_accessible', 'past_failure_tolerance'),
    }
    required['DES-UNINSTALL-002'] = (*required['DES-UNINSTALL-001'], 'gui_restart_healthy', 'all_packages_preserved')
    required['DES-UNINSTALL-003'] = (*required['DES-UNINSTALL-001'], 'gui_restart_healthy', 'all_packages_preserved')
    for row in rows:
        proof = row.get('evidence', {})
        if not isinstance(proof, dict) or any(proof.get(k) is not True for k in required[row['id']]):
            raise ValueError('Incomplete evidence for ' + row['id'])
    if scenario == 'authorized' and result.get('baseline_browser_count') != 2:
        raise ValueError('Both real browser profiles are required')
    return len(rows)


def finalize(vm, suite, record, result):
    """Never clean a failing clone; safety/cleanup errors override product passes."""
    try:
        suite.stop(record)
        vm.unchanged(record)
        result['base_integrity_verified'] = True
        if result['status'] == 'pass' and not result.get('collection_errors'):
            vm.cleanup(record)
            vm.unchanged(record)
            result['clone_removed'] = True
        else:
            result.update(status='fail', clone_retained=True, clone_stopped=True)
    except Exception as error:
        result.update(status='fail', safety_error=redact(error), clone_retained=True)
