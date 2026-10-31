"""Run independent native acceptance suites serially, retaining every failure."""
from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

import release_status

ROOT = Path(__file__).resolve().parents[2]
RESULTS = ROOT / 'Testing/results'
GUESTS = {'ubuntu', 'fedora', 'cachyos'}
LAYER4 = {'tier1-edit-acceptance.py', 'uninstall-acceptance.py',
          'hosts-acceptance.py', 'browser-acceptance.py'}


class SafetyError(RuntimeError):
    """A broken ownership, provenance or immutable-input invariant stops the run."""


def read(path):
    return json.loads(Path(path).read_text())


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    path = Path(path)
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def option(command, name):
    if command.count(name) != 1 or command.index(name) + 1 >= len(command):
        raise ValueError('Expected one ' + name)
    return command[command.index(name) + 1]


def validate_plan(tasks, spec, run_dir):
    """Bind every layer to the candidate, including direct Layer 4 commands."""
    if not tasks or len({t['name'] for t in tasks}) != len(tasks):
        raise ValueError('Acceptance tasks must be nonempty with unique names')
    if len({t['run_id'] for t in tasks}) != len(tasks):
        raise ValueError('Every task requires a fresh unique run ID')
    for task in tasks:
        guest, command = task['guest'], task['command']
        if guest not in GUESTS or not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,39}', task['run_id']):
            raise ValueError('Invalid acceptance guest or run ID')
        if not re.fullmatch(r'[a-z0-9][a-z0-9-]*', task['name']):
            raise ValueError('Invalid acceptance task name')
        if option(command, '--guest') != guest:
            raise ValueError('Command guest differs from task')
        report = spec['package_reports'][release_status.GUEST_PACKAGES[guest]]
        if task['layer'] == 4:
            if Path(command[1]).resolve() != (run_dir / 'run-acceptance.py').resolve() or command[2] not in LAYER4:
                raise ValueError('Unexpected Layer 4 executor')
            if Path(option(command, '--package-report')).resolve() != report.resolve():
                raise ValueError('Layer 4 package report differs from candidate: ' + task['name'])
            run_id = option(command, '--run-id')
        elif task['layer'] == 3:
            if Path(command[1]).resolve() != (run_dir / 'run-layer3.py').resolve():
                raise ValueError('Unexpected Layer 3 executor')
            inputs_path = run_dir / 'verified-inputs.json'
            if Path(option(command, '--verified-inputs')).resolve() != inputs_path.resolve():
                raise ValueError('Layer 3 inputs are not this run snapshot')
            inputs = read(inputs_path)
            if inputs['workflow']['headSha'] != spec['commit']:
                raise ValueError('Layer 3 inputs belong to a different candidate')
            records = [r for r in inputs['packages'] if r['guest'] == guest]
            if len(records) != 1 or Path(records[0]['report']).resolve() != report.resolve():
                raise ValueError('Layer 3 package report differs from candidate')
            run_id = option(command, '--run-prefix')
        else:
            raise ValueError('Only implemented Layers 3 and 4 are supported')
        if run_id != task['run_id']:
            raise ValueError('Command run ID differs from task')
        if not Path(task['report']).resolve().is_relative_to(RESULTS.resolve()):
            raise ValueError('Acceptance report must stay under Testing/results')


def execute_plan(tasks, backend, *, continue_on_failure=False):
    """Continue normal suite/audit failures; never continue a safety failure."""
    state = {'status': 'running', 'pid': __import__('os').getpid(), 'started_at': now(),
             'continue_on_failure': continue_on_failure,
             'planned_runner_invocations': len(tasks), 'commands': []}
    backend.publish(state)
    try:
        for task in tasks:
            if backend.stop_requested():
                state['status'] = 'stopped_after_current'
                break
            backend.check()
            row = {**task, 'status': 'running', 'started_at': now()}
            state['commands'].append(row)
            state['current'] = task['name']
            backend.publish(state)
            try:
                code = backend.invoke(task, row)
            except OSError as error:
                code = 127
                row['error'] = 'Runner could not start: ' + str(error)
            row['exit_code'] = code
            row['status'] = 'pass' if code == 0 else 'fail'
            evidence = backend.inspect(task)
            row['evidence'] = evidence
            if evidence.get('status') != 'pass':
                row['status'] = 'fail'
            if task.get('audit'):
                row['audit_exit_code'] = backend.audit(task)
                if row['audit_exit_code']:
                    row['status'] = 'fail'
            backend.cleanup(task)
            row['finished_at'] = now()
            backend.publish(state)
            backend.check()
            if row['status'] == 'fail' and not continue_on_failure:
                state['status'] = 'stopped_on_failure'
                break
        else:
            state['status'] = ('completed_with_failures' if any(r['status'] != 'pass' for r in state['commands'])
                               else 'completed')
    except SafetyError as error:
        state.update(status='stopped_for_safety', safety_error=str(error))
        if state['commands'] and not state['commands'][-1].get('finished_at'):
            state['commands'][-1].update(status='fail', safety_error=str(error), finished_at=now())
    except Exception as error:
        state.update(status='stopped_for_safety', safety_error='Coordinator failed: ' + type(error).__name__ + ': ' + str(error))
        if state['commands'] and not state['commands'][-1].get('finished_at'):
            state['commands'][-1].update(status='fail', safety_error=state['safety_error'], finished_at=now())
    state['failure_count'] = sum(r['status'] == 'fail' for r in state['commands'])
    state['finished_at'] = now()
    backend.publish(state)
    return state


class NativeBackend:
    def __init__(self, run_dir, spec, vm):
        self.directory, self.spec, self.vm = run_dir, spec, vm
        self.baseline = {'bases': read(run_dir / 'base-inventory-before.json')}
        self.manifests = [read(run_dir / name) for name in
                          ('harness-inputs.json', 'candidate-application-manifest.json', 'runner-inputs.json')]
        self.identities = {}
        self.state = None

    def stop_requested(self):
        return (self.directory / 'stop-after-current').exists()

    def check(self):
        try:
            for manifest in self.manifests:
                for path, expected in manifest.items():
                    if digest(ROOT / path) != expected:
                        raise ValueError('Frozen run input changed: ' + path)
            self.vm.unchanged(self.baseline)
            names = set(self.vm.virsh('list', '--all', '--name').stdout.split())
            if names != {r['domain'] for r in self.vm.CONFIG['templates'].values()}:
                raise ValueError('Unexpected domain remains before the next independent suite')
            if shutil.disk_usage(ROOT).free < 20 * 1024 ** 3:
                raise ValueError('Less than 20 GiB free for the next disposable clone')
            checks, self.identities = release_status.package_checks(self.spec)
            if any(r['status'] != 'pass' for r in checks):
                raise ValueError('Candidate package validation failed')
            if any(r['status'] != 'pass' for r in release_status.ci_checks(
                    self.spec, read(self.spec['ci_receipt']))):
                raise ValueError('Candidate CI identity no longer validates')
        except (OSError, ValueError, RuntimeError, KeyError) as error:
            raise SafetyError(str(error)) from error

    def invoke(self, task, row):
        print('START ' + task['name'], flush=True)
        with (self.directory / 'logs' / (task['name'] + '.log')).open('x') as log:
            child = subprocess.Popen(task['command'], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
            row['child_pid'] = child.pid
            self.publish(self.state)
            return child.wait()

    def audit(self, task):
        with (self.directory / 'logs' / (task['name'] + '-audit.log')).open('x') as log:
            return subprocess.run(task['audit']['command'], cwd=ROOT, stdout=log,
                                  stderr=subprocess.STDOUT).returncode

    def inspect(self, task):
        try:
            result = read(task['report'])
        except (OSError, ValueError) as error:
            return {'status': 'fail', 'reason': 'Missing or unreadable host report: ' + str(error)}
        if not isinstance(result, dict):
            return {'status': 'fail', 'reason': 'Host report is not an object'}
        if result.get('safety_error') or result.get('base_integrity_verified') is False:
            raise SafetyError('Runner reported a safety/base-integrity failure: ' + task['name'])
        if result.get('package_commit') is None:
            return {'status': 'fail', 'reason': 'Host report lacks candidate provenance'}
        if result['package_commit'] != self.spec['commit']:
            raise SafetyError('Runner reported a different candidate: ' + task['name'])
        if task['layer'] == 4 and result.get('package_sha256') != self.identities[
                release_status.GUEST_PACKAGES[task['guest']]]['sha256']:
            raise SafetyError('Runner reported a different package artifact: ' + task['name'])
        if result.get('status') != 'pass':
            return {'status': 'fail', 'reason': result.get('error') or 'Suite reported failure'}
        if any(result.get(key) for key in ('error', 'cleanup_error', 'collection_errors', 'preparation_error')):
            return {'status': 'fail', 'reason': 'Passing host report contains runner/collection errors'}
        try:
            suite, bindings, complete = release_status.e2e_details(result)
            for guest, identity in bindings.items():
                expected = self.identities[release_status.GUEST_PACKAGES[guest]]
                if identity.get('commit') != self.spec['commit'] or identity.get('sha256') != expected['sha256']:
                    raise SafetyError('Passing report differs from the pinned artifact')
            if not bindings or not complete:
                return {'status': 'fail', 'reason': 'Host acceptance or cleanup is incomplete'}
            return {'status': 'pass', 'suite': suite, 'report_sha256': digest(task['report'])}
        except (KeyError, ValueError, TypeError, AttributeError) as error:
            return {'status': 'fail', 'reason': 'Invalid host acceptance: ' + str(error)}

    def cleanup(self, task):
        """Remove only this suite's owned clones; preserve acceptance bytes."""
        vm, receipts = self.vm, []
        try:
            with (vm.RUNTIME / 'runner.lock').open('w') as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                vm.unchanged(self.baseline)
                for owner_path in sorted(vm.RUNTIME.glob(task['run_id'] + '*/*/owner.json')):
                    run_id = owner_path.parent.parent.name
                    if run_id != task['run_id'] and not run_id.startswith(task['run_id'] + '-'):
                        continue
                    owner = read(owner_path)
                    if owner['run_id'] != run_id or owner['template'] != task['guest']:
                        raise ValueError('Clone ownership differs from the selected suite')
                    if owner['stage'] == 'removed':
                        if any(Path(p).exists() for p in owner['paths']):
                            raise ValueError('Removed clone still has copied storage')
                        continue
                    vm.guarded(owner)
                    acceptance = Path(owner['evidence']) / 'acceptance'
                    before = {str(p.relative_to(acceptance)): digest(p) for p in acceptance.rglob('*')
                              if p.is_file() and not p.is_symlink()}
                    if vm.virsh('domstate', owner['uuid']).stdout.strip() != 'shut off':
                        vm.virsh('shutdown', owner['uuid'])
                        try:
                            vm.wait_until(lambda: vm.virsh('domstate', owner['uuid']).stdout.strip() == 'shut off',
                                          'owned failed-clone shutdown', seconds=60)
                        except TimeoutError:
                            vm.guarded(owner)
                            vm.virsh('destroy', owner['uuid'])
                    vm.cleanup(owner)
                    after = {str(p.relative_to(acceptance)): digest(p) for p in acceptance.rglob('*')
                             if p.is_file() and not p.is_symlink()}
                    if before != after:
                        raise ValueError('Acceptance evidence changed during clone cleanup')
                    receipts.append({'run_id': run_id, 'clone_uuid': owner['uuid'], 'removed': True,
                                     'acceptance_files_sha256': before, 'acceptance_evidence_unchanged': True})
                vm.unchanged(self.baseline)
            save(self.directory / 'cleanup' / (task['name'] + '.json'),
                 {'base_integrity_verified': True, 'cleaned_after_suite': receipts})
        except (OSError, ValueError, RuntimeError, KeyError) as error:
            raise SafetyError('Cannot safely continue after ' + task['name'] + ': ' + str(error)) from error

    def publish(self, state):
        self.state = state
        state['candidate_commit'] = self.spec['commit']
        state['failure_count'] = sum(r['status'] == 'fail' for r in state['commands'])
        save(self.directory / 'progress.json', state)
        rows = {r['name']: r for r in state['commands']}
        tasks = read(self.directory / 'vm-command-plan.json')['commands']
        lines = ['# Native acceptance batch', '', '**' + state['status'] + '**', '',
                 'Candidate: `' + self.spec['commit'] + '`', '',
                 'Continue after ordinary suite failures: **' + str(state['continue_on_failure']) + '**.',
                 'Failures remain failures. Ownership, base integrity, provenance and cleanup errors stop execution.', '',
                 '| Suite | Guest | Status | Log |', '| --- | --- | --- | --- |']
        for task in tasks:
            row = rows.get(task['name'], {})
            lines.append('| ' + task['name'] + ' | ' + task['guest'] + ' | ' + row.get('status', 'queued') +
                         ' | [log](logs/' + task['name'] + '.log) |')
        lines += ['', 'Upgrades remain deferred. Unimplemented catalog cases are excluded.', '',
                  '[Candidate](candidate.json) · [Coverage](coverage.json)']
        if state.get('release_report'):
            lines += ['', '[Release status](' + state['release_report'] + ')']
        (self.directory / 'progress.md').write_text('\n'.join(lines) + '\n')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-dir', type=Path, required=True)
    parser.add_argument('--continue-on-failure', action='store_true')
    args = parser.parse_args(argv)
    directory = args.run_dir.resolve()
    if not directory.is_relative_to(RESULTS.resolve()) or directory.is_symlink():
        parser.error('Run directory must be a new snapshot under Testing/results')
    spec = release_status.candidate(directory / 'candidate.json')
    tasks = read(directory / 'vm-command-plan.json')['commands']
    validate_plan(tasks, spec, directory)
    module = importlib.util.spec_from_file_location('batch_vm', Path(__file__).with_name('phase0-vm.py'))
    vm = importlib.util.module_from_spec(module)
    module.loader.exec_module(vm)
    backend = NativeBackend(directory, spec, vm)
    backend.check()
    for task in tasks:
        if Path(task['report']).parent.exists() or any(vm.RUNTIME.glob(task['run_id'] + '*/*/owner.json')):
            raise ValueError('Run ID already has evidence or ownership: ' + task['run_id'])
    for name in ('logs', 'audits', 'cleanup'):
        (directory / name).mkdir(exist_ok=False)
    state = execute_plan(tasks, backend, continue_on_failure=args.continue_on_failure)
    try:
        with (directory / 'release-status-final.log').open('x') as log:
            status = subprocess.run([sys.executable, str(ROOT / 'Testing/scripts/release-status.py'),
                '--candidate', str(directory / 'candidate.json'), '--output-dir',
                str(directory / 'release-status-final'), '--live'], cwd=ROOT,
                stdout=log, stderr=subprocess.STDOUT).returncode
        state.update(release_report='release-status-final/report.md', release_report_exit_code=status,
                     release_status=read(directory / 'release-status-final/report.json')['status'])
    except (OSError, ValueError, KeyError) as error:
        state['release_report_error'] = str(error)
    backend.publish(state)
    return int(state['status'] != 'completed' or state.get('release_status') != 'ready')
