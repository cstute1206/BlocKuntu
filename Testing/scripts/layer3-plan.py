#!/usr/bin/env python3
"""Print the required Layer 3 observations without starting or changing a VM."""
import argparse
import json
from pathlib import Path

from layer3_contract import SUITES, build_plan, load_contract


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=SUITES, default='smoke')
    parser.add_argument('--guest', action='append', choices=('ubuntu', 'fedora', 'cachyos'))
    parser.add_argument('--case', dest='cases', action='append', help='Exact case ID; repeat as needed')
    parser.add_argument('--browser', action='append', help='Exact package variant ID; repeat as needed')
    parser.add_argument('--output', type=Path, help='Write JSON to a new file; never overwrite evidence')
    args = parser.parse_args()
    try:
        registry, matrix = load_contract()
        plan = build_plan(registry, matrix, suite=args.suite, guests=args.guest,
                          case_ids=args.cases, browsers=args.browser)
        if args.output:
            with args.output.open('x') as destination:
                destination.write(json.dumps(plan, indent=2) + '\n')
        print(f"{plan['suite']} ({plan['scope']}): {plan['case_count']} cases, "
              f"{plan['required_count']} required observations")
        print('Implementation: ' + json.dumps(plan['implementation_counts'], sort_keys=True))
        print('Acceptance: not evaluated (planning only; no VM was started)')
        if args.output:
            print(args.output)
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))


if __name__ == '__main__':
    main()
