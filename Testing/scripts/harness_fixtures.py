"""Synthetic Layer 3 envelopes for regression tests, never VM acceptance evidence."""
import hashlib
import json
from pathlib import Path
import shutil

from layer3_contract import ROOT, assess_observations, build_plan, load_contract


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value) + '\n')


class Layer3Fixture:
    commit = 'a' * 40

    def __init__(self, root):
        self.root = Path(root)
        for name in ('layer3/cases.json', 'config/layer3-browsers.json',
                     'layers/03-vm-acceptance.md'):
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)
        self.registry, self.matrix = load_contract(self.root)
        packages = []
        for guest, component, suffix in (('ubuntu', 'deb', '.deb'),
                ('fedora', 'rpm', '.rpm'), ('cachyos', 'arch', '.pkg.tar.zst')):
            report = self.root / 'packages' / guest / 'result.json'
            package = report.parent / 'artifacts' / ('fixture' + suffix)
            package.parent.mkdir(parents=True)
            package.write_bytes(b'Synthetic package bytes for ' + guest.encode())
            digest = hashlib.sha256(package.read_bytes()).hexdigest()
            write_json(report, {'passed': True, 'build_provenance_verified': True,
                'component': component, 'commit': self.commit,
                'package_metadata': {'version': '0.2.0-1'},
                'artifacts': [{'filename': package.name, 'sha256': digest}]})
            packages.append({'guest': guest, 'report': str(report), 'package': str(package),
                'build_provenance_verified': True, 'version': '0.2.0-1', 'sha256': digest})
        self.inputs = self.root / 'inputs.json'
        write_json(self.inputs, {'workflow': {'status': 'completed', 'conclusion': 'success',
            'headSha': self.commit}, 'packages': packages})
        (self.root / 'results').mkdir()

    @staticmethod
    def evidence(case_id, browser):
        if case_id in ('VM-WAL-001', 'VM-AAL-001'):
            return {'enforcement_active': True, 'policy_unchanged': True,
                'rule_id': 'fixture-rule', 'rejection': 'fixture-rule must use Tier 2 or Tier 3'}
        if case_id == 'VM-WAL-005':
            return dict.fromkeys(('enforcement_active', 'outside_rejected', 'listed_allowed',
                'hosts_managed_unchanged', 'host_status_compliant'), True) | {'managed_domain_count': 0}
        evidence = dict.fromkeys(('baseline_accessible', 'positive_blocked', 'near_miss_accessible',
            'open_tab_blocked', 'history_navigation_blocked', 'baseline_resolution',
            'managed_hosts_blocked', 'subdomain_blocked', 'host_status_compliant'), True)
        evidence['rule_id'] = 'fixture-rule'
        if case_id == 'VM-WEB-001':
            host = browser + '.web.blockuntu.test'
            names = (host, 'www.' + host, 'near-' + host)
            evidence['baseline_addresses'] = dict.fromkeys(names, '127.0.0.1')
            evidence['blocked_addresses'] = dict(zip(names, ('0.0.0.0', '0.0.0.0', '127.0.0.1')))
        return evidence

    def bundle(self, prepared, index=0):
        """Write a structurally valid smoke, matcher, domain or isolated envelope."""
        item = prepared['runs'][index]
        guest, run_id = item['guest'], item['run_id']
        expected = prepared['expected'][guest]
        plan = prepared['plan']
        base = self.root / 'results' / run_id / guest
        acceptance = base / 'acceptance'
        package = json.loads(Path(expected['report']).read_text())
        write_json(acceptance / 'package-report.json', package)
        manifest = dict.fromkeys(('scripts/vm-acceptance.py', 'scripts/phase0-vm.py',
            'fixtures/layer3/guest.py', 'fixtures/layer3/browsers.py',
            'fixtures/layer3/processes.py', 'fixtures/layer3/gui.py',
            'fixtures/layer3/scenarios.py'), 'c' * 64)
        write_json(acceptance / 'harness-files.json', manifest)
        write_json(self.root / 'runtime/vm-phase0' / run_id / guest / 'owner.json',
            {'run_id': run_id, 'template': guest, 'stage': 'removed',
             'uuid': 'fixture-clone-' + run_id, 'bases': {guest: 'fixture-base'}})
        write_json(base / 'result.json', {'status': 'passed', 'clone_uuid': 'fixture-clone-' + run_id,
            'machine_id': 'fixture-machine-' + run_id, 'host_fingerprint': 'fixture-key-' + run_id})
        smoke = build_plan(self.registry, self.matrix, suite='smoke', guests=[guest])
        cases = []
        for row in smoke['observations']:
            case = {'id': row['id'], 'status': 'pass',
                'evidence': {'heartbeat': {'extension_version': 'fixture-extension'}}}
            if row['browser_package']:
                case['browser'] = row['browser_package'].removesuffix('-native')
            if row['id'] == 'VM-INSTALL-001':
                case['version'] = expected['version']
            cases.append(case)
        browsers = {row['browser'] for row in cases if 'browser' in row}
        write_json(acceptance / 'browser-metadata.json', {browser: {
            'browser_version': 'fixture-browser', 'extension_version': 'fixture-extension',
            'extension_id': 'fixture-id', 'package_source': 'fixture-source',
            'policy_path': '/fixture/policy'} for browser in browsers})
        observations = [{key: row[key] for key in
            ('observation_id', 'id', 'guest', 'browser_package', 'scenario')} |
            {'status': 'pass', 'seconds': 1, 'evidence': self.evidence(row['id'],
                (row['browser_package'] or '').removesuffix('-native'))}
            for row in plan['observations'] if row['observation_id'] in item['observation_ids']]
        result = {'run_id': run_id, 'guest': guest, 'suite': guest + '-smoke', 'status': 'pass',
            'base_integrity_verified': True, 'package_commit': self.commit,
            'package': package['artifacts'], 'harness_sha256': 'c' * 64, 'cases': cases,
            'started_at': '2000-01-01T00:00:00+00:00', 'finished_at': '2000-01-01T00:01:00+00:00',
            'smoke_coverage': {'observation_status': 'pass'}, 'observations': observations}
        if plan['scope'] == 'selected':
            selected = {**plan, 'observations': [row for row in plan['observations']
                if row['observation_id'] in item['observation_ids']]}
            result.update(suite=guest + '-core-selected', coverage_scope='selected',
                selected_case_ids=list(dict.fromkeys(row['id'] for row in observations)),
                selected_coverage=assess_observations(selected, observations))
            if item['case_id']:
                row = observations[0]
                scenario = {key: row[key] for key in ('id', 'status', 'seconds', 'evidence')}
                result['cases'] = [cases[0], scenario]
                write_json(acceptance / 'scenario-result.json', scenario)
            else:
                result.update(smoke_observations=smoke['observations'], p32_cases=observations)
                write_json(acceptance / 'p32-result.json', observations)
                suffixes = {'VM-WEB-001': 'domain', 'VM-WEB-003': 'url-prefix',
                    'VM-WEB-004': 'url-contains', 'VM-WEB-005': 'path-prefix',
                    'VM-WEB-006': 'open-tab', 'VM-WEB-007': 'spa'}
                for row in observations:
                    browser = row['browser_package'].removesuffix('-native')
                    (acceptance / (browser + '-p32-' + suffixes[row['id']] + '.png')).write_bytes(b'fixture')
                names = {'web.blockuntu.test'} | {name for browser in browsers for name in
                    (browser + '.web.blockuntu.test', 'www.' + browser + '.web.blockuntu.test',
                     'near-' + browser + '.web.blockuntu.test')}
                write_json(acceptance / 'domain-dns.json', {'public_query_resolved': True,
                    'fixture_addresses': dict.fromkeys(names, '127.0.0.1'),
                    'link': 'fixture-link', 'upstream': 'fixture-upstream'})
        write_json(acceptance / 'result.json', result)
        return acceptance
