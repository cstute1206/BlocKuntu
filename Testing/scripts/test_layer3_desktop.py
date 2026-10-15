"""Evidence and selection boundaries for focused native desktop journeys."""
import copy
import importlib.util
from pathlib import Path
import unittest
from layer3_contract import build_plan, load_contract
from layer3_runtime import execution_plans, GUI_EDIT_CASES, PRIVATE_CASES, HELPER_CASES
from layer3_group_runner import desktop_assertion_valid
from layer3_groups import build_execution_groups

GUI_SCENARIOS=['website','application','website-blocklist-mode','website-allowlist-mode',
               'application-blocklist-mode','application-allowlist-mode']


class DesktopTests(unittest.TestCase):
    def test_cross_guest_groups_use_their_own_smoke_and_clone_identity(self):
        registry,matrix=load_contract()
        for guest in ('fedora','cachyos'):
            for ids,scenarios,count in ((GUI_EDIT_CASES,GUI_SCENARIOS,12),(HELPER_CASES,None,2)):
                _,plan=execution_plans(registry,matrix,guest=guest,suite='core',case_ids=sorted(ids),scenarios=scenarios)
                grouped=build_execution_groups(plan)
                self.assertEqual(grouped['runnable_group_count'],1)
                self.assertEqual(grouped['required_observation_count'],count)
                row=grouped['groups'][0]
                self.assertEqual(row['guest'],guest)
                self.assertTrue(row['group_id'].startswith(guest+':targeted-'))
                self.assertTrue(all(oid.startswith(guest+':') for oid in row['observation_ids']))

    def test_gui_form_input_is_limited_to_fixture_names_and_matchers(self):
        spec=importlib.util.spec_from_file_location('desktop_form_vm',Path(__file__).with_name('vm-acceptance.py'))
        vm=importlib.util.module_from_spec(spec);spec.loader.exec_module(vm)
        for value in ('Acceptance GUI website blocklist','/home/akhi/Testing/layer3/bk-test-app-b',
                      'bk-test-block','http://web.blockuntu.test:18080/path/free'):
            self.assertTrue(vm.form_keys(value))
            vm.validate_host_action({'id':'a'*32,'action':'type_form','value':value})
        self.assertIn(('KEY_LEFTSHIFT','KEY_A'),vm.form_keys('Acceptance GUI website blocklist'))
        for value in ('shell;reboot','/etc/blockuntu/config.toml','https://example.com','Acceptance GUI\nwebsite',None):
            with self.assertRaises(ValueError):vm.form_keys(value)

    def test_browser_owned_settings_are_narrowly_whitelisted(self):
        spec=importlib.util.spec_from_file_location('desktop_vm',Path(__file__).with_name('vm-acceptance.py'))
        vm=importlib.util.module_from_spec(spec);spec.loader.exec_module(vm)
        vm.url_keys('chrome://extensions/?id='+'a'*32)
        for url in ('chrome://settings','chrome://extensions/?id='+'a'*31,
                    'chrome://extensions/?id='+'z'*32,'https://example.com'):
            with self.assertRaises(ValueError):vm.url_keys(url)
        vm.validate_host_action({'id':'a'*32,'action':'keys','keys':['KEY_LEFTCTRL','KEY_LEFTSHIFT','KEY_P']})
        vm.validate_host_action({'id':'a'*32,'action':'keys','keys':['KEY_LEFTCTRL','KEY_1']})
        with self.assertRaises(ValueError):
            vm.validate_host_action({'id':'a'*32,'action':'keys','keys':['KEY_LEFTCTRL','KEY_LEFTALT','KEY_DELETE']})

    def test_gui_subset_is_explicit_and_other_variants_remain_nonexecutable(self):
        registry,matrix=load_contract()
        for guest in ('ubuntu', 'fedora', 'cachyos'):
            _,p=execution_plans(registry,matrix,guest=guest,suite='core',case_ids=sorted(GUI_EDIT_CASES),scenarios=GUI_SCENARIOS)
            self.assertEqual(p['required_count'],12)
            self.assertTrue(all(row['browser_package'] is None and row['guest']==guest for row in p['observations']))
        with self.assertRaises(ValueError):
            execution_plans(registry,matrix,guest='ubuntu',suite='core',case_ids=sorted(GUI_EDIT_CASES))

    def test_unknown_scenario_and_missing_case_cannot_silently_project_coverage(self):
        registry,matrix=load_contract()
        for scenarios in ([],['missing'],['website','application','missing'],['website-blocklist-mode']):
            with self.assertRaises(ValueError):
                build_plan(registry,matrix,suite='core',guests=['ubuntu'],case_ids=sorted(GUI_EDIT_CASES),scenarios=scenarios)

    def test_private_and_helper_groups_have_distinct_family_dimensions(self):
        registry,matrix=load_contract()
        for guest in ('ubuntu', 'fedora', 'cachyos'):
            smoke,private=execution_plans(registry,matrix,guest=guest,suite='core',case_ids=sorted(PRIVATE_CASES))
            self.assertEqual(smoke['required_count'],21)
            self.assertEqual(private['required_count'],12)
            self.assertEqual({row['guest'] for row in private['observations']},{guest})
            self.assertEqual({row['browser_package'] for row in private['observations']},
                             {'firefox-native','chromium-native' if guest=='cachyos' else 'chrome-native'})
            self.assertTrue(all(row['browser_package']=='firefox-native' for row in private['observations'] if row['scenario']=='private'))
            _,helper=execution_plans(registry,matrix,guest=guest,suite='core',case_ids=sorted(HELPER_CASES))
            self.assertEqual(helper['required_count'],2)
            self.assertTrue(all(row['browser_package'] is None and row['guest']==guest for row in helper['observations']))

    def test_private_port_does_not_enable_other_browser_cells_or_partial_case_group(self):
        registry,matrix=load_contract()
        for guest in ('fedora','cachyos'):
            with self.subTest(guest=guest), self.assertRaises(ValueError):
                execution_plans(registry,matrix,guest=guest,suite='core',case_ids=['VM-BR-004'])
            with self.subTest(guest=guest), self.assertRaises(ValueError):
                execution_plans(registry,matrix,guest=guest,suite='core',case_ids=sorted(PRIVATE_CASES),browsers=['firefox-flatpak'])

    def test_gui_save_needs_actual_enforcement_and_warning(self):
        evidence={'rule_id':'one','gui_created':True,'gui_save_persisted':True,'stricter_enforced':True,'target_was_accessible':True,'visible_named_block':True,'warning_confirmed':True}
        self.assertTrue(desktop_assertion_valid('VM-EDIT-003','website',evidence))
        for key in ('visible_named_block','warning_confirmed','gui_save_persisted','target_was_accessible'):
            bad={**evidence,key:False}
            self.assertFalse(desktop_assertion_valid('VM-EDIT-003','website',bad))
        self.assertFalse(desktop_assertion_valid('VM-EDIT-003','application',evidence))

    def test_application_gui_tightening_must_preserve_unrelated_session_entries(self):
        evidence={'rule_id':'one',**{k:True for k in ('gui_created','gui_save_persisted','stricter_enforced',
            'target_was_accessible','warning_confirmed','process_terminated','session_entries_preserved')},
            'termination_seconds':5,'process_identity':{'pid':123,'exe':'/test/bk-test-app-b','start_ticks':'12345'}}
        self.assertTrue(desktop_assertion_valid('VM-EDIT-003','application',evidence))
        self.assertFalse(desktop_assertion_valid('VM-EDIT-003','application',{**evidence,'session_entries_preserved':False}))

    def test_policy_snapshot_or_generic_rpc_failure_is_not_a_protected_gui_edit(self):
        evidence={'rule_id':'one','gui_control_disabled':True,'daemon_rejected':True,'policy_unchanged':True,
                  'rejection':"list 'one' is covered by an active detox session and cannot be edited"}
        self.assertTrue(desktop_assertion_valid('VM-EDIT-002','website',evidence))
        self.assertFalse(desktop_assertion_valid('VM-EDIT-002','website',{**evidence,'gui_control_disabled':False}))
        self.assertFalse(desktop_assertion_valid('VM-EDIT-002','website',{**evidence,'rejection':'internal RPC error'}))

    def test_regular_block_is_not_private_block_and_consent_is_required(self):
        regular={'rule_id':'one','regular_blocked':True,'allowed_access':True}
        self.assertTrue(desktop_assertion_valid('VM-WAL-013','regular',regular))
        self.assertFalse(desktop_assertion_valid('VM-WAL-013','private',regular))
        private={'rule_id':'one','private_window_opened':True,'private_start_page_observed':True,'private_document_verified':True,'private_blocked':True,
                 'allowed_access':True,'explicit_extension_consent':True}
        self.assertTrue(desktop_assertion_valid('VM-WAL-013','incognito-extension-permitted',private))
        self.assertFalse(desktop_assertion_valid('VM-WAL-013','incognito-extension-permitted',{**private,'explicit_extension_consent':False}))
        self.assertFalse(desktop_assertion_valid('VM-WAL-013','incognito-extension-permitted',{**private,'private_document_verified':False}))

    def test_disabled_policy_requires_actual_window_attempt_and_setting_locks(self):
        evidence={'rule_id':'one','configured_mode':'disabled','policy_incognito_mode':1,
                  'private_creation_prevented':True,'regular_blocked':True,'gui_settings_locked':True,
                  'daemon_settings_rejected':True,'settings_unchanged':True}
        self.assertTrue(desktop_assertion_valid('VM-BR-004','incognito-disabled',evidence))
        for key in ('private_creation_prevented','gui_settings_locked','daemon_settings_rejected'):
            self.assertFalse(desktop_assertion_valid('VM-BR-004','incognito-disabled',{**evidence,key:False}))

    def test_helper_identity_does_not_transfer_and_control_path_must_survive(self):
        def identity(pid,name):return {'pid':pid,'exe':'/test/'+name,'start_ticks':str(pid*100)}
        evidence={'rule_id':'one','parent_identity':identity(1,'bk-test-parent'),'helper_identity':{**identity(2,'bk-test-app-b'),'parent_pid':1},
                  'native_identity':identity(3,'blockuntu-native'),'gui_identity':identity(4,'blockuntu-gui'),
                  'restored_helper_identity':identity(5,'bk-test-app-b'),'termination_seconds':5,
                  'desktop_user_uid':1000,'second_account_uid':1001,'second_user_termination_seconds':5,
                  'second_user_parent_identity':{**identity(6,'bk-test-parent'),'uid':1001},
                  'second_user_helper_identity':{**identity(7,'bk-test-app-b'),'uid':1001},
                  'second_user_restored_helper_identity':{**identity(8,'bk-test-app-b'),'uid':1001},
                  **{k:True for k in ('helper_terminated','parent_survived','native_survived','gui_survived','protected_paths_omitted',
                      'restored_helper_survived','gui_health_available_during_restriction','rpc_available_during_restriction',
                      'second_user_enforced','second_user_control_survived','second_user_restored_helper_survived')}}
        self.assertTrue(desktop_assertion_valid('VM-AAL-005','baseline',evidence))
        for bad in ({'gui_survived':False},{'protected_paths_omitted':False},{'termination_seconds':31},
                    {'helper_identity':{**evidence['helper_identity'],'parent_pid':99}},
                    {'second_account_uid':1000},{'second_user_enforced':False},
                    {'second_user_helper_identity':{**evidence['second_user_helper_identity'],'uid':0}}):
            self.assertFalse(desktop_assertion_valid('VM-AAL-005','baseline',{**evidence,**bad}))


if __name__=='__main__':unittest.main()
