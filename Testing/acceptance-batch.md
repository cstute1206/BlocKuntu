# Native acceptance batches

Use a new frozen run directory containing `candidate.json`,
`vm-command-plan.json`, `verified-inputs.json`, `base-inventory-before.json`,
`harness-inputs.json`, `candidate-application-manifest.json` and
`runner-inputs.json`. Paths in manifests are relative to the repository.
The directory also contains the recorded `run-layer3.py` and
`run-acceptance.py` preparation wrappers. The candidate supplies successful CI
receipts and exact Debian, Fedora, Arch and extension package reports.

```bash
python3 Testing/scripts/acceptance-batch.py \
  --run-dir Testing/results/NEW-FROZEN-RUN \
  --continue-on-failure
```

The default stops at the first failed suite. `--continue-on-failure` runs the
next independent suite after an assertion, setup, runner or audit failure.
Every suite uses fresh clone/run IDs. Failed reports remain failed and their
acceptance files remain intact. Owned failed clones are shut down and removed
under the same ownership and base-integrity guards before another suite starts.
Dependent steps inside a failed journey remain blocked; they are not reported
as passing. Layer 4 scenario batches already continue ordinary scenario
failures while stopping on safety errors.

Changed base VMs, unexpected domains, broken ownership/cleanup, changed frozen
inputs or artifact-provenance mismatches always stop the coordinator. Both
Layer 3 verified inputs and direct Layer 4 `--package-report` arguments must
select the candidate's exact artifact for the guest. Ubuntu uses the Debian
report. A copied plan still pointing at an earlier candidate is rejected before
starting a VM.

`progress.json` and `progress.md` retain each suite outcome, logs and audit
exit codes. `cleanup/` holds guarded cleanup receipts. A complete batch with
any failed suite ends as `completed_with_failures`, returns nonzero and does
not make the release ready. Final release reporting also checks explicitly
selected reports. Upgrades remain deferred; unimplemented catalog cases are
excluded from the executable plan.
