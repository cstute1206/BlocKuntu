#!/usr/bin/env python3
"""Recheck saved Layer 3 smoke group summaries; optionally inspect live libvirt."""
import argparse
import json
import os
from pathlib import Path

from layer3_group_audit import audit_summaries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', action='append', required=True, type=Path,
                        help='Saved group summary; repeat to include retries')
    parser.add_argument('--guest', action='append', choices=('ubuntu', 'fedora', 'cachyos'))
    parser.add_argument('--live', action='store_true',
                        help='Also verify current immutable bases, clone removal, and libvirt inventory')
    parser.add_argument('--output', type=Path, help='Write a new JSON audit; never overwrite evidence')
    args = parser.parse_args()
    try:
        audit = audit_summaries(args.summary, guests=args.guest, live=args.live)
        if args.output:
            descriptor = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, 'w') as destination:
                json.dump(audit, destination, indent=2)
                destination.write('\n')
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    print(json.dumps({'status': audit['status'],
                      'required_observations_passed': audit['required_observations_passed'],
                      'earlier_attempts': audit['earlier_attempts'],
                      'live_inventory_check': audit['live_inventory_check']}, indent=2))
    return int(audit['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
