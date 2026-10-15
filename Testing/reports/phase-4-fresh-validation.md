# Fresh Ubuntu P4.1 and P4.3 validation

Status: **fresh validation complete; both confirmed fixes verified; full P4.1
and P4.3 acceptance withheld**. Each batch passed 11/12 observations. Upgrade
testing remains deferred.

The installed candidate is the Ubuntu DEB from package workflow
[`37609821265`](https://github.com/cstute1206/BlocKuntu/actions/runs/37609821265),
commit `a3162aadb6d6c27cad78835d6bcbbe2274db05c6`, version `0.2.0-1`, SHA-256
`495ec78c0ce40d04f83b2b585c7b3ada97812d8d53f8cb8046b56c6248eaa2ae`.
Every selected fault uses a new, independent Ubuntu clone and fresh browser
profiles. No candidate binary, service unit or installed extension is patched
to manufacture recovery.

## Assertions fixed before the fresh run

The P4.1 Firefox sentinel was already absent in the old immediate pre-uninstall
checkpoint. The new fixture compares that immediate checkpoint with the
post-uninstall inspection and reports `not_covered` if the sentinel is missing
before removal. This does **not** satisfy the preservation assertion or turn a
failed full milestone into a pass. It distinguishes missing coverage from a
setting demonstrably lost during uninstall. All 26 uninstall and 17 browser
harness checks passed before the fresh run; runtime sources are then frozen.

The P4.3 sustained-outage assertion still requires automatic recovery of the
original browser processes. Explicitly reopening the same profiles is recorded
as diagnostic recovery and cannot satisfy that assertion. The new package
does not add reconnect grace; this behavior remains a separate decision.

## Preflight and VM safety

[Preflight](../results/p4-fresh-20261007-preflight.json) verifies the exact DEB
and stopped bases. Under the runner lock, two redundant, diagnosed old A crash
and outage clones were removed; their original failure verdicts and evidence
remain intact. The later B diagnostic clones remain available. New successful
clones are removed with ownership checks; failed clones are stopped and retained.

## Fresh results

### P4.1: confirmed cleanup fixed; full preservation coverage incomplete

The [fresh batch](../results/p41-ubuntu-20261007-a/summary.json) completed with
11/12 observations passing. The [independent raw-evidence analysis](../results/p4-fresh-20261007-p41-analysis.json)
checks the captured-source hashes, candidate identity, apt refusal and installed
postconditions, immediate before/after files, native purge logs, hosts attributes,
unit state and original-browser recovery. Runtime sources match across all three
scenarios.

| Fresh clone | Result |
| --- | --- |
| `p41-ubuntu-20261007-a-remove` | 2/2: apt exit 100 with genuine refusal, no lease, package still installed, hosts/process blocking intact. Clone removed. |
| `p41-ubuntu-20261007-a-purge` | 2/2: the same intended refusal and intact enforcement on a separate clone. Clone removed. |
| `p41-ubuntu-20261007-a-authorized` | 7/8: wrong phrase and active schedule/Detox rejection, natural expiry, GUI/native purge and original-browser recovery pass. Cleanup removes all managed state; Firefox same-file sentinel coverage is missing. Clone stopped and retained. |

The exact three user manifests and three Snap host/launcher copies from the old
defect existed immediately before uninstall and were all absent afterward. No
system/user native manifest or copied host remains, managed units/state are gone,
and `/etc/hosts` is unlocked with unrelated rows preserved. The unrelated Chrome
policy survives. Both original browser PIDs survive and their management pages
and formerly blocked HTTP routes recover beyond the failure tolerance; the two
recovery screenshots were also visually checked.

The Firefox sentinel is absent in both immediate before and after inspections.
Its preservation status is `not_covered`, so `DES-UNINSTALL-006` remains failed
as a coverage requirement. This is not evidence of uninstall losing that setting.
The pre-uninstall policy handling and the hook's whole-file deletion risk remain
unresolved; a valid same-file preservation test is needed. No full P4.1 acceptance
or adapter qualification is claimed, and a second qualification batch would not
resolve this missing baseline.

### P4.3

The [fresh four-scenario batch](../results/p43-ubuntu-20261007-a/summary.json)
completed with 11/12 observations passing. The
[independent raw-evidence analysis](../results/p4-fresh-20261007-p43-analysis.json)
verifies effective faults, captured source/extension hashes, timing constants,
socket state, current-fault diagnostics, browser/profile identities and actual
baseline/outage/recovery pages. All four clones used identical archived runtime
sources and published extension code.

| Fresh clone | Result |
| --- | --- |
| `p43-ubuntu-20261007-a-policy` | 3/3: both managed policies automatically repaired; original sessions recover current heartbeats and navigation. Clone removed. |
| `p43-ubuntu-20261007-a-manifest` | 3/3: effective connection loss, strict closure and declared restore/reopen recovery pass. Logs contain 14 Firefox and 15 Chrome disconnects while paths are damaged. Clone removed. |
| `p43-ubuntu-20261007-a-crash` | 3/3: automatic restart preserves the RPC socket; both original browser processes recover fresh heartbeats and free/named-blocked pages. Clone removed. |
| `p43-ubuntu-20261007-a-outage` | 2/3: sustained pause and fail-closed existing/new navigation pass; original-session automatic recovery fails after strict closure. Same-profile diagnostic reopen succeeds. Clone stopped and retained. |

The crash changed daemon PID `1405` to `16156`. The socket pathname remained a
socket in both during-fault and recovered inspections. Both browsers produced
three current-fault transport diagnostics; the first sampled healthy post-fault
RPC completed 7.826 seconds after the fault began, including the six-second
pre-pause. Firefox and Chrome retained their original browser PIDs. This verifies
the confirmed RPC restart fix on the installed candidate.

The sustained pause lasted 161.362 seconds through the visible page checks,
exceeding the installed 105-second observation requirement. All four existing/new
page observations show actual fail-closed pages while the daemon remains paused;
the existing-tab screenshots were also visually checked. On resume, the strict
guard records Firefox/Chrome heartbeat ages of 178/176 seconds against its
30-second grace and kills both browsers. Reopening the exact original profiles
restores current-session heartbeats and free/named-blocked navigation without
extension changes. This does not satisfy automatic original-session recovery.

The separate outage behavior decision remains open: accept strict closure and
explicit same-profile reopen as recovery, or require bounded reconnect grace on
daemon resume. No expectation or product behavior was changed during this run.
Full P4.3 acceptance and adapter qualification remain withheld. A second full
qualification batch was not run because the first complete batch fails that
criterion. Historical results remain unchanged in the
[earlier validation report](phase-4-validation.md).

## Final acceptance and safety audit

The [P4.1 acceptance audit](../results/p4-fresh-20261007-p41-acceptance-audit.json)
and [P4.3 acceptance audit](../results/p4-fresh-20261007-p43-acceptance-audit.json)
correctly reject the incomplete full milestones. The separate analyses confirm
the two repaired defects without relabeling either failed batch as a pass.

The [final safety inventory](../results/p4-fresh-20261007-final-safety.json)
verifies all 51 historical/new owned records: 47 domains/copied disks removed,
four retained clones stopped. The seven fresh scenario clones comprise five
removed successes and two stopped incomplete/failing clones. All Ubuntu, Fedora
and CachyOS bases are stopped with unchanged definitions/storage metadata; no
acceptance runner remains active. Free space is 53.43 GiB. Evidence remains
available for every removed clone.

Observed browser versions were Firefox `157.0.1` and Chrome `154.0.8037.57`, with
published extension versions `0.2.8` and `0.2.6`. P4.2, other distributions,
confined-browser runtime/revisions and upgrades were not rerun in this selection.
