# Phase 1: PR source validation

The workflow `.github/workflows/ci.yml` runs seven independent jobs: core,
daemon, native host, Tauri Rust, GUI, Firefox extension, and Chrome extension.
It runs on pull requests, pushes to main, and manual dispatch. Matrix failures
do not cancel sibling jobs. Only the Tauri job installs desktop compile libraries.

Local execution evidence: [reports/phase-1-validation.md](reports/phase-1-validation.md).

## Local commands

Run from the repository root with Rust (rustfmt and Clippy), Python 3, and
Node 22. CI pins Rust 1.93.0, the locally validated toolchain, to keep
Clippy upgrades explicit. The Tauri job requires the Linux compile dependencies listed in CI.

```bash
python3 Testing/scripts/pr-ci.py core
python3 Testing/scripts/pr-ci.py daemon
python3 Testing/scripts/pr-ci.py native
python3 Testing/scripts/pr-ci.py tauri
python3 Testing/scripts/pr-ci.py gui --install
python3 Testing/scripts/pr-ci.py firefox --install
python3 Testing/scripts/pr-ci.py chrome --install
```

`--install` runs `npm ci`; omit it for local runs with dependencies already
installed from the current lockfile. Cargo dependency resolution always uses
`--locked`. Rust jobs check formatting, Clippy with warnings denied, all test
targets, and doctests. Frontend jobs run tests, production compilation, and
Svelte or manifest checks. These commands do not build distributable packages,
install BlocKuntu, launch its daemon, or operate VMs.

Tests use temporary databases/files, synthetic process snapshots and explicit
clock values. Extension tests execute the actual compiled scripts in a Node VM
with mocked browser APIs, Native Messaging, and timers. Both browsers run the
same scenarios; their configured heartbeat timeouts differ intentionally.
GUI tests mount Svelte components in jsdom and test user actions and warning
content. The Add all detected handler uses the same tested identity helper.
GUI setup follows the [Svelte Testing Library setup](https://testing-library.com/docs/svelte-testing-library/setup/).

## Evidence and traceability

`Testing/results/pr-ci/<component>/` contains numbered command logs and a
`result.json` with exit codes and durations. GUI tests additionally emit JUnit
XML. CI uploads reports after success or failure for 14 days. Missing reports
on runner/setup failure remain visible in the Actions log.

[pr-ci-coverage.json](pr-ci-coverage.json) maps all 57 Layer 1 cases to test
names and files. The core job verifies the mapping and tests runner failure
propagation. To audit the mapping locally:

```bash
python3 Testing/scripts/check-pr-coverage.py
python3 Testing/scripts/test_pr_ci.py
```

The mapping is a traceability check, not automatic proof of assertion quality.
Review it when changing policy behavior. There are no deferred Layer 1 cases.

## Decisions and boundaries

- Tier 3 manual unlocks remain available during targeted Detox. Normal reason,
  rolling-hour quota, and two-minute expiry rules still apply. Tier 2 remains
  strict. `PR-AAL-016` reflects this confirmed product decision.
- Process rejection, independent helper identities, exemptions and kill requests
  are tested using synthetic snapshots and a recording killer. Actual SIGTERM
  delivery, minimized desktop behavior, and scan scheduling require VM acceptance.
- Extension tests do not prove browser permission handling, actual navigation
  interception, store installation, or Chrome service-worker suspension behavior.
- GUI tests prove DOM behavior, not a running Tauri window or visual layout.
- The Phase 0 consolidated VM acceptance issue does not block these source jobs.
- Required merge checks must be selected in GitHub repository rules after the
  new workflow has run. No repository settings are changed by this implementation.
