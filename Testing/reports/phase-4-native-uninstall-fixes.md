# Native uninstall transaction fixes and candidate validation

Status: **implementation, candidate CI and selected fresh native validation
complete. Fedora and CachyOS P4.1/P4.2/P4.3 each passed two complete fresh runs
and their independent qualification audits on `dd6ad80`.**

## Changes

- Fedora: require the DNF5 Actions plugin and run a lease check with
  `raise_error=1` before an erase transaction. A refusal must abort before
  dependencies are removed. RPM's existing `%preun` lease consumption remains
  the final-removal backstop.
- CachyOS: install an ALPM `PreTransaction` removal hook with `AbortOnFail`.
  Correct the install script's rejection branches to return failure immediately.
  The transaction guard checks the lease without consuming it; the removal
  script consumes it once.
- P4.2 harness: validate the captured measurement interval against the schedule
  boundaries, independently of later checkpoint collection. Give the initial
  inactive observation at least 30 seconds of lead time. Reject measurements
  that actually cross a boundary.
- P4.1 evidence: capture and independently compare the complete native package
  inventory before and after refusal, require a freshly restarted healthy GUI,
  and check removal of the new guard files after GUI-authorized uninstall.
- P4.3 harness: allow twelve seconds before SIGKILL, covering the installed
  five-second request cycle and three-second RPC timeout with scheduling margin.
  Reject unsupported installed timings and insufficient actual root-witnessed
  pauses. Still require current-fault transport errors from both browsers.

The guard mechanisms follow the official [DNF5 Actions documentation](https://dnf5.readthedocs.io/en/stable/libdnf5_plugins/actions.8.html)
and [ALPM hook manual](https://man.archlinux.org/man/alpm-hooks.5.en).
Upgrade testing remains deferred.

## Candidate

- Commit: `dd6ad807daa50b02354c11d2f2fda4a37af9b6cf`
- Branch: `fix/native-uninstall-transactions`
- Parent: previously validated `9df72e0749a01d2019ed3116eb693a1f5e573980`.
- [Package build](https://github.com/cstute1206/BlocKuntu/actions/runs/38133806817)
- [Source validation](https://github.com/cstute1206/BlocKuntu/actions/runs/38133806669)

Both workflows passed all seven jobs. Native artifact archives passed GitHub
digest, ZIP integrity/path safety, source provenance, exact package SHA-256 and
local package-inspector checks before VM installation.

| Native package | SHA-256 |
| --- | --- |
| Fedora RPM | `e18187050e03d98d80be719c18f006753a95c0f59c46989de0aaa705e7662edc` |
| Arch package | `4e7daf5044ef5be0135afc135f7b271944842cd622b93959fd5eea85322f510d` |

The isolated build checkout retains the earlier Firefox manifest, uninstall
cleanup, browser grace, RPC restart and process-kill fixes. Unrelated changes
and the existing staged deletion in the main checkout are preserved.

## Checks and selected fresh runs

Local checks passed: 27 package-inspector regressions, 6 lease/scriptlet
regressions, 100 Layer 4 harness tests and 29 VM-harness tests. Native installed
behavior requires the following fresh-clone evidence on this exact candidate:

| Distribution | Selection | Fresh complete runs | Status |
| --- | --- | --- | --- |
| Fedora | Layer 3 smoke prerequisite | 1 | 21/21 passed; independent audit passed; clone removed |
| CachyOS | Layer 3 smoke prerequisite | 1 | 21/21 passed; independent audit passed; clone removed |
| Fedora | P4.1 protected and GUI-authorized uninstall | 2 | 12/12 + 12/12 passed; complete pair audit passed; six clones removed; qualified |
| CachyOS | P4.1 protected and GUI-authorized uninstall | 2 | 12/12 + 12/12 passed; complete pair audit passed; six clones removed; qualified |
| Fedora | P4.2 hosts repair with corrected schedule harness | 2 | 11/11 + 11/11 passed; complete pair audit passed; ten clones removed; qualified |
| CachyOS | P4.2 hosts repair with corrected schedule harness | 2 | 11/11 + 11/11 passed; complete pair audit passed; ten clones removed; qualified |
| Fedora | P4.3 browser repair and outage recovery | 2 | 12/12 + 12/12 passed; complete pair audit passed; eight clones removed; qualified |
| CachyOS | P4.3 browser repair and outage recovery | 2 | 12/12 + 12/12 passed; complete pair audit passed; eight clones removed; qualified |

The accepted selection totals **140/140 destructive observations plus 42/42
smoke observations**, across twelve destructive batches and two smoke runs.
This qualifies the selected native P4.1/P4.2/P4.3 adapters on this artifact pair;
it does not claim complete Layer 4 acceptance or a new Ubuntu runtime pass.

| Selection | Fedora run IDs | CachyOS run IDs |
| --- | --- | --- |
| Smoke | `p30-fd-1011-a` | `p30-cy-1011-a` |
| P4.1 | `p41-fd-1011-a`, `p41-fd-1011-b` | `p41-cy-1011-a`, `p41-cy-1011-b` |
| P4.2 | `p42-fd-1011-a`, `p42-fd-1011-b` | `p42-cy-1011-a`, `p42-cy-1011-b` |
| P4.3 | `p43-fd-1011-b`, `p43-fd-1011-c` | `p43-cy-1011-b`, `p43-cy-1011-c` |

The Fedora DNF refusal now aborts before erasing BlocKuntu or its GUI dependencies.
Direct RPM refusal also passes. CachyOS `pacman -R` and `pacman -Rns` both abort
in the pre-transaction hook. Every refusal preserves the complete installed
package inventory and a freshly reopened healthy GUI. GUI-authorized removal
passes on both distributions, including the new guard-file cleanup, Firefox
policy preservation and original-browser communication recovery.

P4.1 and P4.2 need comparable pairs because their evidence/harness changed.
P4.3 was qualified on the previous candidate. Its initial ordinary Fedora check
on this candidate completed 11/12: crash recovery passed, but the six-second
pre-pause did not make Chrome time out. Its late request completed in 2.871
seconds, below its three-second RPC timeout; Firefox reported a real timeout.
Requiring Chrome's transport-error diagnostic therefore correctly failed.
The original outcome and powered-off failed clone were retained at validation
completion. The corrected
injector now requires two complete fresh qualification batches per distribution;
the earlier failing batch cannot contribute to qualification. Package identity
and acceptance requirements are unchanged. CachyOS clones are fully refreshed while BlocKuntu is absent;
this does not test application upgrades. Suites run serially. Passing clones
must be removed; failed evidence remains intact. Original bases must stay off
and unchanged.

Private receipts, exact artifact checksums, captured harness checksums, logs,
qualification audits and final safety checks belong under
`Testing/results/p4-native-fixes-20261011/` and
`Testing/results/p4-native-p43-20261011/`. Previous failed observations in
[the native port report](phase-4-native-ports.md) retain their original outcomes.

## Final safety

All **50 clones contributing to the accepted selection** were removed, including
their copied storage and libvirt definitions. The initial Fedora P4.3 check used
four additional clones: its three passing clones were removed and its failed
crash-diagnostic clone was retained powered off with the original evidence. At
validation completion, 53 of 54 clones were removed; the retained clone was explicitly
excluded from acceptance.

The final live checks verified that the Ubuntu, Fedora and CachyOS bases stayed
off and unchanged, the pinned harness sources stayed unchanged during each
qualification run, and the original main-checkout staging was preserved.
The safety receipts are `interrupted-validation-safety.json` in the first
results directory, `final-safety.json` in the P4.3 directory, and
`combined-final-validation.json` in the first directory.
Upgrade testing and P4.4–P4.8 remain deferred.

## Subsequent VM cleanup — 2026-10-14

At the operator's request, all seven remaining disposable test VMs were removed,
including the failed preliminary crash clone and six older diagnostic clones.
Their exact owned disks and NVRAM copies were deleted, freeing 71.29 GiB.
All saved acceptance evidence remained byte-for-byte unchanged. Only the original
Ubuntu, Fedora and CachyOS domains remain; they stayed powered off and unchanged.
The cleanup receipt is `Testing/results/vm-cleanup-20261014/cleanup.json`.
Historical test outcomes and qualification receipts are unchanged.
