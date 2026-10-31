"""Ordinary-user Tier 1 GUI authorization on an installed package.

Reuse the native AT-SPI/keyboard adapters, without browser-store prerequisites.
Credentials never enter host-action requests, screenshots or result evidence.
"""
import argparse
import copy
import datetime as dt
import json
import os
import time
import traceback

from accessibility import click, dump, find, redact, texts
from guest import WORK, host_action, rpc, run, wait, configure_desktop
from gui import leave_desktop_overview
from ui_controls import edit, enabled, entries, field, focus_app, nodes, open_settings, press, press_observed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--guest', choices=('ubuntu', 'fedora', 'cachyos'), default='ubuntu')
    args = parser.parse_args()
    result = {'status': 'running', 'cases': [], 'ordinary_uid': os.getuid(), 'guest': args.guest}

    def save():
        temporary = WORK / 'protected-result.tmp'
        temporary.write_text(json.dumps(result, indent=2) + '\n')
        temporary.replace(WORK / 'protected-result.json')

    def case(cid, action):
        started = time.monotonic()
        row = {'id': cid}
        try:
            row.update(status='pass', evidence=action())
        except Exception as error:
            row.update(status='fail', error=redact(str(error)), traceback=redact(traceback.format_exc()))
            if cid == 'TIER1-NO-KEY' and not nodes(role='dialog', name='Welcome to BlocKuntu'):
                host_action('screenshot', name='tier1-focus-failure')
        row['seconds'] = round(time.monotonic() - started, 3)
        result['cases'].append(row)
        save()
        print(cid, row['status'], row.get('error', ''), flush=True)
        if row['status'] != 'pass':
            raise RuntimeError('Protected-edit journey stopped at ' + cid)

    def rule(rid):
        return next(r for r in rpc('config_snapshot')['rules'] if r['id'] == rid)

    def rejected(original, name):
        candidate = copy.deepcopy(original)
        candidate['name'] = name
        candidate['patterns'][0]['value'] = ('http://web.blockuntu.test:18080/path/blocked'
            if original['patterns'][0]['value'].endswith('/free') else
            'http://web.blockuntu.test:18080/path/free')
        before = rpc('config_snapshot')
        try:
            rpc('upsert_site_list', {'rule': candidate})
        except RuntimeError as error:
            message = str(error)
            assert 'active' in message and 'cannot be edited' in message, message
        else:
            raise AssertionError('Daemon accepted a protected edit')
        assert rpc('config_snapshot') == before, 'Rejected edit changed policy'
        return {'rpc_rejected': True, 'policy_unchanged': True}

    def show(original, tier):
        focus_app()
        if nodes(role='dialog', name='Settings'):
            click('Close Settings')
        click('Websites')
        press('Refresh')
        press(original['name'] + ' Tier ' + str(tier))

    def locked(original, tier):
        show(original, tier)
        wait(lambda: not enabled(field('Name')), 30)
        saved = wait(lambda: entries(table=True, showing=False), 15)
        assert not enabled(saved[0]), 'Saved pattern remains editable'
        proof = rejected(original, 'Acceptance Tier1 rejected')
        return {**proof, 'gui_name_disabled': True, 'gui_saved_pattern_disabled': True}

    hard = None
    try:
        assert os.geteuid() != 0, 'Policy and GUI operations must be unprivileged'
        save()
        configure_desktop(args.guest)
        run('systemd-run', '--user', '--collect', '--property=ExitType=cgroup',
            '--unit=blockuntu-acceptance-gui', 'gtk-launch', 'local.blockuntu.gui')
        find('Welcome to BlocKuntu', 'dialog', seconds=60)
        leave_desktop_overview(args.guest)
        focus_app()
        press_observed('Get started', lambda: not nodes(role='dialog', name='Welcome to BlocKuntu'))
        wait(lambda: rpc('status')['enforcement_state'] == 'active', 60)
        status = rpc('tier1_edit_status')
        assert status['credential_configured'] and status['protected_access_open']
        assert status['protected_access_mode'] == 'all_time' and not status['active']
        result['protected_access_mode'] = status['protected_access_mode']
        result['clock_integrity_at_start'] = status['clock_integrity']
        result['initial_policy'] = rpc('config_snapshot')
        save()

        for tier, tag in (('hard', 'one'), ('scheduled_block', 'two'), ('controlled_access', 'three')):
            item = {'id': 'acceptance-protected-' + tag,
                    'name': 'Acceptance Tier1 ' + tag, 'tier': tier, 'mode': 'blocklist',
                    'enabled': True, 'schedule_ids': [],
                    'patterns': [{'kind': 'exact_url', 'value': 'http://web.blockuntu.test:18080/path/blocked'}]}
            rpc('upsert_site_list', {'rule': item})
        hard = rule('acceptance-protected-one')
        case('TIER1-NO-KEY', lambda: locked(hard, 1))

        def password():
            return wait(lambda: next((n for n in nodes(role='password text', showing=False)
                                     if n.name.startswith('Tier 1 edit key')), None), 15)

        def type_key(slot):
            node = password()
            assert enabled(node) and node.queryComponent().grabFocus()
            # The host resolves a fixed credential slot privately; no key value
            # is written into the guest request or host result files.
            host_action('type_credential', slot=slot)
            completion = json.loads((WORK / 'host-action-done.json').read_text())
            expected = completion['typed_characters']
            assert isinstance(expected, int) and 1 <= expected <= 200
            wait(lambda: password().queryText().characterCount == expected, 15)
            result.setdefault('credential_input_lengths', []).append({
                'slot': slot, 'expected': expected,
                'observed': password().queryText().characterCount})
            press('Unlock 5 min')

        def invalid():
            open_settings('Protected changes and uninstall')
            before = rpc('config_snapshot')
            type_key('invalid')
            wait(lambda: 'Tier 1 edit credential does not match' in texts(), 30)
            assert not rpc('tier1_edit_status')['active']
            assert rpc('config_snapshot') == before
            return {**locked(hard, 1), 'gui_invalid_key_error': True, 'unlock_inactive': True}

        case('DES-PROTECT-004', invalid)

        def valid():
            open_settings('Protected changes and uninstall')
            type_key('tier1')
            wait(lambda: 'Protected list edits unlocked for 5 minutes.' in texts(), 30)
            granted = rpc('tier1_edit_status')
            assert granted['active'] and granted['protected_access_open']
            assert 270 <= granted['remaining_seconds'] <= 300
            result['grant'] = {k: granted[k] for k in ('expires_at', 'remaining_seconds', 'protected_access_open')}
            show(hard, 1)
            wait(lambda: enabled(field('Name')), 30)
            edit(field('Name'), 'Acceptance Tier1 edited')
            editable = wait(lambda: entries(table=True, showing=False), 15)
            edit(editable[0], 'http://web.blockuntu.test:18080/path/free')
            press('Save')
            find('Save this Tier 1 blocklist?', 'dialog', visible=True)
            click('Save Tier 1 list')
            changed = wait(lambda: rule(hard['id']) if rule(hard['id'])['name'] == 'Acceptance Tier1 edited' else None, 30)
            assert changed['patterns'][0]['value'] == 'http://web.blockuntu.test:18080/path/free'
            assert changed['tier'] == 'hard'
            host_action('screenshot', name='tier1-authorized-edit')
            return {'gui_key_granted': True, 'gui_save_warning_confirmed': True,
                    'gui_name_and_pattern_saved': True, 'saved_rule': changed,
                    'expires_at': granted['expires_at']}

        case('DES-PROTECT-005', valid)

        def cross_tier():
            assert rpc('tier1_edit_status')['active']
            proofs = []
            for tier, tag in ((2, 'two'), (3, 'three')):
                item = rule('acceptance-protected-' + tag)
                session = rpc('start_detox', {'name': 'Acceptance protected boundary',
                    'duration_minutes': 2, 'site_rule_ids': [item['id']]})['session']
                proof = locked(item, tier)
                assert rpc('tier1_edit_status')['active'], 'Tier 1 grant expired before boundary check'
                proofs.append({'tier': tier, 'rule_id': item['id'], 'session': session,
                    'checked_at': dt.datetime.now(dt.timezone.utc).isoformat(), **proof})
            host_action('screenshot', name='tier1-tier-boundary')
            return {'grant_active_for_both_checks': True, 'tiers': proofs}

        case('DES-PROTECT-007', cross_tier)

        def expiry():
            expires = dt.datetime.fromisoformat(result['grant']['expires_at'].replace('Z', '+00:00'))
            before_wait = time.monotonic()
            seconds = max(0, (expires - dt.datetime.now(dt.timezone.utc)).total_seconds())
            wait(lambda: dt.datetime.now(dt.timezone.utc) > expires and not rpc('tier1_edit_status')['active'], seconds + 30)
            assert not rpc('tier1_edit_status')['active']
            proof = locked(rule(hard['id']), 1)
            host_action('screenshot', name='tier1-expired-edit')
            return {**proof, 'natural_expiry': True, 'unlock_inactive': True,
                    'waited_seconds': round(time.monotonic() - before_wait, 3),
                    'observed_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'expires_at': expires.isoformat()}

        case('DES-PROTECT-006', expiry)
        result['clock_integrity_at_end'] = rpc('tier1_edit_status')['clock_integrity']
        result['status'] = 'pass'
    except Exception as error:
        result.update(status='fail', error=redact(str(error)))
        # Welcome may contain recovery keys; never capture it on setup failure.
        if hard is not None:
            (WORK / 'protected-failure.json').write_text(json.dumps(dump('blockuntu-gui'), indent=2))
    finally:
        save()
    return int(result['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
