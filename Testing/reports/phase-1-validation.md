# Phase 1 local validation — 2026-09-18

All seven source-validation jobs passed locally using Rust 1.93.0 and Node
22.13.0. The GitHub Actions workflow itself has not yet run remotely.

| Job | Automated tests | Other checks |
| --- | ---: | --- |
| focus-core | 70 | rustfmt, Clippy with warnings denied, doctests |
| focusd | 113 | rustfmt, Clippy with warnings denied, doctests |
| native-host | 14 | rustfmt, Clippy with warnings denied, doctests |
| Tauri Rust | 9 | rustfmt, Clippy with warnings denied, doctests |
| Svelte GUI | 4 | clean npm ci, production build, Svelte check |
| Firefox extension | 7 | clean npm ci, TypeScript build, manifest check |
| Chrome extension | 7 | clean npm ci, TypeScript build, manifest check |

Additionally, all four CI-runner regression tests passed, and the coverage
checker found test declarations for all 57 Layer 1 IDs. These counts describe
test functions; several functions exercise tables of policy inputs.

Machine-readable command results and logs are under
`Testing/results/pr-ci/<component>/`. They are generated, Git-ignored evidence;
the workflow publishes corresponding artifacts. GUI tests also produce JUnit
XML. Workflow YAML parsing and `git diff --check` passed.

The GUI type check reports zero errors and zero warnings. Development-mode
component tests emit existing Svelte warnings for the non-reactive arrays used
to store input DOM references (`matcherValueInputs`, `patternValueInputs`);
these do not fail the interaction assertions.

The tests confirm Tier 3 manual unlocks during targeted Detox, exact unlock
expiry without replenishing exhausted allowance, independent process identity,
recovery exemptions, overlapping restrictions, and protected allowlist edits.
Small Clippy fixes retain behavior (derived defaults, first-match selection,
redundant borrows/closures, and equivalent control flow). Add all detected now
calls a tested helper shared with the GUI tests.

This is source validation. No distributable package was built, no installed
BlocKuntu service was changed, and no VM acceptance was performed. Actual
signal delivery, browser runtime integration, service-worker suspension and
installed GUI behavior remain later-layer acceptance work.
