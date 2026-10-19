"""Fixed Ubuntu P4.2 fault/evidence boundaries, sharing the P4.0 foundation."""
from layer4_platform import GUESTS, browsers_for
import collections
import re

from layer4_uninstall_contract import finalize, redact, save

SCENARIOS = ('remove', 'modify', 'append', 'delete', 'allowlist')
CASES = {
    'remove': ('P42-BASELINE', 'DES-HOSTS-001', 'DES-HOSTS-004'),
    'modify': ('P42-BASELINE', 'DES-HOSTS-001'),
    'append': ('P42-BASELINE', 'DES-HOSTS-002'),
    'delete': ('P42-BASELINE', 'DES-HOSTS-003'),
    'allowlist': ('P42-BASELINE', 'DES-HOSTS-005'),
}
FAULTS = {'remove': 'remove-managed', 'modify': 'modify-managed',
          'append': 'append-unrelated', 'delete': 'delete-hosts'}
CHECKPOINTS = ('baseline', 'before-fault', 'after-fault', 'schedule-inactive',
               'schedule-active', 'schedule-ended', 'allowlist-active')
BEGIN, END = '# BEGIN BLOCKUNTU MANAGED', '# END BLOCKUNTU MANAGED'
HARD = 'hard.p42.blockuntu.test'
SCHEDULED = 'scheduled.p42.blockuntu.test'
FREE = 'free.p42.blockuntu.test'
OUTSIDE = 'outside.p42.blockuntu.test'
BASE_SENTINEL = '127.0.0.42 baseline.p42.blockuntu.test'
APPEND_SENTINEL = '127.0.0.43 unrelated.p42.blockuntu.test'


def split_hosts(text):
    """One complete, line-anchored managed region; never accept duplicate markers."""
    lines = text.splitlines(keepends=True)
    starts = [i for i, line in enumerate(lines) if line.rstrip('\r\n') == BEGIN]
    ends = [i for i, line in enumerate(lines) if line.rstrip('\r\n') == END]
    if len(starts) != 1 or len(ends) != 1 or starts[0] >= ends[0]:
        raise ValueError('Missing, reordered or repeated managed hosts markers')
    a, b = starts[0], ends[0]
    return ''.join(lines[a:b+1]), ''.join(lines[:a] + lines[b+1:])


def external_rows(text):
    return collections.Counter(line for line in text.splitlines() if line.strip())


def domain_rows(block):
    result = []
    for line in block.splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        parts = line.split()
        if len(parts) != 2 or parts[0] not in ('0.0.0.0', '::'):
            raise ValueError('Unexpected managed hosts row: ' + line)
        result.append(tuple(parts))
    return collections.Counter(result)


def expected_rows(domains):
    return collections.Counter((ip, name) for name in domains for ip in ('0.0.0.0', '::'))


def mutated_hosts(text, fault):
    block, external = split_hosts(text)
    if fault == 'remove-managed':
        replacement = BEGIN + '\n' + END + '\n'
        return text.replace(block, replacement, 1)
    if fault == 'modify-managed':
        replacement = block.replace('0.0.0.0 ', '127.0.0.2 ').replace(':: ', '::1 ')
        replacement = replacement.replace(END, '127.0.0.2 injected.p42.blockuntu.test\n' + END)
        return text.replace(block, replacement, 1)
    if fault == 'append-unrelated':
        if APPEND_SENTINEL in text:
            raise ValueError('Append sentinel must be fresh')
        return text.rstrip('\n') + '\n' + APPEND_SENTINEL + '\n'
    raise ValueError('Unknown hosts content fault')


def validate_action(request, scenario):
    if scenario not in CASES or not isinstance(request, dict) or not re.fullmatch(r'[a-f0-9]{32}', str(request.get('id', ''))):
        raise ValueError('Unknown scenario or malformed action ID')
    kind = request.get('action')
    if kind not in ('fault', 'checkpoint') or set(request) != {'id', 'action', 'name'}:
        raise ValueError('Only named hosts faults/checkpoints are available')
    if kind == 'fault' and (scenario not in FAULTS or request['name'] != FAULTS[scenario]):
        raise ValueError('Fault does not belong to this clone scenario')
    if kind == 'checkpoint' and request['name'] not in CHECKPOINTS:
        raise ValueError('Unknown hosts checkpoint')
    return request


def verify_guest(result, scenario, guest='ubuntu'):
    if scenario not in CASES or (guest not in GUESTS or result.get('guest') != guest) or result.get('scenario') != scenario:
        raise ValueError('Wrong hosts scenario/guest')
    if result.get('status') != 'pass':
        failed = next((r for r in result.get('cases', []) if r.get('status') == 'fail'), {})
        raise ValueError('Hosts journey failed: ' + str(failed.get('id', 'prerequisite')) + ': ' +
                         redact(failed.get('error', result.get('error', 'incomplete checkpoint'))))
    if not 1000 <= result.get('ordinary_uid', 0) < 65534:
        raise ValueError('Policy/function checks must use the ordinary desktop user')
    rows = result.get('cases', [])
    if [r.get('id') for r in rows] != list(CASES[scenario]) or any(r.get('status') != 'pass' or r.get('variant') != scenario for r in rows):
        raise ValueError('Missing, reordered or failed hosts observations')
    required = {
        'P42-BASELINE': ('installed', 'clock_healthy', 'managed_exact', 'immutable_enabled', 'collector_alive', 'control_http_accessible', 'hard_resolution_blocked', 'hard_http_blocked'),
        'DES-HOSTS-001': ('fault_witnessed', 'managed_exact', 'immutable_restored', 'unrelated_preserved', 'hard_resolution_blocked', 'hard_http_blocked', 'control_http_accessible', 'policy_unchanged', 'collector_alive'),
        'DES-HOSTS-002': ('fault_witnessed', 'managed_exact', 'immutable_restored', 'unrelated_preserved', 'sentinel_preserved', 'sentinel_resolves', 'hard_resolution_blocked', 'hard_http_blocked', 'control_http_accessible', 'policy_unchanged', 'collector_alive'),
        'DES-HOSTS-003': ('fault_witnessed', 'file_recreated', 'managed_exact', 'immutable_restored', 'hard_resolution_blocked', 'hard_http_blocked', 'control_http_accessible', 'policy_unchanged', 'collector_alive', 'external_loss_recorded'),
        'DES-HOSTS-004': ('initially_inactive', 'natural_activation', 'natural_expiry', 'scheduled_resolution_transitions', 'scheduled_http_transitions', 'tier1_persisted', 'immutable_enabled', 'unrelated_preserved', 'clock_healthy', 'collector_alive'),
        'DES-HOSTS-005': ('allowlist_active', 'outside_rpc_blocked', 'listed_rpc_allowed', 'managed_empty', 'no_catch_all', 'immutable_enabled', 'unrelated_preserved', 'direct_http_accessible', 'collector_alive'),
    }
    for row in rows:
        proof = row.get('evidence', {})
        if not isinstance(proof, dict) or any(proof.get(key) is not True for key in required[row['id']]):
            raise ValueError('Incomplete hosts proof: ' + row['id'])
    return len(rows)
