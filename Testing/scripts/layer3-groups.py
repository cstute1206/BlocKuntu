#!/usr/bin/env python3
"""Plan safe Layer 3 clone boundaries; never create or start a VM."""
import argparse
import json
from pathlib import Path

from layer3_contract import SUITES, build_plan, load_contract
from layer3_groups import build_execution_groups


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=SUITES, default='smoke')
    parser.add_argument('--guest', action='append', choices=('ubuntu', 'fedora', 'cachyos'))
    parser.add_argument('--case', action='append', dest='cases')
    parser.add_argument('--browser', action='append')
    parser.add_argument('--output', type=Path, help='Write a new JSON file; never overwrite evidence')
    args = parser.parse_args()
    try:
        registry, matrix = load_contract()
        plan = build_plan(registry, matrix, suite=args.suite, guests=args.guest,
                          case_ids=args.cases, browsers=args.browser)
        groups = build_execution_groups(plan)
        if args.output:
            with args.output.open('x') as destination:
                destination.write(json.dumps(groups, indent=2) + '\n')
        print(f"{groups['group_count']} fresh-clone groups for {groups['required_observation_count']} "
              f"observations; {groups['runnable_group_count']} groups have an executor")
        print('Acceptance: not evaluated (planning only; no VM was started)')
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))


if __name__ == '__main__':
    main()
