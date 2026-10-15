#!/usr/bin/env python3
"""Run only implemented Layer 3 fresh-clone groups, one guest at a time."""
import argparse
import json
from pathlib import Path

from layer3_contract import SUITES
from layer3_group_runner import execute, preflight


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=SUITES, default='smoke')
    parser.add_argument('--guest', action='append', choices=('ubuntu', 'fedora', 'cachyos'))
    parser.add_argument('--case', action='append', dest='case_ids', metavar='CASE-ID')
    parser.add_argument('--browser', action='append', choices=('firefox-native', 'chrome-native'))
    parser.add_argument('--scenario', action='append', help='Explicit representative scenarios of selected cases')
    parser.add_argument('--run-prefix', required=True)
    parser.add_argument('--verified-inputs', required=True, type=Path)
    parser.add_argument('--preflight-only', action='store_true',
                        help='Validate groups, exact artifacts and unused IDs without starting a VM')
    args = parser.parse_args()
    try:
        prepared = preflight(args.suite, args.guest, args.run_prefix, args.verified_inputs,
                             case_ids=args.case_ids, browsers=args.browser, scenarios=args.scenario)
        if args.preflight_only:
            print(json.dumps({'status': 'preflight_pass', 'suite': args.suite,
                              'groups': prepared['groups']['groups'],
                              'package_commit': prepared['commit']}, indent=2))
            return 0
        path, summary = execute(prepared)
        print(f"Group run {summary['status']}: {path}")
        return int(summary['status'] != 'pass')
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))


if __name__ == '__main__':
    raise SystemExit(main())
