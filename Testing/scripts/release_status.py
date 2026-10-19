"""Read-only aggregation of evidence for one explicit release candidate."""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess

from layer3_group_audit import audit_summaries

ROOT = Path(__file__).resolve().parents[1]
GUESTS = ('ubuntu', 'fedora', 'cachyos')
PACKAGES = {'ubuntu': 'deb', 'debian': 'deb', 'fedora': 'rpm',
            'arch': 'arch', 'firefox': 'firefox', 'chrome': 'chrome'}
GUEST_PACKAGES = {'ubuntu': 'ubuntu', 'fedora': 'fedora', 'cachyos': 'arch'}
WORKFLOWS = {'source': '.github/workflows/ci.yml', 'package': '.github/workflows/package-ci.yml'}
SOURCE_JOBS = {'Source / Test harness', *(f'Source / Rust / {name}' for name in
    ('core', 'daemon', 'native', 'tauri')), *(f'Source / Frontend / {name}' for name in
    ('gui', 'firefox', 'chrome'))}
PACKAGE_JOBS = {'Package inspector regression tests',
    *(f'Package / {name} / x86-64' for name in ('ubuntu', 'debian', 'fedora', 'arch')),
    *(f'Package / {name} extension' for name in ('firefox', 'chrome'))}


def load(path):
    result = json.loads(Path(path).read_text())
    if not isinstance(result, dict):
        raise ValueError('Expected a JSON object: ' + str(path))
    return result


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def module(name):
    spec = importlib.util.spec_from_file_location('release_' + name.replace('-', '_'),
                                                ROOT / 'scripts' / (name + '.py'))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def candidate(path):
    path = Path(path).resolve()
    data = load(path)
    allowed = {'schema_version', 'name', 'commit', 'repository', 'ci_runs', 'ci_receipt',
               'package_reports', 'smoke_summaries', 'recovery_checks', 'e2e_reports'}
    if (not isinstance(data, dict) or set(data) - allowed or
            type(data.get('schema_version')) is not int or data['schema_version'] != 1):
        raise ValueError('Candidate must use schema_version 1 and known fields')
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,79}', data.get('name', '')):
        raise ValueError('Candidate name must be a short filename-safe label')
    if not re.fullmatch(r'[0-9a-f]{40}', data.get('commit', '')):
        raise ValueError('Candidate requires a full 40-character source commit')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', data.get('repository', '')):
        raise ValueError('Candidate requires an owner/repository')
    runs = data.get('ci_runs', {})
    if (not isinstance(runs, dict) or set(runs) - set(WORKFLOWS) or
            any(v is not None and (type(v) is not int or v <= 0) for v in runs.values())):
        raise ValueError('ci_runs accepts positive source/package run IDs or null')
    reports = data.get('package_reports', {})
    if not isinstance(reports, dict) or set(reports) - set(PACKAGES):
        raise ValueError('Unknown package report target')
    def resolve(value):
        if not isinstance(value, str) or not value:
            raise ValueError('Evidence paths must be nonempty strings')
        return (path.parent / value).resolve()
    def paths(values):
        if not isinstance(values, list):
            raise ValueError('Evidence summaries must be a list')
        resolved = [resolve(value) for value in values]
        if len(resolved) != len(set(resolved)):
            raise ValueError('Repeated evidence summary')
        return resolved
    data['package_reports'] = {key: resolve(value) for key, value in reports.items()}
    data['smoke_summaries'] = paths(data.get('smoke_summaries', []))
    data['e2e_reports'] = paths(data.get('e2e_reports', []))
    data['ci_receipt'] = resolve(data['ci_receipt']) if data.get('ci_receipt') is not None else None
    recovery = data.get('recovery_checks', [])
    if not isinstance(recovery, list):
        raise ValueError('recovery_checks must be a list')
    selected = set()
    for entry in recovery:
        if (not isinstance(entry, dict) or set(entry) - {'suite', 'guest', 'summaries', 'qualify'} or
                entry.get('suite') not in ('p41', 'p42', 'p43') or entry.get('guest') not in GUESTS or
                type(entry.get('qualify', False)) is not bool):
            raise ValueError('Recovery selection requires p41/p42/p43, a guest and boolean qualify')
        key = (entry['suite'], entry['guest'])
        if key in selected:
            raise ValueError('Repeated recovery selection')
        selected.add(key)
        entry['summaries'] = paths(entry.get('summaries', []))
    data['recovery_checks'] = recovery
    data['ci_runs'] = runs
    data['manifest'] = path
    return data


def check(identifier, status, reason, *, required=True, evidence=(), **details):
    return {'id': identifier, 'status': status, 'required': required, 'reason': reason,
            'evidence': [str(path) for path in evidence], **details}


def github_json(endpoint):
    answer = subprocess.run(['gh', 'api', endpoint], capture_output=True, text=True,
                            timeout=60, check=True)
    return json.loads(answer.stdout)


def fetch_ci(spec, api=github_json):
    receipt = {'schema_version': 1, 'repository': spec['repository'],
               'observed_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'runs': {}}
    for kind in WORKFLOWS:
        run_id = spec['ci_runs'].get(kind)
        if run_id is None:
            continue
        try:
            endpoint = f'repos/{spec["repository"]}/actions/runs/{run_id}'
            run = api(endpoint)
            attempt = run['run_attempt']
            jobs = api(f'{endpoint}/attempts/{attempt}/jobs?per_page=100')
            if jobs['total_count'] != len(jobs['jobs']):
                raise ValueError('CI job list is truncated')
            receipt['runs'][kind] = {'run': {key: run[key] for key in
                ('id', 'run_attempt', 'head_sha', 'path', 'status', 'conclusion', 'html_url')},
                'repository': run['repository']['full_name'], 'jobs': [
                    {key: job[key] for key in ('name', 'status', 'conclusion', 'html_url')}
                    for job in jobs['jobs']]}
        except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError) as error:
            receipt['runs'][kind] = {'error': 'GitHub lookup unavailable: ' + type(error).__name__}
    return receipt


def ci_checks(spec, receipt):
    if receipt is not None and (not isinstance(receipt, dict) or not isinstance(receipt.get('runs'), dict)):
        return [check('ci:' + kind, 'fail', 'CI receipt must be an object') for kind in WORKFLOWS]
    rows = []
    for kind, workflow in WORKFLOWS.items():
        identifier = 'ci:' + kind
        run_id = spec['ci_runs'].get(kind)
        if run_id is None:
            rows.append(check(identifier, 'missing', 'No CI run selected'))
            continue
        entry = (receipt or {}).get('runs', {}).get(kind)
        if entry is None or (isinstance(entry, dict) and entry.get('error')):
            rows.append(check(identifier, 'unverified', 'No usable GitHub receipt for this run'))
            continue
        try:
            run, jobs = entry['run'], entry['jobs']
            if (receipt.get('schema_version') != 1 or receipt.get('repository') != spec['repository'] or
                    entry['repository'] != spec['repository'] or run['id'] != run_id or
                    run['head_sha'] != spec['commit'] or run['path'] != workflow or
                    run['html_url'] != f'https://github.com/{spec["repository"]}/actions/runs/{run_id}' or
                    type(run['run_attempt']) is not int or run['run_attempt'] < 1):
                rows.append(check(identifier, 'mismatch', 'CI identity differs from candidate/workflow'))
                continue
            observed = dt.datetime.fromisoformat(receipt['observed_at'])
            if observed.tzinfo is None:
                raise ValueError('CI receipt lacks a timezone')
            expected = SOURCE_JOBS if kind == 'source' else PACKAGE_JOBS
            names = [job['name'] for job in jobs]
            if len(names) != len(set(names)):
                raise ValueError('Duplicate CI job names')
            if run['status'] != 'completed':
                status, reason = 'running', 'Selected CI run has not completed'
            elif run['conclusion'] != 'success':
                status, reason = 'fail', 'Selected CI run did not succeed'
            elif not expected <= set(names):
                status, reason = 'fail', 'Required CI jobs missing: ' + ', '.join(sorted(expected - set(names)))
            elif any(job['status'] != 'completed' or job['conclusion'] != 'success' for job in jobs):
                status, reason = 'fail', 'CI contains a failed, skipped or unfinished job'
            else:
                status, reason = 'pass', f'{len(jobs)} jobs passed; receipt observed {receipt["observed_at"]}'
            rows.append(check(identifier, status, reason, evidence=[run['html_url']],
                              run_id=run_id, run_attempt=run['run_attempt']))
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            rows.append(check(identifier, 'fail', 'Invalid CI receipt: ' + str(error)))
    return rows


def package_checks(spec):
    rows, identities = [], {}
    contract = module('package-ci')
    for target, component in PACKAGES.items():
        identifier = 'package:' + target
        path = spec['package_reports'].get(target)
        if path is None or not path.is_file():
            rows.append(check(identifier, 'missing', 'Package report missing', evidence=[path] if path else []))
            continue
        try:
            report = load(path)
            if report.get('commit') != spec['commit'] or report.get('component') != component:
                rows.append(check(identifier, 'mismatch', 'Package commit/component differs from candidate', evidence=[path]))
                continue
            if (report.get('layer') != 'package-ci' or report.get('source_clean') is not True or
                    report.get('passed') is not True or report.get('build_provenance_verified') is not True):
                raise ValueError('Package build did not pass with recorded build provenance')
            if target in ('ubuntu', 'debian'):
                release = dict(line.split('=', 1) for line in report.get('os_release', '').splitlines() if '=' in line)
                if release.get('ID', '').strip('"') != target:
                    raise ValueError('Debian/Ubuntu build environment differs from selected target')
            expected = {contract.BUILD_IDS[component], 'PKG-META-001', 'PKG-META-003'}
            if component in contract.CONTRACT['targets']:
                expected |= {'PKG-META-002', *contract.PACKAGE_CASES}
            cases = report['cases']
            names = [row['id'] for row in cases]
            if len(names) != len(set(names)) or not expected <= set(names):
                raise ValueError('Package assertions are missing or repeated')
            for row in cases:
                deferred_upgrade = (row['id'] in ('PKG-UPGRADE-001', 'PKG-UPGRADE-002') and
                    contract.CONTRACT['previous_accepted_release'] is None and row.get('reason'))
                if row['status'] != 'pass' and not (row['status'] == 'skip' and deferred_upgrade):
                    raise ValueError('Package assertion did not pass: ' + row['id'])
            artifacts = report['artifacts']
            names = [a['filename'] for a in artifacts]
            if not names or len(names) != len(set(names)):
                raise ValueError('Package artifact inventory is empty or repeated')
            for artifact in artifacts:
                name = artifact['filename']
                if Path(name).name != name or not re.fullmatch(r'[0-9a-f]{64}', artifact['sha256']):
                    raise ValueError('Invalid artifact filename/checksum')
                binary = path.parent / 'artifacts' / name
                if binary.is_symlink() or binary.parent.resolve() != (path.parent / 'artifacts').resolve():
                    raise ValueError('Invalid artifact path')
                if sha256(binary) != artifact['sha256']:
                    raise ValueError('Artifact checksum differs: ' + name)
            suffix = {'deb': '.deb', 'rpm': '.rpm', 'arch': '.pkg.tar.zst',
                      'firefox': '.xpi', 'chrome': '.zip'}[component]
            binaries = [a for a in artifacts if a['filename'].endswith(suffix)]
            if len(binaries) != 1:
                raise ValueError('Expected exactly one distribution/extension artifact')
            version = report.get('package_metadata', {}).get('version') if component in contract.CONTRACT['targets'] else report.get('extension_version')
            if not isinstance(version, str) or not version:
                raise ValueError('Installed package/extension version missing')
            identity = {'commit': spec['commit'], 'filename': binaries[0]['filename'],
                        'sha256': binaries[0]['sha256'], 'version': version}
            identities[target] = identity
            rows.append(check(identifier, 'pass', 'Assertions and artifact checksums verified',
                              evidence=[path], artifact=identity))
        except FileNotFoundError as error:
            rows.append(check(identifier, 'missing', 'Referenced artifact missing: ' + str(error.filename), evidence=[path]))
        except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
            rows.append(check(identifier, 'fail', str(error), evidence=[path]))
    return rows, identities


def smoke_checks(spec, packages, live=False, auditor=audit_summaries):
    paths = spec['smoke_summaries']
    if not paths or any(not p.is_file() for p in paths):
        return [check('smoke:' + guest, 'missing', 'Smoke summaries missing', evidence=paths) for guest in GUESTS] + [
            check('vm-safety', 'unverified', 'No complete smoke evidence to audit')]
    try:
        summaries = [load(p) for p in paths]
        if any(s.get('package_commit') != spec['commit'] for s in summaries):
            return [check('smoke:' + guest, 'mismatch', 'Smoke evidence belongs to another candidate', evidence=paths)
                    for guest in GUESTS] + [check('vm-safety', 'unverified', 'Candidate smoke evidence required')]
        present = {r.get('guest') for s in summaries for r in s.get('runs', [])}
        if not present <= set(GUESTS):
            raise ValueError('Unknown smoke guest')
        if any(summary.get('status') == 'running' for summary in summaries):
            return [check('smoke:' + guest, 'running' if guest in present else 'missing',
                          'A selected smoke batch is still running', evidence=paths) for guest in GUESTS] + [
                check('vm-safety', 'unverified', 'Complete smoke evidence required before safety audit')]
        guests = [guest for guest in GUESTS if guest in present]
        if not guests:
            raise ValueError('No smoke attempts recorded')
        audit = auditor(paths, guests=guests, live=live)
        rows = []
        latest = {r['guest']: r for r in audit['latest_successful_runs']}
        if set(latest) != set(guests):
            raise ValueError('Smoke audit returned a different guest selection')
        for guest in GUESTS:
            if guest not in latest:
                rows.append(check('smoke:' + guest, 'missing', 'No smoke attempt selected'))
                continue
            evidence = latest[guest]['evidence']
            expected = packages.get(GUEST_PACKAGES[guest])
            # Bind every declared attempt to the exact selected binary, not merely its commit.
            selected_packages = [s.get('packages', {}).get(guest) for s in summaries
                                 if any(r.get('guest') == guest for r in s.get('runs', []))]
            if expected is None:
                status, reason = 'unverified', 'Matching package artifact must pass first'
            elif (not selected_packages or any(not p or any(p.get(k) != expected[k] for k in
                    ('commit', 'sha256', 'filename', 'version')) for p in selected_packages) or
                    evidence.get('package_sha256') != expected['sha256']):
                status, reason = 'mismatch', 'Smoke artifact differs from the selected package'
            elif evidence.get('valid') is not True or evidence.get('required_passes') != 21:
                status, reason = 'fail', 'Smoke audit did not verify all 21 observations'
            else:
                status, reason = 'pass', '21 observations and saved clone evidence verified'
            rows.append(check('smoke:' + guest, status, reason, evidence=paths,
                              run_id=latest[guest]['run_id'],
                              browser_versions=evidence.get('browser_versions', {}),
                              harness_manifest=evidence.get('manifest', {})))
        verified_live = live and audit.get('status') == 'pass' and audit.get('live_inventory_check') == 'pass'
        rows.append(check('vm-safety', 'pass' if verified_live else 'unverified',
            'Current base integrity and clone inventory verified' if verified_live else
            'Saved evidence checked; use --live on the VM host to verify current safety', evidence=paths))
        return rows
    except FileNotFoundError as error:
        status, reason = 'missing', 'Smoke bundle file missing: ' + str(error.filename)
    except (OSError, ValueError, KeyError, TypeError, AttributeError, RuntimeError,
            AssertionError, StopIteration, subprocess.SubprocessError) as error:
        status, reason = 'fail', str(error)
    return [check('smoke:' + guest, status, reason, evidence=paths) for guest in GUESTS] + [
        check('vm-safety', 'unverified', 'Smoke audit did not complete')]


def recovery_checks(spec, packages, auditors=None):
    if not spec['recovery_checks']:
        return [check('recovery', 'not_selected', 'No recovery suites selected for this candidate', required=False)]
    rows = []
    filenames = {'p41': 'check_uninstall_acceptance', 'p42': 'check_hosts_acceptance',
                 'p43': 'check_browser_acceptance'}
    for entry in spec['recovery_checks']:
        suite, guest, paths = entry['suite'], entry['guest'], entry['summaries']
        identifier = 'recovery:' + guest + ':' + suite
        details = {'evidence': paths}
        if not paths or any(not path.is_file() for path in paths):
            rows.append(check(identifier, 'missing', 'Selected recovery summaries missing', **details))
            continue
        try:
            summaries = [load(path) for path in paths]
            if any(summary.get('package_commit') != spec['commit'] for summary in summaries):
                rows.append(check(identifier, 'mismatch', 'Recovery belongs to another candidate', **details))
                continue
            if any(summary.get('status') == 'running' for summary in summaries):
                rows.append(check(identifier, 'running', 'A selected recovery batch is still running', **details))
                continue
            auditor = auditors[suite] if auditors else module(filenames[suite]).audit
            audit = auditor(paths, qualify=entry.get('qualify', False))
            expected = packages.get(GUEST_PACKAGES[guest])
            if audit.get('scope') != guest + '-' + suite or audit.get('package_commit') != spec['commit']:
                status, reason = 'mismatch', 'Recovery scope/commit differs from selection'
            elif expected is None:
                status, reason = 'unverified', 'Matching package artifact must pass first'
            elif audit.get('package_sha256') != expected['sha256'] or audit.get('installed_version') != expected['version']:
                status, reason = 'mismatch', 'Recovery artifact differs from the selected package'
            elif audit.get('status') != 'pass' or audit.get('all_owned_clones_removed') is not True or audit.get('required_passes', 0) <= 0:
                status, reason = 'fail', 'Recovery evidence or clone-removal audit did not pass'
            elif entry.get('qualify') and audit.get('adapter_qualified') is not True:
                status, reason = 'fail', 'Requested adapter qualification did not pass'
            else:
                status, reason = 'pass', f'{audit["required_passes"]} selected observations verified'
            rows.append(check(identifier, status, reason, **details,
                              adapter_qualified=audit.get('adapter_qualified', False)))
        except FileNotFoundError as error:
            rows.append(check(identifier, 'missing', 'Recovery bundle file missing: ' + str(error.filename), **details))
        except (OSError, ValueError, KeyError, TypeError, AttributeError, StopIteration, AssertionError) as error:
            rows.append(check(identifier, 'fail', str(error), **details))
    return rows


def e2e_details(saved):
    """Read existing host report envelopes; leave test acceptance to their runners."""
    commit = saved.get('package_commit')
    if saved.get('kind') == 'layer3-group-run':
        runs = saved['runs']
        bindings = {row['guest']: saved['packages'][row['guest']] for row in runs}
        complete = (bool(saved.get('finished_at')) and bool(runs) and
                    len(runs) == saved.get('group_count') and all(
                        row.get('status') == 'pass' and row.get('exit_code') == 0 and
                        row.get('evidence_check', {}).get('valid') is True for row in runs))
        return 'layer3/' + saved['suite'], bindings, complete
    if saved.get('kind') == 'layer3-group-audit':
        runs = saved.get('latest_successful_runs', [])
        bindings = {row['guest']: {'commit': commit,
                    'sha256': row['evidence']['package_sha256']} for row in runs}
        complete = (bool(runs) and saved.get('copied_storage_removed') is True and
                    all(row['evidence'].get('valid') is True for row in runs))
        return 'layer3/' + saved['scope'], bindings, complete
    if saved.get('scope') == 'P3.0-smoke-repeatability':
        pairs = saved['pairs']
        runs = {guest: [row for row in saved['runs'][guest] if row['run_id'] in pair['runs']]
                for guest, pair in pairs.items()}
        bindings = {guest: {'commit': commit, 'sha256': rows[-1]['package_sha256']}
                    for guest, rows in runs.items() if rows}
        complete = bool(pairs) and all(pair.get('status') == 'pass' and len(runs[guest]) == 2 and
            all(row.get('valid') is True for row in runs[guest]) for guest, pair in pairs.items())
        return saved['scope'], bindings, complete
    if saved.get('layer') == 'vm-acceptance':
        guest = saved['guest']
        suffix = {'ubuntu': '.deb', 'fedora': '.rpm', 'cachyos': '.pkg.tar.zst'}[guest]
        artifacts = [item for item in saved['package'] if item['filename'].endswith(suffix)]
        if len(artifacts) != 1:
            raise ValueError('Expected one native package in host acceptance report')
        complete = bool(saved.get('finished_at')) and saved.get('base_integrity_verified') is True
        return saved['suite'], {guest: {**artifacts[0], 'commit': commit}}, complete
    scope = saved.get('scope', '')
    if saved.get('layer') == 4:
        guest = saved['guest']
        # The uninstall batch predates the scope field.
        scope = scope or guest + '-p41'
        if not re.fullmatch(re.escape(guest) + r'-(p4[123](?:-.+)?|tier1-protected-edits)', scope):
            raise ValueError('Unknown Layer 4 host suite')
        if 'runs' in saved:
            runs, selected = saved['runs'], saved['selected_scenarios']
            complete = (bool(selected) and len(runs) == len(selected) and
                        {row['scenario'] for row in runs} == set(selected) and all(
                            row.get('status') == 'pass' and row.get('base_integrity_verified') is True and
                            row.get('clone_removed') is True for row in runs))
        else:
            complete = (bool(saved.get('finished_at')) and saved.get('base_integrity_verified') is True and
                        saved.get('clone_removed') is True)
    elif re.fullmatch(r'(ubuntu|fedora|cachyos|native)-p4[123]', scope):
        guest = scope.split('-')[0]
        complete = saved.get('all_owned_clones_removed') is True and saved.get('required_passes', 0) > 0
    else:
        raise ValueError('Not a supported final host E2E acceptance or audit report')
    binding = {'commit': commit, 'sha256': saved.get('package_sha256')}
    for source, target in (('package_filename', 'filename'), ('installed_version', 'version')):
        if source in saved:
            binding[target] = saved[source]
    return scope, {guest: binding}, complete


def e2e_checks(spec, packages):
    rows = []
    for index, path in enumerate(spec.get('e2e_reports', []), 1):
        details = {'evidence': [path]}
        try:
            saved = load(path)
            environments = ([saved['guest']] if saved.get('guest') else
                            sorted(saved.get('packages', {})))
            details.update(suite=saved.get('suite') or saved.get('scope') or 'E2E',
                           environments=environments)
            # Failed audits may omit identities; their failure must still block the candidate.
            if saved.get('status') == 'fail':
                status, reason = 'fail', saved.get('error') or 'E2E runner reported failure'
            elif saved.get('status') != 'pass':
                status, reason = 'incomplete', 'E2E report has not recorded a completed pass'
            else:
                suite, bindings, complete = e2e_details(saved)
                details.update(suite=suite, environments=sorted(bindings), scope=saved.get('scope'))
                if saved.get('package_commit') != spec['commit']:
                    status, reason = 'mismatch', 'E2E report belongs to another candidate'
                elif not complete:
                    status, reason = 'incomplete', 'E2E report lacks completed host acceptance/cleanup'
                elif not bindings or not set(bindings) <= set(GUESTS):
                    raise ValueError('E2E report has no supported guest environment')
                else:
                    status, reason = 'pass', 'Existing E2E acceptance passed for the candidate artifact'
                    for guest, binding in bindings.items():
                        expected = packages.get(GUEST_PACKAGES[guest])
                        if expected is None:
                            if status != 'mismatch':
                                status, reason = 'unverified', 'Matching package artifact must pass first'
                        elif (binding.get('commit') != spec['commit'] or binding.get('sha256') != expected['sha256'] or
                              any(binding[key] != expected[key] for key in ('filename', 'version') if key in binding)):
                            status, reason = 'mismatch', 'E2E artifact differs from the selected package'
            if saved.get('status') == 'pass' and any(saved.get(key) for key in
                    ('error', 'cleanup_error', 'safety_error', 'collection_errors', 'preparation_error')):
                status, reason = 'fail', 'E2E report contains a runner or cleanup error'
            details['report_sha256'] = sha256(path)
        except FileNotFoundError:
            status, reason = 'missing', 'Selected E2E report missing'
        except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
            status, reason = 'fail', 'Invalid E2E report: ' + str(error)
        rows.append(check('e2e:' + str(index), status, reason, **details))
    return rows


def report(spec, receipt=None, *, live=False, smoke_auditor=audit_summaries, recovery_auditors=None):
    packages, identities = package_checks(spec)
    rows = ci_checks(spec, receipt) + packages
    rows += smoke_checks(spec, identities, live=live, auditor=smoke_auditor)
    rows += recovery_checks(spec, identities, auditors=recovery_auditors)
    rows += e2e_checks(spec, identities)
    required = [row for row in rows if row['required']]
    status = ('ready' if all(row['status'] == 'pass' for row in required) else
              'not_ready' if any(row['status'] in ('fail', 'mismatch') for row in required) else 'incomplete')
    return {'schema_version': 1, 'kind': 'release-status', 'policy': 'release-gate-v1',
        'candidate': {key: spec[key] for key in ('name', 'commit', 'repository')},
        'generated_at': dt.datetime.now(dt.timezone.utc).isoformat(),
        'reporter_sha256': sha256(Path(__file__)), 'status': status,
        'full_catalog_acceptance': False, 'ci_observed_at': (receipt or {}).get('observed_at'),
        'checks': rows, 'blocking_checks': [row['id'] for row in required if row['status'] != 'pass'],
        'manifest': str(spec['manifest']), 'manifest_sha256': sha256(spec['manifest'])}


def markdown(result):
    def cell(value):
        return str(value).replace('|', '\\|').replace('\n', ' ').replace('\r', ' ')
    selected = result['candidate']
    lines = [f'# Release status: {cell(selected["name"])}', '',
        f'**{result["status"].upper()}** — `{selected["commit"]}`', '',
        f'Repository: {cell(selected["repository"])}. Generated: {result["generated_at"]}.', '',
        'Scope: source/package CI, six package outputs, three-guest smoke, current VM safety, '
        'and explicitly selected recovery/E2E reports. Full catalog acceptance is not claimed.', '',
        '| Check | Required | Status | Detail |', '| --- | --- | --- | --- |']
    lines += [f'| {cell(row["id"])} | {"yes" if row["required"] else "no"} | {row["status"]} | {cell(row["reason"])} |'
              for row in result['checks']]
    lines += ['', '## Artifacts', '', '| Target | Version | Filename | SHA-256 |', '| --- | --- | --- | --- |']
    lines += [f'| {cell(row["id"])} | {cell(row["artifact"]["version"])} | '
              f'{cell(row["artifact"]["filename"])} | `{row["artifact"]["sha256"]}` |'
              for row in result['checks'] if 'artifact' in row]
    lines += ['', '## Browser and harness versions', '',
              '| Smoke run | Browser | Browser version | Extension version | Harness manifest SHA-256 |',
              '| --- | --- | --- | --- | --- |']
    for row in result['checks']:
        if 'browser_versions' not in row:
            continue
        manifest = row.get('harness_manifest', {})
        digest = hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest() if manifest else 'unavailable'
        for browser, versions in row['browser_versions'].items():
            lines.append(f'| {cell(row["run_id"])} | {cell(browser)} | {cell(versions.get("browser_version", "unavailable"))} | '
                         f'{cell(versions.get("extension_version", "unavailable"))} | `{digest}` |')
    e2e = [row for row in result['checks'] if row['id'].startswith('e2e:')]
    if e2e:
        lines += ['', '## E2E acceptance reports', '', '| Suite | Environment | Status | Report |',
                  '| --- | --- | --- | --- |']
        for row in e2e:
            path = row['evidence'][0]
            lines.append(f'| {cell(row.get("suite", row["id"]))} | '
                         f'{cell(", ".join(row.get("environments", [])) or "unavailable")} | '
                         f'{row["status"]} | [report](<{cell(path)}>) |')
    lines += ['', '## Evidence references', '']
    lines += [f'- {cell(row["id"])}: {cell(path)}' for row in result['checks'] for path in row['evidence']]
    return '\n'.join(lines) + '\n'


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path, help='New report directory; never overwrite evidence')
    parser.add_argument('--github', action='store_true', help='Read pinned CI runs from GitHub and save a receipt')
    parser.add_argument('--live', action='store_true', help='Read current libvirt/base state on the configured VM host')
    args = parser.parse_args(argv)
    try:
        if args.output_dir.exists():
            raise ValueError('Output directory already exists; choose a new report directory')
        spec = candidate(args.candidate)
        receipt = fetch_ci(spec) if args.github else None
        if not args.github and spec['ci_receipt']:
            receipt = load(spec['ci_receipt']) if spec['ci_receipt'].is_file() else None
        result = report(spec, receipt, live=args.live)
        args.output_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
        (args.output_dir / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
        (args.output_dir / 'report.md').write_text(markdown(result))
        if receipt is not None:
            (args.output_dir / 'ci-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    print(result['status'] + ': ' + str(args.output_dir / 'report.md'))
    return 0 if result['status'] == 'ready' else 1
