#!/usr/bin/env python3
"""Require every Layer 1 case to reference an existing automated test declaration.

This checks traceability, not assertion quality or whether tests passed.
"""
import json
from pathlib import Path
import re

root = Path(__file__).resolve().parents[2]
cases = set(re.findall(r'\| (PR-[A-Z]+-\d{3}) \|', (root / 'Testing/layers/01-pr-ci.md').read_text()))
coverage = json.loads((root / 'Testing/pr-ci-coverage.json').read_text())
assert cases == set(coverage), f'Missing: {cases - set(coverage)}; obsolete: {set(coverage) - cases}'
for case, entry in coverage.items():
    entries = [entry, *entry.get('additional', [])]
    for entry in entries:
        source = (root / entry['file']).read_text()
        assert entry['tests'], f'{case}: no tests'
        for test in entry['tests']:
            if entry['file'].endswith('.rs'):
                declaration = rf'#\[test\]\s*fn {re.escape(test)}\('
            else:
                declaration = rf"test\(['\"]{re.escape(test)}['\"]"
            assert re.search(declaration, source), f'{case}: missing test declaration {test}'
print(f'All {len(cases)} Layer 1 cases reference automated tests. Assertion coverage requires review.')
