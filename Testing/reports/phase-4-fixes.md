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
and checksums. Their final results and artifact identifiers will be recorded
after completion.

Passing source/static package checks do not establish installed-package recovery.
The new exact Ubuntu artifact still needs fresh P4.1 and P4.3 validation before
those milestones can be accepted. Earlier candidate failures remain historical
evidence and are not rewritten as passes.
