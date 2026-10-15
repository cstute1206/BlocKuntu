# One-candidate release status

`scripts/release-status.py` combines existing evidence into one report. It reads
files and, when requested, GitHub or libvirt state. It does not build, install,
start VMs, inject faults, remove clones, dispatch workflows or publish releases.

The command writes `report.json` and a readable `report.md` from the same data.
An optional `ci-receipt.json` records the CI snapshot used in the report. Reports
go in a new directory; an existing output directory is rejected.

## Candidate inputs

Copy `config/release-candidate.example.json` into an ignored results directory,
then replace the example name/commit and add the available evidence:

```bash
mkdir -p Testing/results/release-candidates/rc1
cp Testing/config/release-candidate.example.json \
  Testing/results/release-candidates/rc1/candidate.json
```

The candidate requires a name, full source commit and `owner/repository`. It
never infers the candidate from the current branch, dirty working files, package
version alone or the newest passing report in the results directory. All relative
evidence paths resolve from the candidate JSON's directory.

An example of populated input fields (replace every placeholder with real input):

```json
{
  "schema_version": 1,
  "name": "rc1",
  "commit": "REPLACE_WITH_THE_FULL_40_CHARACTER_COMMIT",
  "repository": "cstute1206/BlocKuntu",
  "ci_runs": {"source": 12345, "package": 12346},
  "ci_receipt": null,
  "package_reports": {
    "ubuntu": "../../package-ci/rc1/ubuntu/result.json",
    "debian": "../../package-ci/rc1/debian/result.json",
    "fedora": "../../package-ci/rc1/fedora/result.json",
    "arch": "../../package-ci/rc1/arch/result.json",
    "firefox": "../../package-ci/rc1/firefox/result.json",
    "chrome": "../../package-ci/rc1/chrome/result.json"
  },
  "smoke_summaries": ["../../rc1-smoke-group-run/summary.json"],
  "recovery_checks": [
    {"suite": "p41", "guest": "ubuntu", "summaries": ["../../rc1-p41/summary.json"]}
  ]
}
```

Package reports retain their sibling `artifacts/` directories. Smoke/recovery
summaries retain the raw bundles, captured harnesses and ownership records used
by their existing auditors. Provide every attempt summary in the selected smoke
series, including retries: the latest declared attempt per guest must pass, and
earlier failures remain subject to cleanup checks. The report does not discover
or assert the absence of attempts omitted from the candidate manifest.

Recovery selections support `p41` (uninstall), `p42` (hosts) and `p43` (browser
repair/outage). Every selected entry is required. Unselected catalog cases do
not block the gate and are not represented as passes. Add `"qualify": true` and
two complete comparable batches when adapter qualification is needed. Historical
qualification does not replace the selected candidate's own evidence.

## Commands

Offline report using local evidence and an optional saved CI receipt:

```bash
python3 Testing/scripts/release-status.py \
  --candidate Testing/results/release-candidates/rc1/candidate.json \
  --output-dir Testing/results/release-status/rc1-offline-001
```

Refresh the explicitly selected CI runs using authenticated `gh api`:

```bash
python3 Testing/scripts/release-status.py \
  --candidate Testing/results/release-candidates/rc1/candidate.json \
  --output-dir Testing/results/release-status/rc1-ci-001 --github
```

Use `ci_receipt` to reference that output's receipt in a later offline report.
CI status is always presented with its observation timestamp. There is no invented
age cutoff and no claim that a saved receipt represents current GitHub state.
The current run attempt is queried, so a rerun is not confused with an earlier
attempt. The workflow, repository, exact commit, selected run ID and all eight
source/seven package job outcomes are checked; skipped jobs cannot count as passes.

On the configured VM host, also verify current base integrity and clone inventory:

```bash
python3 Testing/scripts/release-status.py \
  --candidate Testing/results/release-candidates/rc1/candidate.json \
  --output-dir Testing/results/release-status/rc1-live-001 --github --live
```

`--live` performs the existing read-only libvirt/base checks. An offline smoke
audit can establish saved observations but cannot establish current host safety;
the report stays incomplete until that required check passes. No guest is booted.

## Gate and evidence boundaries

The initial `release-gate-v1` policy requires:

- source and package CI for the exact candidate;
- all six existing package outputs (Ubuntu, Debian, Fedora, Arch, Firefox, Chrome);
- all 21 smoke observations per Ubuntu/Fedora/CachyOS guest;
- current VM-host safety checks;
- any explicitly selected recovery suites.

The command checks complete package assertions, recorded clean-build provenance,
and the hashes of all listed artifact files. It distinguishes the Ubuntu and
Debian build reports. Smoke and recovery must match the selected native package's
commit, filename where available, SHA-256 and installed version. Browser/extension
versions and harness manifests from smoke evidence appear in the report. Package
origin remains the existing Layer 2 provenance process's responsibility; this
command does not download or authenticate artifact archives on its own.

The smoke and recovery adapters call existing evidence auditors, rather than
accepting a summary's `pass` field. Evidence for another candidate is a mismatch;
it never fills a missing current result. Missing artifacts/bundles, failed cases,
retained clones, incomplete qualification or contradictory raw evidence cannot
produce a ready result. Full catalog or full Layer 4 acceptance is never implied.

Overall statuses and exit codes:

| Status | Meaning | Exit |
| --- | --- | --- |
| `ready` | Every required check passed for this selected gate | 0 |
| `not_ready` | A required check failed or belongs to a different candidate | 1 |
| `incomplete` | Required evidence/checks are missing, running or unverified | 1 |
| Invalid invocation/configuration | Input cannot define a report, or output already exists | 2 |

Per-check states retain `pass`, `fail`, `missing`, `running`, `unverified`,
`mismatch` and `not_selected` separately. JSON includes the candidate-manifest
checksum, reporter checksum, blocking checks and evidence references. Re-run into
a new output directory after evidence changes. Running VM batches stay incomplete;
their partial results do not substitute for a completed audit.

Regression tests run in the existing harness CI job:

```bash
python3 Testing/scripts/test_release_status.py
python3 Testing/scripts/pr-ci.py harness
```
