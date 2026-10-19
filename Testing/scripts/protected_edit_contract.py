"""Strict evidence requirements for the small Tier 1 journey."""
import datetime as dt
import re

CASE_IDS = ('TIER1-NO-KEY', 'DES-PROTECT-004', 'DES-PROTECT-005',
            'DES-PROTECT-007', 'DES-PROTECT-006')


def input_keys(value):
    """Encode only this journey's rule text and installer credential format."""
    if not isinstance(value, str) or not re.fullmatch(
            r'Acceptance Tier1 (?:edited|rejected)|'
            r'http://web\.blockuntu\.test:18080/path/free|'
            r'BLOCKUNTU-TIER1-EDIT(?:-[A-Z0-9]+)+', value):
        raise ValueError('Input outside the protected-edit fixture')
    punctuation = {' ': ('KEY_SPACE',), '/': ('KEY_SLASH',), '.': ('KEY_DOT',),
                   ':': ('KEY_LEFTSHIFT', 'KEY_SEMICOLON'), '-': ('KEY_MINUS',)}
    return [punctuation[c] if c in punctuation else
            ('KEY_LEFTSHIFT', 'KEY_' + c.upper()) if c.isupper() else
            ('KEY_' + c.upper(),) for c in value]


def validate_action(request):
    if not isinstance(request, dict) or not re.fullmatch(r'[a-f0-9]{32}', str(request.get('id', ''))):
        raise ValueError('Invalid protected-edit action ID')
    action = request.get('action')
    fields = {'type_form': {'id', 'action', 'value'},
              'type_credential': {'id', 'action', 'slot'},
              'screenshot': {'id', 'action', 'name'}, 'keys': {'id', 'action', 'keys'}}
    if not isinstance(action, str) or action not in fields or set(request) != fields[action]:
        raise ValueError('Malformed protected-edit action')
    if action == 'type_form':
        if str(request['value']).startswith('BLOCKUNTU-'):
            raise ValueError('Credential values must not be sent as form requests')
        input_keys(request['value'])
    elif action == 'type_credential':
        if request['slot'] not in ('invalid', 'tier1'):
            raise ValueError('Unknown credential slot')
    elif action == 'screenshot':
        if request['name'] not in ('tier1-authorized-edit', 'tier1-tier-boundary', 'tier1-expired-edit', 'tier1-focus-failure'):
            raise ValueError('Unknown protected-edit screenshot')
    elif request['keys'] not in (['KEY_LEFTALT', 'KEY_TAB'], ['KEY_ESC'], ['KEY_ENTER']):
        raise ValueError('Unknown protected-edit key combination')
    return request


def verify_guest(result, expected_guest=None):
    if expected_guest is not None and (expected_guest not in ('ubuntu', 'fedora', 'cachyos') or result.get('guest') != expected_guest):
        raise ValueError('Protected-edit result belongs to a different guest')
    if result.get('status') != 'pass':
        raise ValueError('Protected-edit journey failed: ' + str(result.get('error', 'no complete result')))
    if not 1000 <= result.get('ordinary_uid', 0) < 65534:
        raise ValueError('Journey must pass as an ordinary user')
    if result.get('protected_access_mode') != 'all_time':
        raise ValueError('Expected a permitted real protected-access window')
    for key in ('clock_integrity_at_start', 'clock_integrity_at_end'):
        if result.get(key, {}).get('state') != 'ok' or result[key].get('boot_id') in (None, 'test'):
            raise ValueError('Missing healthy clock evidence')
    cases = result.get('cases', [])
    if [r.get('id') for r in cases] != list(CASE_IDS) or any(r.get('status') != 'pass' for r in cases):
        raise ValueError('Missing, failed, duplicated or reordered protected-edit observations')
    evidence = {r['id']: r['evidence'] for r in cases}
    for cid in ('TIER1-NO-KEY', 'DES-PROTECT-004', 'DES-PROTECT-006'):
        if any(evidence[cid].get(k) is not True for k in (
                'rpc_rejected', 'policy_unchanged', 'gui_name_disabled', 'gui_saved_pattern_disabled')):
            raise ValueError('Incomplete locked-edit evidence: ' + cid)
    if evidence['DES-PROTECT-004'].get('gui_invalid_key_error') is not True:
        raise ValueError('Missing GUI invalid-key rejection')
    authorized = evidence['DES-PROTECT-005']
    if any(authorized.get(k) is not True for k in (
            'gui_key_granted', 'gui_save_warning_confirmed', 'gui_name_and_pattern_saved')):
        raise ValueError('Missing actual authorized GUI save')
    saved = authorized.get('saved_rule', {})
    if (saved.get('tier') != 'hard' or saved.get('name') != 'Acceptance Tier1 edited' or
            saved.get('patterns', [{}])[0].get('value') != 'http://web.blockuntu.test:18080/path/free'):
        raise ValueError('Authorized GUI save did not persist the expected rule')
    boundary = evidence['DES-PROTECT-007']
    if boundary.get('grant_active_for_both_checks') is not True or [r.get('tier') for r in boundary.get('tiers', [])] != [2, 3]:
        raise ValueError('Both active rule tiers must be checked during the grant')
    for row in boundary['tiers']:
        if any(row.get(k) is not True for k in ('rpc_rejected', 'policy_unchanged', 'gui_name_disabled', 'gui_saved_pattern_disabled')):
            raise ValueError('Tier boundary edit was not rejected')
        if row.get('rule_id') not in row.get('session', {}).get('site_rule_ids', []):
            raise ValueError('Boundary rule was not in the active Detox session')
        session = row['session']
        checked = dt.datetime.fromisoformat(row['checked_at'])
        if not (dt.datetime.fromisoformat(session['starts_at']) <= checked <
                dt.datetime.fromisoformat(session['ends_at']) and
                checked < dt.datetime.fromisoformat(result['grant']['expires_at'])):
            raise ValueError('Tier boundary checked outside the active session or grant')
    expiry = evidence['DES-PROTECT-006']
    if expiry.get('natural_expiry') is not True or expiry.get('unlock_inactive') is not True:
        raise ValueError('Missing natural expiry')
    end = dt.datetime.fromisoformat(expiry['expires_at'])
    observed = dt.datetime.fromisoformat(expiry['observed_at'])
    if observed <= end or authorized['expires_at'] != result['grant']['expires_at'] or end != dt.datetime.fromisoformat(result['grant']['expires_at']):
        raise ValueError('Expiry evidence does not match the granted window')
    if not 270 <= result['grant'].get('remaining_seconds', 0) <= 300:
        raise ValueError('Expected a real five-minute grant')
    return len(cases)
