# Package fixes from Ubuntu P4.1 and P4.3

The candidate from workflow run `36269349435`, commit
`605d707b92cc5fa007674169f0e74ce6fc879c33`, left six per-user Native Messaging
artifacts after GUI-authorized purge. Automatic daemon restart also left the
RPC socket pathname missing while the socket unit remained active. These are
the two confirmed failures addressed by this change.

## Changes

Final package removal invokes a shared per-user cleanup helper in DEB, RPM and
Arch hooks. It removes BlocKuntu's known native manifests and confined-browser
host/launcher copies, including retained Snap revisions. It preserves browser
profiles and unrelated files. Account enumeration uses the system's `UID_MIN`;
root, system accounts and nobody are excluded. Each cleanup runs with that
account's privileges before traversing user-controlled paths. The packages
declare the `util-linux` dependency that provides `runuser`.

The daemon unit sets `RuntimeDirectoryPreserve=restart` so automatic restart
retains the socket pathname owned by the still-active socket unit. Normal final
stop/removal retains its existing cleanup. This follows the documented
[systemd restart semantics](https://raw.githubusercontent.com/systemd/systemd/main/man/systemd.exec.xml).

Direct package-manager uninstall still requires the GUI-issued removal lease.
No outage reconnect grace, policy-loss recovery or allowance-loss behavior is
changed. Upgrade testing remains deferred.

## Validation and build scope

All 26 package-inspector regression checks passed locally, including real DEB
and Arch fixture assembly, the six-file cleanup reproduction, unrelated-file
preservation, Flatpak/Snap revision coverage, privilege dropping and rejection of
packages missing the new helper or runtime preservation setting. Shell syntax
and patch whitespace checks passed. The local environment used Ubuntu archive
tools, umask 022 and RPM v4 fixture format to match the Ubuntu 24.04 inspector
environment; host RPM 6 defaults to a format unsupported by its archive reader.

CI builds originate from a clean dedicated branch based on the tested candidate;
earlier uncommitted local testing/extension changes are preserved separately.
The source and package workflows retain commit-bound build reports, archives
and checksums. All 14 CI jobs completed successfully; final artifact verification is recorded below.

Passing source/static package checks do not establish installed-package recovery.
The new exact Ubuntu artifact passed fresh validation of both confirmed fixes.
The complete P4.1 and P4.3 milestones remain withheld for Firefox preservation
coverage and sustained-outage automatic recovery; see the
[fresh Ubuntu validation report](phase-4-fresh-validation.md). Earlier candidate
failures remain historical evidence and are not rewritten as passes.

## Completed CI artifacts

Build commit: `a3162aadb6d6c27cad78835d6bcbbe2274db05c6` on `fix/package-uninstall-rpc-recovery`.

[Source validation](https://github.com/cstute1206/BlocKuntu/actions/runs/37609767757) passed 7/7 jobs.
[Package validation/build](https://github.com/cstute1206/BlocKuntu/actions/runs/37609821265) passed 7/7 jobs, including
26 inspector regressions, four Linux builds and both extension builds.

All four Linux artifact archives were downloaded and verified against GitHub
artifact digests, commit-bound clean-build reports and individual package
checksums. Independent inspection of each downloaded package also passed.
The package version remains `0.2.0-1`; identify this candidate by its exact
build commit and checksum.

| Target | Package SHA-256 |
| --- | --- |
| ubuntu | `495ec78c0ce40d04f83b2b585c7b3ada97812d8d53f8cb8046b56c6248eaa2ae` |
| debian | `77c8f79b7af41dd1841d98fcbd4138130ccfc91723de0e2278b9effd1425fd2e` |
| fedora | `4db87578ce700e4de37167d102379e1a1cc68eea589b22cec5ba427456182358` |
| arch | `d27a157f5f159b67bf40fe1859c4be3a1c157516aa25798e613cfaeffb0fe20d` |

[Local build evidence](../results/p4-fixes-20261007/implementation.json) records
artifact paths, provenance and verification. Fresh installed-package P4.1/P4.3
runs and independent evidence audits are complete in the
[fresh Ubuntu validation report](phase-4-fresh-validation.md). Each batch passed
11/12 observations; both repaired defects are verified, with full milestone
acceptance and adapter qualification withheld for the remaining criteria.
