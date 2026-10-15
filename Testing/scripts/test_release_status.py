"""Candidate aggregation must reject missing, contradictory and mismatched evidence."""
import contextlib
import copy
import functools
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import release_status as release
from harness_fixtures import Layer3Fixture, write_json
from layer3_group_audit import audit_summaries
from layer3_group_runner import preflight


class ReleaseStatusTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.path = self.root / 'candidate.json'
        self.data = {'schema_version': 1, 'name': 'fixture-rc', 'commit': 'a' * 40,
            'repository': 'fixture/repo', 'ci_runs': {'source': 11, 'package': 22},
            'package_reports': {}, 'smoke_summaries': [], 'recovery_checks': []}
        self.receipt = {'schema_version': 1, 'repository': 'fixture/repo',
                        'observed_at': '2026-10-15T12:00:00+00:00', 'runs': {}}
        for kind, jobs in (('source', release.SOURCE_JOBS), ('package', release.PACKAGE_JOBS)):
            run_id = self.data['ci_runs'][kind]
            self.receipt['runs'][kind] = {'repository': 'fixture/repo', 'run': {
                'id': run_id, 'run_attempt': 1, 'head_sha': 'a' * 40,
                'path': release.WORKFLOWS[kind], 'status': 'completed', 'conclusion': 'success',
                'html_url': f'https://github.com/fixture/repo/actions/runs/{run_id}'},
                'jobs': [{'name': name, 'status': 'completed', 'conclusion': 'success'} for name in sorted(jobs)]}

    def spec(self):
        write_json(self.path, self.data)
        return release.candidate(self.path)

    def packages(self):
        contract = release.module('package-ci')
        for target, component in release.PACKAGES.items():
            path = self.root / target / 'result.json'
            suffix = {'deb': '.deb', 'rpm': '.rpm', 'arch': '.pkg.tar.zst',
                      'firefox': '.xpi', 'chrome': '.zip'}[component]
            binary = path.parent / 'artifacts' / ('fixture' + suffix)
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_bytes(b'Explicitly synthetic artifact: ' + target.encode())
            expected = {contract.BUILD_IDS[component], 'PKG-META-001', 'PKG-META-003'}
            if component in contract.CONTRACT['targets']:
                expected |= {'PKG-META-002', *contract.PACKAGE_CASES}
            cases = [{'id': name, 'status': 'pass'} for name in sorted(expected)]
            for case in cases:
                if case['id'].startswith('PKG-UPGRADE-'):
                    case.update(status='skip', reason='No accepted prior release')
            write_json(path, {'layer': 'package-ci', 'source_clean': True, 'component': component,
                'commit': self.data['commit'], 'passed': True, 'build_provenance_verified': True,
                'os_release': 'ID=' + target + '\n', 'cases': cases,
                'package_metadata': {'version': '0.2.0-1'}, 'extension_version': 'fixture-extension',
                'artifacts': [{'filename': binary.name, 'sha256': release.sha256(binary)}]})
            self.data['package_reports'][target] = str(path.relative_to(self.root))

    def smoke(self):
        _, identities = release.package_checks(self.spec())
        path = self.root / 'smoke.json'
        packages = {guest: identities[release.GUEST_PACKAGES[guest]] for guest in release.GUESTS}
        write_json(path, {'package_commit': self.data['commit'], 'packages': packages,
                          'runs': [{'guest': guest} for guest in release.GUESTS]})
        self.data['smoke_summaries'] = [str(path.name)]
        return {'status': 'pass', 'live_inventory_check': 'pass', 'latest_successful_runs': [
            {'guest': guest, 'run_id': 'fixture-' + guest, 'evidence': {'valid': True,
             'required_passes': 21, 'package_sha256': packages[guest]['sha256']}}
            for guest in release.GUESTS]}

    def test_minimal_candidate_is_incomplete_and_does_not_shrink_gate(self):
        result = release.report(self.spec(), self.receipt)
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(len([r for r in result['checks'] if r['id'].startswith('package:')]), 6)
        self.assertEqual(len([r for r in result['checks'] if r['id'].startswith('smoke:')]), 3)
        self.assertFalse(result['full_catalog_acceptance'])
        self.assertEqual(result['checks'][-1]['status'], 'not_selected')

    def test_full_selected_gate_needs_live_safety_and_exact_packages(self):
        self.packages()
        audited = self.smoke()
        auditor = Mock(return_value=audited)
        spec = self.spec()
        offline = release.report(spec, self.receipt, smoke_auditor=auditor)
        self.assertEqual(offline['status'], 'incomplete')
        self.assertEqual(offline['blocking_checks'], ['vm-safety'])
        complete = release.report(spec, self.receipt, live=True, smoke_auditor=auditor)
        self.assertEqual(complete['status'], 'ready')
        self.assertEqual(complete['blocking_checks'], [])
        self.assertEqual(auditor.call_args.kwargs, {'guests': list(release.GUESTS), 'live': True})

    def test_ci_rejects_wrong_sha_workflow_repo_and_run(self):
        for key, value in (('head_sha', 'b' * 40), ('path', release.WORKFLOWS['package']), ('id', 99)):
            with self.subTest(key=key):
                receipt = copy.deepcopy(self.receipt)
                receipt['runs']['source']['run'][key] = value
                self.assertEqual(release.ci_checks(self.spec(), receipt)[0]['status'], 'mismatch')
        receipt = copy.deepcopy(self.receipt)
        receipt['runs']['source']['repository'] = 'other/repo'
        self.assertEqual(release.ci_checks(self.spec(), receipt)[0]['status'], 'mismatch')

    def test_ci_cannot_pass_with_missing_skipped_failed_or_duplicate_job(self):
        for mutation in ('missing', 'skipped', 'failed', 'duplicate'):
            with self.subTest(mutation=mutation):
                receipt = copy.deepcopy(self.receipt)
                jobs = receipt['runs']['source']['jobs']
                if mutation == 'missing': jobs.pop()
                elif mutation == 'duplicate': jobs.append(jobs[0])
                else: jobs[0]['conclusion'] = 'skipped' if mutation == 'skipped' else 'failure'
                self.assertEqual(release.ci_checks(self.spec(), receipt)[0]['status'], 'fail')
        receipt['runs']['source']['run']['status'] = 'in_progress'
        receipt['runs']['source']['jobs'].pop()
        self.assertEqual(release.ci_checks(self.spec(), receipt)[0]['status'], 'running')

    def test_github_fetch_pins_attempt_and_rejects_truncated_jobs(self):
        spec = self.spec()
        run = {**self.receipt['runs']['source']['run'], 'repository': {'full_name': 'fixture/repo'}}
        api = Mock(side_effect=[run, {'total_count': 2, 'jobs': []}, OSError('offline')])
        receipt = release.fetch_ci(spec, api)
        self.assertIn('/attempts/1/jobs?', api.call_args_list[1].args[0])
        self.assertTrue(receipt['runs']['source'].get('error'))
        self.assertTrue(receipt['runs']['package'].get('error'))
        self.assertEqual(release.ci_checks(spec, receipt)[0]['status'], 'unverified')

    def test_changed_or_missing_artifact_cannot_pass(self):
        self.packages()
        spec = self.spec()
        binary = self.root / 'ubuntu/artifacts/fixture.deb'
        binary.write_bytes(b'changed after inspection')
        checks, _ = release.package_checks(spec)
        self.assertEqual(checks[0]['status'], 'fail')
        binary.unlink()
        checks, _ = release.package_checks(spec)
        self.assertEqual(checks[0]['status'], 'missing')
        outside = self.root / 'other-artifact.deb'
        outside.write_bytes(b'not the selected artifact')
        binary.symlink_to(outside)
        checks, _ = release.package_checks(spec)
        self.assertEqual(checks[0]['status'], 'fail')

    def test_false_pass_flag_missing_assertions_and_wrong_distro_are_rejected(self):
        self.packages()
        path = self.root / 'ubuntu/result.json'
        original = release.load(path)
        for mutation in ('assertion', 'provenance', 'distro', 'sha'):
            report = copy.deepcopy(original)
            if mutation == 'assertion': report['cases'].pop()
            elif mutation == 'provenance': report['build_provenance_verified'] = False
            elif mutation == 'distro': report['os_release'] = 'ID=debian\n'
            else: report['commit'] = 'b' * 40
            write_json(path, report)
            checks, _ = release.package_checks(self.spec())
            self.assertEqual(checks[0]['status'], 'mismatch' if mutation == 'sha' else 'fail')

    def test_smoke_for_same_commit_but_other_binary_is_rejected(self):
        self.packages()
        audit = self.smoke()
        path = self.root / 'smoke.json'
        data = release.load(path)
        data['packages']['ubuntu']['sha256'] = 'f' * 64
        write_json(path, data)
        result = release.report(self.spec(), self.receipt, live=True, smoke_auditor=Mock(return_value=audit))
        row = next(row for row in result['checks'] if row['id'] == 'smoke:ubuntu')
        self.assertEqual(row['status'], 'mismatch')
        self.assertEqual(result['status'], 'not_ready')

    def test_other_candidate_smoke_never_substitutes_for_current_evidence(self):
        self.packages()
        self.smoke()
        path = self.root / 'smoke.json'
        summary = release.load(path)
        summary['package_commit'] = 'b' * 40
        write_json(path, summary)
        auditor = Mock()
        result = release.report(self.spec(), self.receipt, smoke_auditor=auditor)
        auditor.assert_not_called()
        self.assertEqual(result['status'], 'not_ready')
        self.assertTrue(all(row['status'] == 'mismatch' for row in result['checks'] if row['id'].startswith('smoke:')))

    def test_malformed_package_evidence_is_reported_as_failure(self):
        self.packages()
        write_json(self.root / 'ubuntu/result.json', ['a JSON array cannot be a package report'])
        checks, _ = release.package_checks(self.spec())
        self.assertEqual(checks[0]['status'], 'fail')
        receipt = copy.deepcopy(self.receipt)
        receipt['runs']['source'] = 'not a CI envelope'
        self.assertEqual(release.ci_checks(self.spec(), receipt)[0]['status'], 'fail')

    def test_latest_failed_retry_is_not_hidden_or_replaced_by_older_pass(self):
        self.packages()
        self.smoke()
        auditor = Mock(side_effect=ValueError('The latest attempted group did not pass'))
        result = release.report(self.spec(), self.receipt, live=True, smoke_auditor=auditor)
        self.assertEqual(result['status'], 'not_ready')
        self.assertTrue(all(row['status'] == 'fail' for row in result['checks'] if row['id'].startswith('smoke:')))

    def test_running_vm_batches_are_incomplete_without_claiming_a_pass(self):
        self.packages()
        self.smoke()
        path = self.root / 'smoke.json'
        summary = release.load(path)
        summary['status'] = 'running'
        write_json(path, summary)
        self.data['recovery_checks'] = [{'suite': 'p41', 'guest': 'ubuntu', 'summaries': ['recovery.json']}]
        write_json(self.root / 'recovery.json', {'package_commit': self.data['commit'], 'status': 'running'})
        smoke, recovery = Mock(), Mock()
        result = release.report(self.spec(), self.receipt, live=True,
                                smoke_auditor=smoke, recovery_auditors={'p41': recovery})
        smoke.assert_not_called()
        recovery.assert_not_called()
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(result['checks'][-1]['status'], 'running')

    def test_selected_recovery_blocks_and_unselected_catalog_does_not(self):
        self.packages()
        smoke = self.smoke()
        self.data['recovery_checks'] = [{'suite': 'p41', 'guest': 'ubuntu', 'summaries': ['recovery.json']}]
        write_json(self.root / 'recovery.json', {'package_commit': self.data['commit']})
        auditor = Mock(side_effect=ValueError('Refusal changed package inventory'))
        result = release.report(self.spec(), self.receipt, live=True, smoke_auditor=Mock(return_value=smoke),
                                recovery_auditors={'p41': auditor})
        self.assertEqual(result['status'], 'not_ready')
        self.data['recovery_checks'] = []
        result = release.report(self.spec(), self.receipt, live=True, smoke_auditor=Mock(return_value=smoke),
                                recovery_auditors={'p41': auditor})
        self.assertEqual(result['status'], 'ready')
        self.assertEqual(result['checks'][-1]['status'], 'not_selected')

    def test_recovery_must_match_guest_candidate_and_binary(self):
        self.packages()
        self.data['recovery_checks'] = [{'suite': 'p43', 'guest': 'ubuntu', 'summaries': ['recovery.json']}]
        write_json(self.root / 'recovery.json', {'package_commit': self.data['commit']})
        expected = release.package_checks(self.spec())[1]['ubuntu']
        result = {'scope': 'ubuntu-p43', 'status': 'pass', 'package_commit': self.data['commit'],
            'package_sha256': expected['sha256'], 'installed_version': expected['version'],
            'all_owned_clones_removed': True, 'required_passes': 12}
        self.assertEqual(release.recovery_checks(self.spec(), {'ubuntu': expected},
            {'p43': Mock(return_value=result)})[0]['status'], 'pass')
        result['package_sha256'] = 'f' * 64
        self.assertEqual(release.recovery_checks(self.spec(), {'ubuntu': expected},
            {'p43': Mock(return_value=result)})[0]['status'], 'mismatch')
        result['package_sha256'] = expected['sha256']
        self.data['recovery_checks'][0]['qualify'] = True
        self.assertEqual(release.recovery_checks(self.spec(), {'ubuntu': expected},
            {'p43': Mock(return_value=result)})[0]['status'], 'fail')

    def test_invalid_configuration_rejected_and_paths_resolve_from_manifest(self):
        self.data['package_reports'] = {'ubuntu': 'relative/result.json'}
        self.assertEqual(self.spec()['package_reports']['ubuntu'], self.root / 'relative/result.json')
        for field, value in (('commit', 'short'), ('schema_version', True),
                ('ci_runs', {'source': True}), ('package_reports', {'typo': 'x'}),
                ('recovery_checks', [{'suite': 'p41', 'guest': 'ubuntu', 'required': False}])):
            original = self.data[field]
            self.data[field] = value
            with self.assertRaises(ValueError): self.spec()
            self.data[field] = original

    def test_cli_emits_consistent_formats_without_network_and_never_overwrites(self):
        self.spec()
        output = self.root / 'report'
        with patch.object(release, 'fetch_ci') as network, contextlib.redirect_stdout(io.StringIO()):
            status = release.main(['--candidate', str(self.path), '--output-dir', str(output)])
        network.assert_not_called()
        self.assertEqual(status, 1)
        result = release.load(output / 'report.json')
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual((output / 'report.md').read_text(), release.markdown(result))
        before = (output / 'report.json').read_bytes()
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            release.main(['--candidate', str(self.path), '--output-dir', str(output)])
        self.assertEqual(error.exception.code, 2)
        self.assertEqual((output / 'report.json').read_bytes(), before)

    def test_actual_smoke_auditor_rechecks_temporary_bundles(self):
        fixture = Layer3Fixture(self.root / 'actual-smoke')
        prepared = preflight('smoke', list(release.GUESTS), 'release-fixture', fixture.inputs, root=fixture.root)
        for index, item in enumerate(prepared['runs']):
            fixture.bundle(prepared, index)
            item.update(status='pass', exit_code=0,
                started_at=f'2000-01-01T00:0{index * 2}:00+00:00',
                finished_at=f'2000-01-01T00:0{index * 2 + 1}:00+00:00')
            path = fixture.root / 'runtime/vm-phase0' / item['run_id'] / item['guest'] / 'owner.json'
            owner = release.load(path)
            owner['paths'] = [str(fixture.root / (item['run_id'] + '-removed-disk'))]
            write_json(path, owner)
        summary = fixture.root / 'summary.json'
        write_json(summary, {'kind': 'layer3-group-run', 'suite': 'smoke', 'scope': 'smoke-only',
            'status': 'pass', 'package_commit': fixture.commit, 'workflow': {'headSha': fixture.commit},
            'packages': prepared['expected'], 'runs': prepared['runs']})
        self.data['smoke_summaries'] = [str(summary)]
        packages = {release.GUEST_PACKAGES[guest]: value for guest, value in prepared['expected'].items()}
        rows = release.smoke_checks(self.spec(), packages,
                                   auditor=functools.partial(audit_summaries, root=fixture.root))
        self.assertEqual([row['status'] for row in rows], ['pass', 'pass', 'pass', 'unverified'])


if __name__ == '__main__':
    unittest.main()
