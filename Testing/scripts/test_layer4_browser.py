"""Meaningful P4.3 action, installed-timing and bounded recovery guard checks."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

from layer4_browser_contract import CASES, SCENARIOS, TIMING_NAMES, crash_pause_seconds, verify_crash_pause, diagnostic_targets, diagnostic_witness, installed_timing, validate_action, verify_guest

HERE = Path(__file__).parent
def load(name, path):
    spec = importlib.util.spec_from_file_location(name,path)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module
runner = load('browser_runner_test',HERE/'browser-acceptance.py')
auditor = load('browser_auditor_test',HERE/'check_browser_acceptance.py')
sys.path.insert(0,str(HERE.parent/'fixtures/layer4'))
ops = load('browser_ops_test',HERE.parent/'fixtures/layer4/browser_ops.py')
SOURCE = '\n'.join('const '+name+' = '+str(value)+';' for name,value in zip(TIMING_NAMES,(5000,75000,3000,5000,3,10000,30000)))


class BrowserBoundaries(unittest.TestCase):
    def test_crash_pause_covers_late_request_and_timeout_without_accepting_slow_success(self):
        timing = installed_timing(SOURCE)
        # A request beginning near the end of a five-second cycle needs three
        # further seconds to time out. The previous six-second pause did not.
        self.assertGreater(crash_pause_seconds([timing]), 5 + 3)
        with self.assertRaises(ValueError):
            crash_pause_seconds([{**timing, 'REVALIDATE_TABS_INTERVAL_MS': 10000}])
        with self.assertRaises(ValueError): crash_pause_seconds([])
        slow = {'observed_at': '2026-10-11T16:12:10Z', 'kind': 'rpc_slow', 'message': 'elapsed_ms=2871'}
        log = 'kind="runtime_diagnostic" target=Some("chrome_extension") details=Some(' + json.dumps(json.dumps(slow)) + ')'
        self.assertFalse(diagnostic_witness(log, 'chrome', '2026-10-11T16:12:03+00:00'))

    def test_crash_pause_requires_actual_root_elapsed_evidence(self):
        timings = [installed_timing(SOURCE)]
        fault = {'transient_pre_pause_seconds': 12, 'transient_pre_pause_elapsed_seconds': 12.01}
        verify_crash_pause(fault, timings)
        for elapsed in (None, True, 6, 11.99, float('nan'), 240):
            with self.assertRaises(ValueError):
                verify_crash_pause({**fault, 'transient_pre_pause_elapsed_seconds': elapsed}, timings)
        with self.assertRaises(ValueError):
            verify_crash_pause({**fault, 'transient_pre_pause_seconds': 6}, timings)

    def test_installed_constants_parsed_without_source_defaults(self):
        self.assertEqual(installed_timing(SOURCE)['HEARTBEAT_TIMEOUT_MS'],75000)
        with self.assertRaises(ValueError): installed_timing(SOURCE.replace('75000','0'))
        with self.assertRaises(ValueError): installed_timing(SOURCE.replace('75000','999000'))
        with self.assertRaises(ValueError): installed_timing(SOURCE+'\nconst RPC_TIMEOUT_MS = 3000;')
        with self.assertRaises(ValueError): installed_timing('')

    def test_arbitrary_actions_paths_commands_signals_and_urls_rejected(self):
        good={'id':'a'*32,'action':'fault','name':'outage'}
        self.assertEqual(validate_action(good,'outage'),good)
        for invalid in ({**good,'name':'crash'},{**good,'pid':1},{**good,'name':'/etc/passwd'},
                        {**good,'action':'shell'},{**good,'id':'../x'},
                        {'id':'a'*32,'action':'type_url','url':'https://example.com'},
                        {'id':'a'*32,'action':'keys','keys':['KEY_LEFTCTRL','KEY_Q']}):
            with self.assertRaises(ValueError): validate_action(invalid,'outage')

    def test_invalid_action_or_base_guard_fails_before_guest_mutation(self):
        action={'id':'a'*32,'action':'fault','name':'outage'}
        with patch.object(runner,'checked') as ssh, patch.object(runner.vm,'guarded',side_effect=ValueError('base or changed identity')):
            with self.assertRaises(ValueError): runner.handle_action({},Path('/unused'),action,'outage')
            ssh.assert_not_called()
        with patch.object(runner,'checked') as ssh, patch.object(runner.vm,'guarded') as guard:
            with self.assertRaises(ValueError): runner.handle_action({},Path('/unused'),{**action,'name':'crash'},'outage')
            ssh.assert_not_called();guard.assert_not_called()

    def test_unknown_root_fault_does_not_inspect_or_signal(self):
        with patch.object(ops,'inspect') as inspect,patch.object(ops.os,'kill') as kill:
            with self.assertRaises(ValueError): ops.fault('arbitrary')
            inspect.assert_not_called();kill.assert_not_called()

    def test_recovery_cannot_signal_reused_daemon_pid(self):
        with tempfile.TemporaryDirectory() as directory:
            lease=Path(directory)/'lease.json';lease.write_text(json.dumps({'scenario':'outage','daemon':{'pid':77}}))
            with patch.object(ops,'LEASE',lease),patch.object(ops,'same_process',return_value=False),patch.object(ops.os,'kill') as kill:
                with self.assertRaises(ValueError):ops.restore()
                kill.assert_not_called();self.assertTrue(lease.exists())

    def test_recovery_rejects_arbitrary_manifest_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            lease=Path(directory)/'lease.json';lease.write_text(json.dumps({'scenario':'manifest','original_files':{'/etc/passwd':'x'}}))
            with patch.object(ops,'LEASE',lease),patch.object(ops,'write_target') as write:
                with self.assertRaises(ValueError):ops.restore()
                write.assert_not_called()

    def test_resume_uses_fixed_signal_on_verified_process_then_removes_lease(self):
        with tempfile.TemporaryDirectory() as directory:
            lease=Path(directory)/'lease.json';lease.write_text(json.dumps({'scenario':'outage','daemon':{'pid':77}}))
            with patch.object(ops,'LEASE',lease),patch.object(ops,'same_process',return_value=True),patch.object(ops.os,'kill') as kill, \
                    patch.object(ops,'publish'),patch.object(ops,'inspect',return_value={}):
                self.assertTrue(ops.restore()['restored']);kill.assert_called_once_with(77,ops.signal.SIGCONT)
                self.assertFalse(lease.exists())

    def test_rescue_rejects_unbounded_or_expired_deadline(self):
        with tempfile.TemporaryDirectory() as directory:
            lease=Path(directory)/'lease.json'
            for deadline in (999999999999,0):
                lease.write_text(json.dumps({'deadline_monotonic':deadline}))
                with patch.object(ops,'LEASE',lease),patch.object(ops,'restore') as restore:
                    with self.assertRaises(ValueError):ops.rescue()
                    restore.assert_not_called()

    def test_valid_rescue_that_starts_late_restores_immediately(self):
        with tempfile.TemporaryDirectory() as directory:
            lease=Path(directory)/'lease.json';lease.write_text(json.dumps({'armed_monotonic':1,'deadline_monotonic':241}))
            with patch.object(ops,'LEASE',lease),patch.object(ops.time,'monotonic',return_value=300),patch.object(ops,'restore') as restore:
                ops.rescue();restore.assert_called_once()

    def test_native_signal_delivery_is_waited_for_before_connection_witness(self):
        with tempfile.TemporaryDirectory() as directory:
            paths=tuple(str(Path(directory)/name) for name in ('firefox.json','chrome.json'))
            original=json.dumps({'path':'/usr/bin/blockuntu-native'})
            for path in paths:Path(path).write_text(original)
            before={'daemon':{'state':'S'},'files':dict.fromkeys(paths,original),'native_processes':[{'pid':101},{'pid':102}]}
            with patch.object(ops,'LEASE',Path(directory)/'lease.json'),patch.object(ops,'MANIFESTS',paths),patch.object(ops,'inspect',return_value=before), \
                    patch.object(ops,'run'),patch.object(ops,'publish'),patch.object(ops.os,'kill') as kill, \
                    patch.object(ops,'write_target',side_effect=lambda p,t:Path(p).write_text(t)), \
                    patch.object(ops,'same_process',side_effect=[True,True,True,False,False,False,False]),patch.object(ops.time,'sleep') as sleep:
                self.assertTrue(ops.fault('manifest')['connections_terminated'])
                self.assertEqual(kill.call_count,2);sleep.assert_called_once_with(.05)

    def test_only_current_fault_diagnostics_from_correct_browser_prove_interruption(self):
        detail={'observed_at':'2026-10-06T17:00:02Z','kind':'rpc_timeout','message':'timeout'}
        line='2026-10-06T17:01:00Z kind="runtime_diagnostic" target=Some("firefox_extension") details=Some('+json.dumps(json.dumps(detail))+')'
        self.assertEqual(len(diagnostic_witness(line,'firefox','2026-10-06T17:00:00+00:00')),1)
        self.assertFalse(diagnostic_witness(line,'chrome','2026-10-06T17:00:00+00:00'))
        self.assertFalse(diagnostic_witness(line,'firefox','2026-10-06T17:00:10+00:00'))
        self.assertFalse(diagnostic_witness(line.replace('rpc_timeout','heartbeat_recovered'),'firefox','2026-10-06T17:00:00+00:00'))

    def test_chromium_reported_chrome_diagnostics_require_verified_native_identity(self):
        pulse={'component':'chromium_extension','browser':'chrome','state':'active','current_session_heartbeat':True}
        def line(target,stamp):
            detail={'observed_at':stamp,'kind':'native_host_disconnected','message':'native host unavailable'}
            return '2026-10-06T17:01:00Z kind="runtime_diagnostic" target=Some("'+target+'") details=Some('+json.dumps(json.dumps(detail))+')'
        start,end='2026-10-06T17:00:00+00:00','2026-10-06T17:00:30+00:00'
        event=line('chrome_extension','2026-10-06T17:00:02Z')
        self.assertFalse(diagnostic_witness(event,'chromium',start))
        self.assertEqual(diagnostic_targets('chromium',pulse),('chromium_extension','chrome_extension'))
        self.assertEqual(len(diagnostic_witness(event,'chromium',start,pulse)),1)
        self.assertFalse(diagnostic_witness(event,'chromium','2026-10-06T17:00:10+00:00',pulse))
        log='\n'.join(line(target,stamp) for target in ('firefox_extension','chrome_extension')
                      for stamp in ('2026-10-06T17:00:02Z','2026-10-06T17:00:04Z'))
        with self.assertRaises(ValueError):
            auditor.manifest_disconnect_counts(log,start,end,('firefox','chromium'))
        pulses={'firefox':{'component':'firefox_extension','browser':'firefox','state':'active','current_session_heartbeat':True},
                'chromium':pulse}
        self.assertEqual(auditor.manifest_disconnect_counts(log,start,end,('firefox','chromium'),pulses),
                         {'firefox':2,'chromium':2})

    def test_diagnostic_alias_rejects_wrong_canonical_component_session_or_reported_browser(self):
        valid={'component':'chromium_extension','browser':'chrome','state':'active','current_session_heartbeat':True}
        for key,value in [('component','chrome_extension'),('browser','edge'),('state','missing'),
                          ('current_session_heartbeat',False),('current_session_heartbeat','true')]:
            with self.assertRaises(ValueError):
                diagnostic_targets('chromium',{**valid,key:value})
        with self.assertRaises(ValueError):diagnostic_targets('chromium',{})

    def test_manifest_audit_requires_repeated_reconnects_during_damage_for_each_browser(self):
        def line(browser, stamp, kind='native_host_disconnected'):
            detail={'observed_at':stamp,'kind':kind,'message':'native host unavailable'}
            return '2026-10-06T17:01:00Z kind="runtime_diagnostic" target=Some("'+browser+'_extension") details=Some('+json.dumps(json.dumps(detail))+')'
        start,end='2026-10-06T17:00:00+00:00','2026-10-06T17:00:30+00:00'
        valid='\n'.join(line(browser,stamp) for browser in ('firefox','chrome') for stamp in ('2026-10-06T17:00:02Z','2026-10-06T17:00:04Z'))
        self.assertEqual(auditor.manifest_disconnect_counts(valid,start,end),{'firefox':2,'chrome':2})
        for invalid in ('',valid.replace('chrome_extension','other_extension'),valid.replace('17:00:04Z','17:00:31Z'),valid.replace('17:00:04Z','16:59:50Z'),valid.replace('native_host_disconnected','heartbeat_failed')):
            with self.assertRaises(ValueError):auditor.manifest_disconnect_counts(invalid,start,end)

    def test_expected_socket_gap_is_recorded_and_later_heartbeat_can_recover(self):
        stubs={name:Mock() for name in ('accessibility','browsers','ui_controls','uninstall','guest')}
        with tempfile.TemporaryDirectory() as directory,patch.dict(sys.modules,stubs):
            stubs['guest'].WORK=Path(directory)
            journey=load('browser_journey_poll_test',HERE.parent/'fixtures/layer4/browser_recovery.py')
            stubs['browsers'].heartbeat.side_effect=[FileNotFoundError('socket gone'),{'state':'active'}]
            self.assertIsNone(journey.available_heartbeat('firefox'))
            self.assertEqual(journey.available_heartbeat('firefox'),{'state':'active'})
            trace=json.loads((Path(directory)/'p43-rpc-trace.json').read_text())
            self.assertEqual([row['rpc_succeeded'] for row in trace],[False,True])

    def test_declared_manifest_restart_recovers_without_claiming_original_processes(self):
        stubs={name:Mock() for name in ('accessibility','browsers','ui_controls','uninstall','guest')}
        with patch.dict(sys.modules,stubs):
            journey=load('browser_journey_identity_test',HERE.parent/'fixtures/layer4/browser_recovery.py')
            changed=[{'original_pid':'10','recovered_pid':'20'},{'original_pid':'11','recovered_pid':'21'}]
            proof=journey.recovery_proof('manifest',changed,True,True)
            self.assertTrue(journey.require(proof,{})['browser_identity_expected'])
            self.assertNotIn('same_browsers',proof)
            with self.assertRaises(AssertionError):journey.require(journey.recovery_proof('outage',changed,True,True),{})
            missing=[changed[0],{'original_pid':'11','recovered_pid':'0'}]
            with self.assertRaises(AssertionError):journey.require(journey.recovery_proof('manifest',missing,True,True),{})

    def test_rescue_is_armed_before_signal_and_arm_failure_does_not_pause_daemon(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(ops,'LEASE',Path(directory)/'lease.json'),patch.object(ops,'inspect',return_value={'daemon':{'state':'S'},'files':{}}), \
                    patch.object(ops,'run',side_effect=RuntimeError('systemd-run failed')),patch.object(ops.os,'kill') as kill:
                with self.assertRaises(RuntimeError):ops.fault('outage')
                kill.assert_not_called();self.assertFalse((Path(directory)/'lease.json').exists())

    def test_empty_partial_and_root_guest_results_cannot_pass(self):
        for scenario in SCENARIOS:
            for result in ({'status':'pass','guest':'ubuntu','scenario':scenario,'ordinary_uid':1000,'cases':[]},
                           {'status':'fail','guest':'ubuntu','scenario':scenario},
                           {'status':'pass','guest':'ubuntu','scenario':scenario,'ordinary_uid':0}):
                with self.assertRaises(ValueError):verify_guest(result,scenario)

    def test_failed_partial_batch_cannot_qualify(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'summary.json'
            path.write_text(json.dumps({'status':'fail','scope':'ubuntu-p43','selected_scenarios':list(SCENARIOS)}))
            with self.assertRaises(ValueError):auditor.audit([path,path],True)
            with self.assertRaises(ValueError):auditor.audit([path],True)


if __name__ == '__main__':
    unittest.main()
