# Firefox policy preservation and bounded browser recovery

The two remaining criteria from the [fresh P4.1/P4.3 validation](phase-4-fresh-validation.md)
have approved source fixes at commit
[`15a3248c37e956532e7f8046f7ac3be0cb555101`](https://github.com/cstute1206/BlocKuntu/commit/15a3248c37e956532e7f8046f7ac3be0cb555101),
on `fix/package-uninstall-rpc-recovery`. Local verification and all 14 source
and package CI jobs passed. Fresh installed-package validation verifies Firefox
preservation and automatic sustained-outage recovery. P4.1 is qualified with
24/24 across two fresh complete batches; P4.3 completed 11/12, with acceptance
withheld for a process-exit/logging race.

## Firefox preservation

Native Firefox now uses the existing shared-policy merge manager rather than
replacing the whole document. Before the first change, it records the original
values of the six managed policy keys, BlocKuntu's extension entry and the
quarantined-domains preference. Other policy keys, extensions and preferences
remain in the document through activation and subsequent repairs.

The protected backup defaults to
`/var/lib/blockuntu/browser-policy-backups/firefox.json`, beside the daemon's
database. `--firefox-policy-backup` can override it for a separate deployment.
The backup survives daemon restarts and is not replaced during repair.

GUI `prepare_uninstall` restores previous managed values and removes entries
BlocKuntu originally added. Unrelated additions made while BlocKuntu was active
remain. If BlocKuntu created the document and nothing unrelated remains, the
document is removed. DEB, RPM and Arch removal hooks no longer delete the shared
Firefox document after the GUI has restored it. Direct package-manager removal
continues to require the GUI-issued lease.

An already-compliant legacy document without a backup is adopted by identifying
the managed settings, rather than backing them up as unrelated configuration.
Settings already destroyed by an earlier whole-document writer cannot be
recovered retrospectively. Invalid policy JSON is reported without overwriting
the file. Confined-browser policy/revision testing and package upgrade testing
remain outside this selection.

## Browser grace and recovery

The default `strict_mode.grace_seconds` and the packaged preset are now **60**.
Explicit existing configuration values remain unchanged. First browser startup
already had a minimum 60-second opportunity; packaged manual onboarding retains
its existing deferral until the first extension heartbeat.

After a daemon restart, an existing browser session with a current-session
heartbeat can receive a **60-second reconnect window** if that heartbeat is now
stale. After a sustained process-scan gap, only browsers previously observed
healthy are eligible. The gap threshold is the greater of the configured stale
grace and twice the normal scan interval, so ordinary scans do not continually
grant recovery windows.

The window defers only the strict stale-extension process kill. It does not
refresh stored heartbeats, relax extension fail-closed behavior, or bypass
application blocklists/allowlists. A fresh heartbeat completes recovery for that
browser. Without one, strict closure resumes after the monotonic deadline.
Another gap cannot extend an outstanding or expired window; a verified fresh
heartbeat must rearm eligibility. Start, completion and expiry are recorded as
`browser_reconnect_grace_started`, `browser_reconnect_grace_completed` and
`browser_reconnect_grace_expired` events.

## Verification and build state

The clean build branch passed the full core and daemon source gates: formatting,
Clippy with warnings denied, all-target tests and documentation tests. Core ran
70 Rust tests; daemon ran 122. Core's four CI-runner regression checks and all
57 source-contract mappings also passed. The 27 package-inspector regression
checks passed, including rejection of a native hook deleting the shared Firefox
policy document. Shell syntax and whitespace checks for changed product files
passed. The working checkout also passed its 26 package regressions; it predates
one unrelated RPM-manifest regression present on the clean build branch.

Focused coverage verifies Firefox preservation through repair, restart and the
real uninstall RPC, previous-value restoration, unrelated additions, missing
and malformed policies, legacy adoption, sustained-pause and restart recovery,
window expiry without extension, independent browser recovery, rearming after a
successful reconnect, ordinary extension failure and startup behavior.

- [Source CI](https://github.com/cstute1206/BlocKuntu/actions/runs/37634221871)
- [Package build/validation](https://github.com/cstute1206/BlocKuntu/actions/runs/37634225607)
- [Local implementation and source-check evidence](../results/p4-policy-recovery-20261007/implementation.json)

Both runs passed 7/7 jobs and are pinned to the commit above. The Ubuntu archive
digest, clean build provenance and package checksum were verified, followed by
independent static inspection. Its DEB SHA-256 is
`02c4edd0f5a10f96c9a90fd1cae5579bb9402be6444d550085cdaa1ca2d29777`.
Fresh Ubuntu P4.1/P4.3 results are recorded in the
[revalidation report](phase-4-revalidation.md), with build verification in
[the new evidence directory](../results/p4-revalidate-20261007/ubuntu-artifact-verification.json).
The saved `a3162aa` batches remain 11/12 each; implementation and passing
source/static checks do not convert them into passes.
