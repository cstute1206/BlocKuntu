"""Ordinary-user pre-reboot policy and post-reboot process/browser witnesses."""
import argparse
import json
import os
from pathlib import Path
import subprocess

from guest import WORK, rpc, wait

URL = 'http://web.blockuntu.test:18080/prefix/blocked'
NAME = 'Acceptance persisted reboot website'


def seed(run_id):
    assert os.geteuid() != 0, 'Policy must be created as the desktop user'
    site = {'id': run_id + '-reboot-site', 'name': NAME, 'tier': 'hard',
            'patterns': [{'kind': 'exact_url', 'value': URL}]}
    app = {'id': run_id + '-reboot-app', 'name': 'Acceptance persisted reboot process',
           'tier': 'hard', 'matchers': [{'kind': 'command_name', 'value': 'bk-test-helper'}]}
    rpc('upsert_site_list', {'rule': site})
    rpc('upsert_app_rule', {'rule': app})
    answer = rpc('evaluate_url', {'url': URL, 'probe': True})
    assert answer['decision'] == 'block' and answer['reason']['rule_id'] == site['id']
    record = {'site_rule_id': site['id'], 'app_rule_id': app['id'],
              'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
              'created_by_uid': os.geteuid(), 'url': URL}
    (WORK / 'reboot-policy.json').write_text(json.dumps(record, indent=2) + '\n')
    return record


def check_process():
    record = json.loads((WORK / 'reboot-policy.json').read_text())
    current = Path('/proc/sys/kernel/random/boot_id').read_text().strip()
    assert current != record['boot_id'], 'Policy was not created before this boot'
    config = rpc('config_snapshot')
    assert any(r['id'] == record['site_rule_id'] for r in config['rules'])
    assert any(r['id'] == record['app_rule_id'] for r in config['app_rules'])
    answer = rpc('evaluate_url', {'url': URL, 'probe': True})
    assert answer['decision'] == 'block' and answer['reason']['rule_id'] == record['site_rule_id']
    process = subprocess.Popen([str(WORK / 'bk-test-helper'), '--lifetime', '600'],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        wait(lambda: process.poll() is not None or
             Path(f'/proc/{process.pid}/comm').read_text().strip() == 'bk-test-helper', 5)
        assert process.poll() is None, 'Fixture died before identity could be verified'
        path = Path(f'/proc/{process.pid}')
        identity = {'pid': process.pid, 'exe': str((path / 'exe').readlink()),
                    'start_ticks': (path / 'stat').read_text().rsplit(')', 1)[1].split()[19]}
        wait(lambda: process.poll() is not None, 30)
        assert process.returncode in (0, -15, -9), process.returncode
        return {'pre_reboot_policy': record, 'new_boot_id': current,
                'persisted_process_terminated': True, 'process_identity': identity,
                'persisted_site_rpc_blocked': True}
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--run-id', required=True)
    args = parser.parse_args()
    print(json.dumps(seed(args.run_id)))
