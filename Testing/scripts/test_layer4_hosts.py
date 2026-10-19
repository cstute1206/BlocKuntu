"""Regression checks for new fixed hosts faults and complete P4.2 evidence."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
import threading
import unittest
import urllib.error
import urllib.request
from unittest.mock import Mock, patch

from layer4_hosts_contract import (APPEND_SENTINEL, BEGIN, CASES, END, FAULTS,
                                   HARD, SCHEDULED, SCENARIOS, domain_rows, expected_rows, external_rows,
                                   mutated_hosts, split_hosts, validate_action, verify_guest)

HERE = Path(__file__).parent
spec = importlib.util.spec_from_file_location('hosts_runner_test', HERE / 'hosts-acceptance.py')
runner = importlib.util.module_from_spec(spec); spec.loader.exec_module(runner)
spec = importlib.util.spec_from_file_location('hosts_audit_test', HERE / 'check_hosts_acceptance.py')
auditor = importlib.util.module_from_spec(spec); spec.loader.exec_module(auditor)
spec = importlib.util.spec_from_file_location('hosts_site_test', HERE.parent / 'fixtures/test-site/server.py')
site = importlib.util.module_from_spec(spec); spec.loader.exec_module(site)
sys.path.insert(0, str(HERE.parent / 'fixtures/layer4'))
spec = importlib.util.spec_from_file_location('hosts_ops_test', HERE.parent / 'fixtures/layer4/hosts_ops.py')
ops = importlib.util.module_from_spec(spec); spec.loader.exec_module(ops)

TEXT = '127.0.0.1 localhost\n# unrelated comment\n' + BEGIN + '\n0.0.0.0 hard.test\n:: hard.test\n' + END + '\n127.0.0.2 other.test\n'


class HostsBoundaries(unittest.TestCase):
    def test_managed_faults_preserve_every_external_row(self):
        block, outside = split_hosts(TEXT)
        for name in ('remove-managed', 'modify-managed'):
            changed, actual_outside = split_hosts(mutated_hosts(TEXT, name))
            self.assertEqual(outside, actual_outside)
            self.assertNotEqual(block, changed)

    def test_append_preserves_managed_and_original_external_content(self):
        before, outside = split_hosts(TEXT)
        after, changed = split_hosts(mutated_hosts(TEXT, 'append-unrelated'))
        self.assertEqual(before, after)
        expected = external_rows(outside); expected.update([APPEND_SENTINEL])
        self.assertEqual(expected, external_rows(changed))
        with self.assertRaises(ValueError):
            mutated_hosts(mutated_hosts(TEXT, 'append-unrelated'), 'append-unrelated')

    def test_duplicate_missing_embedded_and_reordered_markers_rejected(self):
        for text in (TEXT.replace(BEGIN, ''), TEXT + BEGIN + '\n' + END + '\n',
                     TEXT.replace(BEGIN, 'prefix ' + BEGIN), END + '\n' + BEGIN + '\n'):
            with self.assertRaises(ValueError): split_hosts(text)

    def test_duplicate_or_rogue_domain_rows_cannot_equal_exact_expected_set(self):
        block = split_hosts(TEXT)[0]
        self.assertEqual(domain_rows(block), expected_rows(['hard.test']))
        self.assertNotEqual(domain_rows(block + '0.0.0.0 hard.test\n'), expected_rows(['hard.test']))
        with self.assertRaises(ValueError): domain_rows(block + '127.0.0.2 rogue.test\n')

    def test_only_fixed_matching_faults_and_checkpoint_names(self):
        for scenario, fault in FAULTS.items():
            good = {'id': 'a'*32, 'action': 'fault', 'name': fault}
            self.assertEqual(validate_action(good, scenario), good)
            with self.assertRaises(ValueError): validate_action(good, 'allowlist')
            for changed in ({**good, 'name': '/etc/passwd'}, {**good, 'path': '/etc/hosts'},
                            {**good, 'action': 'shell'}, {**good, 'id': '../x'}):
                with self.assertRaises(ValueError): validate_action(changed, scenario)
        with self.assertRaises(ValueError):
            validate_action({'id': 'a'*32, 'action': 'checkpoint', 'name': '../baseline'}, 'remove')

    def test_invalid_action_never_reaches_guest_or_clone_mutation(self):
        with patch.object(runner, 'checked') as ssh, patch.object(runner.vm, 'guarded') as guard:
            with self.assertRaises(ValueError):
                runner.handle_action({}, Path('/unused'), {'id':'a'*32,'action':'fault','name':'delete-hosts'}, 'remove')
            ssh.assert_not_called(); guard.assert_not_called()

    def test_valid_fault_still_rejects_base_or_changed_clone_before_mutation(self):
        with patch.object(runner, 'checked') as ssh, patch.object(runner.vm, 'guarded', side_effect=ValueError('base/shared storage')):
            with self.assertRaises(ValueError):
                runner.handle_action({}, Path('/unused'), {'id':'a'*32,'action':'fault','name':'delete-hosts'}, 'delete')
            ssh.assert_not_called()

    def test_root_fault_witnesses_damage_and_never_stops_repair_services(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)/'hosts'; work=Path(directory)
            def command(*args, **kwargs):
                return Mock(returncode=0, stdout='--------------e------- '+str(target)+'\n' if args[0]=='lsattr' else '', stderr='')
            with patch.object(ops,'HOSTS',target),patch.object(ops,'WORK',work),patch.object(ops,'run',side_effect=command) as run, \
                    patch.object(ops.pwd,'getpwnam',return_value=Mock(pw_uid=1000,pw_gid=1000)),patch.object(ops.os,'chown'):
                for name in FAULTS.values():
                    target.write_text(TEXT)
                    result=ops.fault(name)
                    self.assertTrue(result['fault_witnessed'])
                    self.assertTrue(result['immutable_clear_witnessed'])
                    if name=='delete-hosts': self.assertIsNone(result['fault_hosts'])
                    else: self.assertEqual(result['fault_hosts'],mutated_hosts(TEXT,name))
                self.assertFalse(any(call.args[:2] in (('systemctl','stop'),('systemctl','kill')) for call in run.call_args_list))

    def test_unknown_root_fault_does_not_even_inspect_hosts(self):
        with patch.object(ops,'inspect') as inspect:
            with self.assertRaises(ValueError):ops.fault('/etc/passwd')
            inspect.assert_not_called()

    def test_json_evidence_keeps_expected_sudo_hostname_warning_separate(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(runner, 'checked') as checked:
            def answer(record, command, **kwargs):
                return Mock(returncode=0, stdout='{}' if command.startswith('cat') or ' inspect' in command else '',
                            stderr='sudo: unable to resolve host fixture\n')
            checked.side_effect = answer
            self.assertEqual(runner.collect({}, Path(directory)), [])
            json.loads((Path(directory)/'inspection.json').read_text())
            self.assertIn('unable to resolve', (Path(directory)/'inspection.json.stderr.log').read_text())

    def test_lost_or_invalid_json_checkpoint_never_counts_as_complete(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(runner, 'checked', return_value=Mock(returncode=0, stdout='{', stderr='')):
            errors = runner.collect({}, Path(directory))
            self.assertTrue(any(e['file']=='inspection.json' for e in errors))

    def test_terminal_publication_race_is_reread(self):
        terminal = {'status':'pass'}
        responses = [Mock(returncode=1,stdout=''),Mock(returncode=1,stdout=''),Mock(returncode=0,stdout='inactive'),Mock(returncode=0,stdout=json.dumps(terminal))]
        with tempfile.TemporaryDirectory() as directory, patch.object(runner, 'checked', side_effect=[Mock(),*responses]):
            self.assertEqual(runner.exercise({'run_id':'p42-x', 'template':'ubuntu'}, Path(directory),'remove'),terminal)

    def test_incomplete_reordered_or_wrong_variant_guest_results_rejected(self):
        guest={'status':'pass','guest':'ubuntu','scenario':'remove','ordinary_uid':1000,'cases':[]}
        for bad in (guest, {**guest,'ordinary_uid':0}, {**guest,'guest':'fedora'}):
            with self.assertRaises(ValueError):verify_guest(bad,'remove')
        guest['cases']=[{'id':cid,'status':'pass','variant':'remove','evidence':{}} for cid in CASES['remove']]
        with self.assertRaisesRegex(ValueError,'Incomplete'):verify_guest(guest,'remove')
        guest['cases'][0]['variant']='delete'
        with self.assertRaisesRegex(ValueError,'Missing'):verify_guest(guest,'remove')

    def test_partial_or_failed_batch_cannot_qualify(self):
        with tempfile.TemporaryDirectory() as directory:
            p=Path(directory)/'summary.json'
            for summary in ({'status':'pass','guest':'ubuntu','scope':'ubuntu-p42','selected_scenarios':['remove']},
                            {'status':'fail','scope':'ubuntu-p42','selected_scenarios':list(SCENARIOS)}):
                p.write_text(json.dumps(summary))
                with self.assertRaises(ValueError):auditor.audit([p])
            with self.assertRaises(ValueError):auditor.audit([p],True)

    def test_loopback_http_fixture_cannot_mistake_zero_address_for_allowed_access(self):
        server=site.FixtureServer(('127.0.0.2',0),site.FixtureHandler)
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
        try:
            with opener.open(f'http://127.0.0.2:{server.server_port}/free',timeout=2) as response:
                self.assertIn('BlocKuntu test route /free',response.read().decode())
            with self.assertRaises(urllib.error.URLError):
                opener.open(f'http://0.0.0.0:{server.server_port}/free',timeout=2)
        finally:
            server.shutdown();thread.join(timeout=2);server.server_close()


class HostsAuditTests(unittest.TestCase):
    """Isolate archive/root/safety audits; guest-contract rejection is tested above."""
    def make_batch(self, root, batch):
        def state(domains):
            rows=''.join(ip+' '+name+'\n' for name in domains for ip in ('0.0.0.0','::'))
            return {'hosts':'127.0.0.1 localhost\n'+BEGIN+'\n'+rows+END+'\n',
                    'hosts_attributes':{'stdout':'----i---------e------- /etc/hosts\n','returncode':0}}
        hard=(HARD,'www.'+HARD)
        runs=[]
        def write(path, data):
            path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(data))
        for scenario in SCENARIOS:
            run_id=batch+'-'+scenario
            directory=root/'results'/run_id/'ubuntu/acceptance'
            directory.mkdir(parents=True)
            uuid='uuid-'+run_id
            result={'status':'pass','guest':'ubuntu','scope':'ubuntu-p42-'+scenario,'run_id':run_id,'clone_uuid':uuid,
                    'base_integrity_verified':True,'clone_removed':True,'console_access_verified':True,
                    'package_commit':'a'*40,'package_sha256':'b'*64,'installed_version':'0.2.0-1',
                    'required_passes':len(CASES[scenario])}
            write(directory/'result.json',result)
            write(directory/'guest-results.json',{'scenario':scenario})
            write(directory/'package-report.json',{'component':'deb','passed':True,'build_provenance_verified':True,'commit':'a'*40,
                'artifacts':[{'filename':'blockuntu.deb','sha256':'b'*64}],'package_metadata':{'version':'0.2.0-1'}})
            write(root/'runtime/vm-phase0'/run_id/'ubuntu/owner.json',{'uuid':uuid,'stage':'removed','template':'ubuntu',
                'run_id':run_id,'paths':[str(root/(run_id+'.qcow2'))]})
            manifest={}
            for name in runner.SOURCE_PATHS:
                file=directory/'harness'/name;file.parent.mkdir(parents=True,exist_ok=True);file.write_text('captured source '+name)
                manifest[name]=auditor.hashlib.sha256(file.read_bytes()).hexdigest()
            write(directory/'harness-files.json',manifest)
            for name in ['baseline/baseline.json','baseline/export.json','baseline/seeded.json','inspection.json',
                         'collector.json','network.json']:
                write(directory/name,{})
            for name in ['journal.log','collector.log','suite.log','p42-console-preflight.png']:
                (directory/name).write_text('fixture')
            if scenario!='allowlist':
                write(directory/'before-fault/inspection.json',state(hard))
                after = state(hard)
                repaired = {}
                if scenario == 'append':
                    after['hosts'] += '\n' + APPEND_SENTINEL + '\n'
                    sample = {'native': {'returncode': 0, 'addresses': ['127.0.0.43']},
                              'files': {'returncode': 0, 'addresses': ['127.0.0.43']},
                              'nss': {'errno': None, 'addresses': ['127.0.0.43']}}
                    repaired['details'] = {'sentinel_resolution': {'expected_address': '127.0.0.43',
                        'timeout_seconds': 30, 'settled': True, 'samples': [sample]}}
                write(directory/'after-fault/inspection.json', after)
                write(directory/'p42-repaired.json', repaired)
                write(directory/'p42-fault.json',{'name':FAULTS[scenario],'fault_witnessed':True,'immutable_clear_witnessed':True,
                    'before':state(hard),'fault_hosts':None if scenario=='delete' else mutated_hosts(state(hard)['hosts'],FAULTS[scenario])})
            if scenario=='remove':
                write(directory/'p42-schedule.json',{})
                for stage,domains in [('inactive',hard),('active',(*hard,SCHEDULED)),('ended',hard)]:
                    write(directory/('schedule-'+stage+'/inspection.json'),state(domains))
            if scenario=='allowlist':
                write(directory/'p42-allowlist.json',{})
                write(directory/'allowlist-active/inspection.json',state(()))
            runs.append({'scenario':scenario,'status':'pass','evidence':str(directory)})
        summary=root/'results'/batch/'summary.json'
        write(summary,{'status':'pass','guest':'ubuntu','scope':'ubuntu-p42','selected_scenarios':list(SCENARIOS),
            'package_commit':'a'*40,'package_sha256':'b'*64,'runs':runs})
        return summary

    def test_two_complete_distinct_archives_can_qualify(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);a=self.make_batch(root,'a');b=self.make_batch(root,'b')
            with patch.object(auditor,'ROOT',root),patch.object(auditor,'verify_guest',side_effect=lambda guest,scenario,guest_name:len(CASES[scenario])):
                result=auditor.audit([a,b],True)
            self.assertTrue(result['adapter_qualified']);self.assertEqual(result['required_passes'],22)

    def test_false_root_witness_or_wrong_checkpoint_cannot_be_hidden_by_guest_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);a=self.make_batch(root,'a')
            p=root/'results/a-remove/ubuntu/acceptance/p42-fault.json';original=p.read_text()
            data=json.loads(original);data['fault_witnessed']=False;p.write_text(json.dumps(data))
            with patch.object(auditor,'ROOT',root),patch.object(auditor,'verify_guest',side_effect=lambda guest,scenario,guest_name:len(CASES[scenario])):
                with self.assertRaisesRegex(ValueError,'root fault'):auditor.audit([a])
                p.write_text(original)
                q=root/'results/a-remove/ubuntu/acceptance/schedule-active/inspection.json'
                d=json.loads(q.read_text());d['hosts']=d['hosts'].replace('0.0.0.0 '+SCHEDULED+'\n','');q.write_text(json.dumps(d))
                with self.assertRaisesRegex(ValueError,'checkpoint'):auditor.audit([a])

    def test_missing_evidence_and_changed_archive_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);a=self.make_batch(root,'a')
            bundle=root/'results/a-remove/ubuntu/acceptance'
            with patch.object(auditor,'ROOT',root),patch.object(auditor,'verify_guest',side_effect=lambda guest,scenario,guest_name:len(CASES[scenario])):
                p=bundle/'harness/scripts/hosts-acceptance.py';original=p.read_text();p.write_text('altered')
                with self.assertRaisesRegex(ValueError,'checksum'):auditor.audit([a])
                p.write_text(original);(bundle/'p42-repaired.json').unlink()
                with self.assertRaisesRegex(ValueError,'evidence missing'):auditor.audit([a])

    def test_same_batch_cannot_count_as_two_fresh_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);a=self.make_batch(root,'a')
            with patch.object(auditor,'ROOT',root),patch.object(auditor,'verify_guest',side_effect=lambda guest,scenario,guest_name:len(CASES[scenario])):
                with self.assertRaisesRegex(ValueError,'distinct fresh clone'):auditor.audit([a,a],True)

    def test_append_needs_actual_settled_native_resolution_not_just_guest_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);a=self.make_batch(root,'a')
            p=root/'results/a-append/ubuntu/acceptance/p42-repaired.json'
            original=json.loads(p.read_text())
            with patch.object(auditor,'ROOT',root),patch.object(auditor,'verify_guest',side_effect=lambda guest,scenario,guest_name:len(CASES[scenario])):
                for mismatch in ('missing', 'stale', 'api-error', 'missing-files', 'native-error'):
                    data=json.loads(json.dumps(original))
                    proof=data['details']['sentinel_resolution']
                    if mismatch=='missing':proof['samples']=[]
                    elif mismatch=='stale':proof['samples'][-1]['native']['addresses']=['127.0.0.2']
                    elif mismatch=='api-error':proof['samples'][-1]['nss']['errno']=-3
                    elif mismatch=='missing-files':proof['samples'][-1]['files']['addresses']=[]
                    elif mismatch=='native-error':proof['samples'][-1]['native']['returncode']=2
                    p.write_text(json.dumps(data))
                    with self.assertRaisesRegex(ValueError,'sentinel resolver'):auditor.audit([a])

    def test_append_needs_the_external_row_in_the_independent_checkpoint(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);a=self.make_batch(root,'a')
            p=root/'results/a-append/ubuntu/acceptance/after-fault/inspection.json'
            data=json.loads(p.read_text());data['hosts']=data['hosts'].replace(APPEND_SENTINEL+'\n','')
            p.write_text(json.dumps(data))
            with patch.object(auditor,'ROOT',root),patch.object(auditor,'verify_guest',side_effect=lambda guest,scenario,guest_name:len(CASES[scenario])):
                with self.assertRaisesRegex(ValueError,'sentinel external'):auditor.audit([a])


if __name__ == '__main__':
    unittest.main()
