"""Fixed Ubuntu P4.3 actions; no arbitrary root paths, signals or commands."""
from layer4_platform import GUESTS, browsers_for, browser_paths
import datetime as dt
import json
import re

from layer4_uninstall_contract import finalize, redact, save

SCENARIOS = ('policy', 'manifest', 'crash', 'outage')
TRANSIENT_PRE_PAUSE_SECONDS = 12
CASES = {
    'policy': ('P43-BASELINE', 'DES-BROWSER-001', 'DES-BROWSER-004'),
    'manifest': ('P43-BASELINE', 'DES-BROWSER-002', 'DES-BROWSER-004'),
    'crash': ('P43-BASELINE', 'DES-BROWSER-005', 'DES-BROWSER-004'),
    'outage': ('P43-BASELINE', 'DES-BROWSER-003', 'DES-BROWSER-004'),
}
POLICIES = ('/etc/firefox/policies/policies.json', '/etc/opt/chrome/policies/managed/blockuntu.json')
MANIFESTS = ('/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json',
             '/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json')


def fault_paths(guest):
    """Cover RPM and native-release Firefox registrations in the Fedora fixture."""
    policies, manifests = browser_paths(guest)
    if guest == 'fedora':
        manifests = (manifests[0].replace('/usr/lib/', '/usr/lib64/'), manifests[1], manifests[0])
    return policies, manifests
TIMING_NAMES = ('HEARTBEAT_INTERVAL_MS', 'HEARTBEAT_TIMEOUT_MS', 'RPC_TIMEOUT_MS',
    'REVALIDATE_TABS_INTERVAL_MS', 'EXISTING_TAB_FAILURE_THRESHOLD',
    'EXISTING_TAB_FAILURE_GRACE_MS', 'NATIVE_RECONNECT_MAX_MS')
URLS = ('http://web.blockuntu.test:18080/free', 'http://web.blockuntu.test:18080/allowed',
    'http://web.blockuntu.test:18080/firefox/exact/blocked',
    'http://web.blockuntu.test:18080/chrome/exact/blocked',
    'http://web.blockuntu.test:18080/chromium/exact/blocked',
    'https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc')


def installed_timing(source):
    values = {}
    for name in TIMING_NAMES:
        matches = re.findall(r'\bconst\s+' + name + r'\s*=\s*([\d_]+)\s*;', source)
        if len(matches) != 1:
            raise ValueError('Cannot establish installed timing: ' + name)
        values[name] = int(matches[0].replace('_', ''))
        if values[name] <= 0:
            raise ValueError('Invalid installed timing: ' + name)
    # Bound this adapter rather than silently making an indefinite root fault.
    if observation_seconds(values) > 170:
        raise ValueError('Installed timing exceeds this bounded outage adapter')
    return values


def observation_seconds(timing):
    grace = timing['EXISTING_TAB_FAILURE_GRACE_MS'] + timing['EXISTING_TAB_FAILURE_THRESHOLD'] * (
        timing['REVALIDATE_TABS_INTERVAL_MS'] + timing['RPC_TIMEOUT_MS'])
    return (max(grace, timing['HEARTBEAT_TIMEOUT_MS'], timing['NATIVE_RECONNECT_MAX_MS']) + 999) // 1000 + 30


def crash_pause_seconds(timings):
    """Leave a full installed request cycle and timeout before SIGKILL."""
    if not timings:
        raise ValueError('Installed browser timings are required')
    for timing in timings:
        cycle = max(timing['HEARTBEAT_INTERVAL_MS'], timing['REVALIDATE_TABS_INTERVAL_MS'])
        if cycle + timing['RPC_TIMEOUT_MS'] > (TRANSIENT_PRE_PAUSE_SECONDS - 3) * 1000:
            raise ValueError('Installed request cycle exceeds the bounded crash adapter')
    return TRANSIENT_PRE_PAUSE_SECONDS


def verify_crash_pause(fault, timings):
    expected = crash_pause_seconds(timings)
    elapsed = fault.get('transient_pre_pause_elapsed_seconds')
    if (fault.get('transient_pre_pause_seconds') != expected or
            type(elapsed) not in (int, float) or not expected <= elapsed < 240):
        raise ValueError('Missing or insufficient witnessed pre-crash pause')


def diagnostic_targets(browser, heartbeat=None):
    """Bind a client's diagnostic label to its verified native browser pulse."""
    canonical = browser + '_extension'
    if heartbeat is None:
        return (canonical,)
    if (not isinstance(heartbeat, dict) or heartbeat.get('component') != canonical or
            heartbeat.get('state') != 'active' or
            heartbeat.get('current_session_heartbeat') is not True):
        raise ValueError('Diagnostic identity requires a healthy canonical current-session heartbeat')
    reported = heartbeat.get('browser')
    if reported != browser and not (browser == 'chromium' and reported == 'chrome'):
        raise ValueError('Diagnostic reported browser differs from verified native identity')
    return tuple(dict.fromkeys((canonical, reported + '_extension')))


def diagnostic_witness(log, browser, not_before, heartbeat=None):
    """Require this browser's observed transport error after this exact fault."""
    start = dt.datetime.fromisoformat(not_before) - dt.timedelta(seconds=1)
    targets = diagnostic_targets(browser, heartbeat)
    answer = []
    for line in log.splitlines():
        if ('kind="runtime_diagnostic"' not in line or
                not any('target=Some("' + target + '")' in line for target in targets)):
            continue
        matched = re.search(r' details=Some\((".*")\)$',line)
        if not matched:
            continue
        try:
            detail = json.loads(json.loads(matched[1]))
            observed = dt.datetime.fromisoformat(detail['observed_at'].replace('Z','+00:00'))
        except (KeyError, TypeError, ValueError):
            continue
        if observed.tzinfo is not None and observed >= start and detail.get('kind') in ('rpc_timeout','heartbeat_failed','rpc_failed','rpc_abandoned','native_host_disconnected'):
            answer.append(detail)
    return answer


def validate_action(request, scenario):
    if scenario not in SCENARIOS or not isinstance(request, dict) or not re.fullmatch(r'[a-f0-9]{32}', str(request.get('id', ''))):
        raise ValueError('Unknown scenario or malformed browser action ID')
    fields = {'fault': {'id','action','name'}, 'checkpoint': {'id','action','name'},
              'keys': {'id','action','keys'}, 'type_url': {'id','action','url'},
              'screenshot': {'id','action','name'}}
    kind = request.get('action')
    if kind not in fields or set(request) != fields[kind]:
        raise ValueError('Only fixed browser actions are available')
    if kind == 'fault' and request['name'] not in (scenario, 'restore'):
        raise ValueError('Fault does not belong to this clone')
    if kind == 'checkpoint' and request['name'] not in ('baseline','before-fault','during-fault','recovered'):
        raise ValueError('Unknown browser checkpoint')
    if kind == 'screenshot' and request['name'] not in tuple(b+'-'+s for b in ('firefox','chrome','chromium') for s in ('baseline','existing','new','recovered','failure')):
        raise ValueError('Unknown browser screenshot')
    if kind == 'type_url' and request['url'] not in URLS:
        raise ValueError('URL is outside the fixture whitelist')
    if kind == 'keys' and request['keys'] not in ([k] for k in ('KEY_ENTER','KEY_ESC','KEY_TAB')) and request['keys'] not in (
            ['KEY_LEFTALT','KEY_TAB'], ['KEY_LEFTCTRL','KEY_L'], ['KEY_LEFTCTRL','KEY_1'],
            ['KEY_LEFTCTRL','KEY_2'], ['KEY_LEFTCTRL','KEY_T']):
        raise ValueError('Unknown browser key chord')
    return request


def verify_guest(result, scenario, guest='ubuntu'):
    if result.get('status') != 'pass' or (guest not in GUESTS or result.get('guest') != guest) or result.get('scenario') != scenario:
        failures = [(r.get('id'), r.get('error')) for r in result.get('cases', []) if r.get('status') == 'fail']
        raise ValueError('Browser journey failed: ' + repr(failures or result.get('error')))
    if not 1000 <= result.get('ordinary_uid', 0) < 65534:
        raise ValueError('Browser operations must use the ordinary desktop user')
    rows = result.get('cases', [])
    if [r.get('id') for r in rows] != list(CASES[scenario]) or any(r.get('status') != 'pass' or r.get('variant') != scenario for r in rows):
        raise ValueError('Missing, reordered or failed browser observations')
    required = {
        'P43-BASELINE': ('both_browsers', 'installed_timing_verified', 'collector_alive', 'allowed_visible', 'named_block_visible'),
        'DES-BROWSER-001': ('fault_witnessed', 'policies_repaired', 'collector_alive'),
        'DES-BROWSER-002': ('fault_witnessed', 'connections_terminated', 'degraded_visible', 'collector_alive'),
        'DES-BROWSER-003': ('fault_witnessed', 'rpc_failed', 'beyond_installed_tolerance', 'existing_fail_closed', 'new_fail_closed', 'collector_alive'),
        'DES-BROWSER-005': ('fault_witnessed', 'daemon_restarted', 'interruption_reported', 'current_session_heartbeat', 'collector_alive'),
        'DES-BROWSER-004': ('restored', 'new_heartbeats', 'same_profiles', 'browser_identity_expected', 'allowed_visible', 'named_block_visible', 'collector_alive'),
    }
    for row in rows:
        if any(row.get('evidence', {}).get(key) is not True for key in required[row['id']]):
            raise ValueError('Incomplete browser proof: ' + row['id'])
    return len(rows)
