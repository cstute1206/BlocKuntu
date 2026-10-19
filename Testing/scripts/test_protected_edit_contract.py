import copy
import unittest

from protected_edit_contract import CASE_IDS, input_keys, validate_action, verify_guest


def accepted():
    locked = {k: True for k in ('rpc_rejected', 'policy_unchanged',
              'gui_name_disabled', 'gui_saved_pattern_disabled')}
    expires = '2026-10-03T12:05:00+00:00'
    evidence = [locked, {**locked, 'gui_invalid_key_error': True},
        {'gui_key_granted': True, 'gui_save_warning_confirmed': True,
         'gui_name_and_pattern_saved': True, 'expires_at': expires,
         'saved_rule': {'tier': 'hard', 'name': 'Acceptance Tier1 edited',
             'patterns': [{'value': 'http://web.blockuntu.test:18080/path/free'}]}},
        {'grant_active_for_both_checks': True,
         'tiers': [{**locked, 'tier': tier, 'rule_id': str(tier),
                    'checked_at': '2026-10-03T12:01:00+00:00',
                    'session': {'site_rule_ids': [str(tier)],
                        'starts_at': '2026-10-03T12:00:45+00:00',
                        'ends_at': '2026-10-03T12:02:45+00:00'}} for tier in (2, 3)]},
        {**locked, 'natural_expiry': True, 'unlock_inactive': True,
         'expires_at': expires, 'observed_at': '2026-10-03T12:05:02+00:00'}]
    return {'status': 'pass', 'ordinary_uid': 1000, 'protected_access_mode': 'all_time',
            'clock_integrity_at_start': {'state': 'ok', 'boot_id': 'real-boot'},
            'clock_integrity_at_end': {'state': 'ok', 'boot_id': 'real-boot'},
            'grant': {'expires_at': expires, 'remaining_seconds': 298},
            'cases': [{'id': cid, 'status': 'pass', 'evidence': proof}
                      for cid, proof in zip(CASE_IDS, evidence)]}


class ProtectedEditEvidenceTests(unittest.TestCase):
    def test_evidence_cannot_be_reused_for_another_distribution(self):
        for guest in ('ubuntu','fedora','cachyos'):
            result=accepted();result['guest']=guest
            self.assertEqual(verify_guest(result,expected_guest=guest),5)
            with self.assertRaises(ValueError):
                verify_guest(result,expected_guest='cachyos' if guest!='cachyos' else 'fedora')

    def test_complete_ordinary_user_journey(self):
        self.assertEqual(verify_guest(accepted()), 5)

    def test_partial_duplicate_and_failed_cases_are_not_acceptance(self):
        for mutation in ('partial', 'duplicate', 'failed', 'root'):
            with self.subTest(mutation=mutation):
                result = accepted()
                if mutation == 'partial': result['cases'].pop()
                elif mutation == 'duplicate': result['cases'][1] = copy.deepcopy(result['cases'][0])
                elif mutation == 'failed': result['cases'][2]['status'] = 'fail'
                else: result['ordinary_uid'] = 0
                with self.assertRaises(ValueError): verify_guest(result)

    def test_valid_rpc_grant_without_gui_save_is_not_acceptance(self):
        result = accepted()
        result['cases'][2]['evidence']['gui_name_and_pattern_saved'] = False
        with self.assertRaises(ValueError): verify_guest(result)

    def test_inactive_wrong_tier_and_accepted_weakening_are_rejected(self):
        for mutation in ('inactive', 'wrong_tier', 'accepted', 'ended'):
            with self.subTest(mutation=mutation):
                result = accepted()
                proof = result['cases'][3]['evidence']['tiers'][1]
                if mutation == 'inactive': proof['session']['site_rule_ids'] = []
                elif mutation == 'wrong_tier': proof['tier'] = 2
                elif mutation == 'accepted': proof['rpc_rejected'] = False
                else: proof['checked_at'] = '2026-10-03T12:03:00+00:00'
                with self.assertRaises(ValueError): verify_guest(result)

    def test_expiry_must_follow_original_real_five_minute_grant(self):
        for mutation in ('early', 'other_grant', 'short', 'test_clock', 'tampered'):
            with self.subTest(mutation=mutation):
                result = accepted()
                proof = result['cases'][-1]['evidence']
                if mutation == 'early': proof['observed_at'] = '2026-10-03T12:04:00+00:00'
                elif mutation == 'other_grant': proof['expires_at'] = '2026-10-03T12:04:30+00:00'
                elif mutation == 'short': result['grant']['remaining_seconds'] = 30
                elif mutation == 'test_clock': result['clock_integrity_at_end']['boot_id'] = 'test'
                else: result['clock_integrity_at_end']['state'] = 'tampered'
                with self.assertRaises(ValueError): verify_guest(result)

    def test_actions_keep_keys_out_of_requests_and_reject_arbitrary_input(self):
        base = {'id': 'a' * 32, 'action': 'type_credential', 'slot': 'tier1'}
        self.assertEqual(validate_action(base), base)
        escape = {'id': 'a' * 32, 'action': 'keys', 'keys': ['KEY_ESC']}
        self.assertEqual(validate_action(escape), escape)
        for action in (
                {**base, 'slot': '/etc/shadow'}, {**base, 'value': 'secret'},
                {'id': 'a' * 32, 'action': 'type_form', 'value': 'BLOCKUNTU-TIER1-EDIT-TEST'},
                {'id': 'a' * 32, 'action': 'screenshot', 'name': 'welcome'},
                {'id': 'a' * 32, 'action': 'keys', 'keys': ['KEY_LEFTCTRL', 'KEY_LEFTALT', 'KEY_DELETE']}):
            with self.subTest(action=action), self.assertRaises(ValueError): validate_action(action)
        with self.assertRaises(ValueError): input_keys('arbitrary shell input')
        self.assertTrue(input_keys('Acceptance Tier1 edited'))


if __name__ == '__main__':
    unittest.main()
