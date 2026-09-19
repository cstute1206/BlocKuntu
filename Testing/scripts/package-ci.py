#!/usr/bin/env python3
"""Build and inspect one distribution or browser artifact; never install BlocKuntu."""
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
import tarfile
import zipfile

from package_inspect import CONTRACT, ROOT, inspect_extension, inspect_package, read_package, require, sha256

BUILD_IDS = {'deb': 'PKG-BUILD-001', 'rpm': 'PKG-BUILD-002', 'arch': 'PKG-BUILD-003', 'firefox': 'PKG-BUILD-004', 'chrome': 'PKG-BUILD-004'}
PACKAGE_CASES = [f'PKG-CONTENT-{i:03}' for i in range(1, 11)] + [f'PKG-HARD-{i:03}' for i in range(1, 4)] + ['PKG-ALLOW-001', 'PKG-UPGRADE-001', 'PKG-UPGRADE-002']


def capture(command):
    try:
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False)
        return {'command': command, 'exit_code': result.returncode, 'stdout': result.stdout.strip(), 'stderr': result.stderr.strip()}
    except OSError as error:
        return {'command': command, 'exit_code': 127, 'stderr': str(error)}


def run_command(command, cwd, report, report_dir):
    log = report_dir / f'{len(report["commands"]) + 1:02}-build.log'
    started = time.monotonic()
    print('Running: ' + ' '.join(command), flush=True)
    with log.open('w') as stream:
        try:
            status = subprocess.run(command, cwd=cwd, env={**os.environ, 'CI': 'true', 'CARGO_TERM_COLOR': 'never'}, stdout=stream, stderr=subprocess.STDOUT).returncode
        except OSError as error:
            stream.write(str(error))
            status = 127
    report['commands'].append({'command': command, 'exit_code': status, 'seconds': round(time.monotonic() - started, 2), 'log': log.name})
    print(f'Exit {status}; log: {log}', flush=True)
    require(status == 0, f'Command failed ({status}); see {log.name}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('component', choices=BUILD_IDS)
    parser.add_argument('--artifact', type=Path, help='Inspect an existing distribution artifact; does not claim build provenance')
    parser.add_argument('--allow-dirty', action='store_true', help='Local development only; explicitly mark source as unverified')
    parser.add_argument('--output-dir', type=Path, help='New, empty result directory (default includes timestamp)')
    parser.add_argument('--docker-network', help='Arch only: optional network for disposable Docker builds (for example host)')
    args = parser.parse_args()
    if args.docker_network and args.component != 'arch':
        parser.error('--docker-network only applies to arch')
    kind = args.component
    report_dir = (args.output_dir or ROOT / 'Testing/results/package-ci' / f'{kind}-{time.time_ns()}').resolve()
    report_dir.mkdir(parents=True, exist_ok=True)
    require(not any(report_dir.iterdir()), f'Result directory must be empty: {report_dir}')
    artifacts = report_dir / 'artifacts'
    artifacts.mkdir()
    report = {'layer': 'package-ci', 'component': kind, 'installed_package_acceptance': False,
              'commands': [], 'cases': [], 'artifacts': [], 'passed': False, 'build_provenance_verified': False}
    result = report_dir / 'result.json'
    try:
        require(platform.machine() == 'x86_64', 'Only x86-64 is supported')
        commit = capture(['git', 'rev-parse', 'HEAD'])
        status = capture(['git', 'status', '--porcelain', '--untracked-files=normal'])
        report['commit'] = commit.get('stdout')
        report['source_status'] = status
        report['environment'] = [capture(c) for c in [['uname', '-a'], ['rustc', '--version'], ['cargo', '--version'], ['node', '--version'], ['npm', '--version']]]
        report['os_release'] = Path('/etc/os-release').read_text()
        locks = ['focusd/Cargo.lock', 'native-host/Cargo.lock', 'focus-gui/src-tauri/Cargo.lock', 'focus-gui/package-lock.json'] if kind in CONTRACT['targets'] else [f'browser-extension-{kind}/package-lock.json']
        report['lockfiles'] = {path: sha256(ROOT / path) for path in locks}
        if args.artifact:
            require(kind in CONTRACT['targets'], '--artifact only supports deb, rpm, arch')
            artifact = artifacts / args.artifact.name
            shutil.copyfile(args.artifact, artifact)
            report['cases'].append({'id': BUILD_IDS[kind], 'status': 'skip', 'reason': 'Inspection-only invocation; origin of existing artifact not verified.'})
        else:
            clean = commit['exit_code'] == status['exit_code'] == 0 and not status['stdout']
            require(clean or args.allow_dirty, 'A clean committed checkout is required; use --allow-dirty for development evidence only')
            if clean:
                for path in locks:
                    require(capture(['git', 'ls-files', '--error-unmatch', path])['exit_code'] == 0, f'Lockfile is not committed: {path}')
            report['source_clean'] = clean
            version, revision = CONTRACT['version'], CONTRACT['release']
            ref = os.environ.get('GITHUB_REF', '')
            if ref.startswith('refs/tags/'):
                require(ref == f'refs/tags/v{version}-{revision}', 'Release tag must match vVERSION-RELEASE in package contract')
            if kind in CONTRACT['targets']:
                script = {'deb': 'package-deb.sh', 'rpm': 'package-rpm.sh', 'arch': 'package-arch-docker.sh'}[kind]
                command = ['bash', str(ROOT / 'scripts' / script), '--version', version + '-' + revision if kind == 'deb' else version, '--output-dir', str(artifacts)]
                if kind != 'deb':
                    command += ['--release', revision]
                if args.docker_network:
                    command += ['--network', args.docker_network]
                run_command(command, ROOT, report, report_dir)
                pattern = {'deb': '*.deb', 'rpm': '*.rpm', 'arch': '*.pkg.tar.*'}[kind]
                candidates = [p for p in artifacts.glob(pattern) if not p.name.endswith(('.sha256', '.sig'))]
                require(len(candidates) == 1, f'Expected exactly one binary package, found {len(candidates)}')
                artifact = candidates[0]
            else:
                cwd = ROOT / f'browser-extension-{kind}'
                run_command(['npm', 'ci'], cwd, report, report_dir)
                run_command(['npm', 'run', 'package:amo' if kind == 'firefox' else 'package:zip'], cwd, report, report_dir)
                artifact = artifacts / ('BlocKuntu.xpi' if kind == 'firefox' else 'BlocKuntu-Chrome.zip')
                shutil.copyfile(cwd / artifact.name, artifact)
            require(report['lockfiles'] == {path: sha256(ROOT / path) for path in locks}, 'Build changed a lockfile')
            if clean:
                require(not capture(['git', 'status', '--porcelain', '--untracked-files=normal'])['stdout'], 'Build changed the clean source checkout')
            report['build_provenance_verified'] = clean
            report['cases'].append({'id': BUILD_IDS[kind], 'status': 'pass'})
        if kind in CONTRACT['targets']:
            entries, meta, hooks = read_package(artifact, kind)
            report['package_metadata'] = meta
            (report_dir / 'inventory.json').write_text(json.dumps({path: {key: value for key, value in entry.items() if key != 'data'} for path, entry in entries.items()}, indent=2) + '\n')
            (report_dir / 'hooks.json').write_text(json.dumps(hooks, indent=2) + '\n')
            report['cases'] += inspect_package(entries, meta, hooks, kind)
        else:
            report['extension_version'] = inspect_extension(artifact, kind)
            report['cases'].append({'id': 'PKG-META-001', 'status': 'pass'})
    except (OSError, ValueError, KeyError, tarfile.TarError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        report['error'] = str(error)
        report['cases'].append({'id': 'PKG-RUN', 'status': 'fail', 'reason': str(error)})
    finally:
        # Preserve checksums even when a later inspection or build step fails.
        try:
            for path in sorted(artifacts.iterdir()):
                if path.is_file() and not path.name.endswith('.sha256'):
                    report['artifacts'].append({'filename': path.name, 'sha256': sha256(path), 'bytes': path.stat().st_size})
            if report['artifacts']:
                (report_dir / 'SHA256SUMS').write_text(''.join(f'{item["sha256"]}  artifacts/{item["filename"]}\n' for item in report['artifacts']))
                report['cases'].append({'id': 'PKG-META-003', 'status': 'pass'})
        except OSError as error:
            report['cases'].append({'id': 'PKG-META-003', 'status': 'fail', 'reason': str(error)})
        expected = [BUILD_IDS[kind], 'PKG-META-001', 'PKG-META-003']
        if kind in CONTRACT['targets']:
            expected += ['PKG-META-002', *PACKAGE_CASES]
        recorded = {case['id'] for case in report['cases']}
        for identifier in expected:
            if identifier not in recorded:
                report['cases'].append({'id': identifier, 'status': 'blocked', 'reason': report.get('error', 'Prerequisite failed')})
        report['passed'] = all(case['status'] in ['pass', 'skip'] for case in report['cases'])
        result.write_text(json.dumps(report, indent=2) + '\n')
        summary = ['# Package CI: ' + kind, '', 'Result: ' + ('PASS' if report['passed'] else 'FAIL'), '', 'Static artifact checks only; no installed-package acceptance.', '', '| Case | Result | Detail |', '| --- | --- | --- |']
        summary += [f'| {case["id"]} | {case["status"]} | {case.get("reason", "").replace(chr(10), " ").replace("|", "/")} |' for case in report['cases']]
        (report_dir / 'report.md').write_text('\n'.join(summary) + '\n')
        print(f'Report: {result}', flush=True)
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
