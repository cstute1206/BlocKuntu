# Ubuntu P4.1/P4.3 revalidation after policy and recovery fixes

Status: **P4.1 qualified with 24/24 across two fresh complete batches.
P4.3 completed 11/12; Firefox preservation and automatic sustained-outage
recovery are verified, with full P4.3 acceptance withheld for a newly exposed
process-exit/logging race.** Upgrade testing remains deferred.

## Exact candidate

The tested Ubuntu DEB is from
[package workflow 37634225607](https://github.com/cstute1206/BlocKuntu/actions/runs/37634225607),
commit `15a3248c37e956532e7f8046f7ac3be0cb555101`, version `0.2.0-1`, SHA-256
`02c4edd0f5a10f96c9a90fd1cae5579bb9402be6444d550085cdaa1ca2d29777`.
[Source CI](https://github.com/cstute1206/BlocKuntu/actions/runs/37634221871)
and package CI each passed all seven jobs. The archive digest, clean build
provenance and package checksum were verified before transfer; independent
static inspection of the downloaded DEB passed.

See [artifact verification](../results/p4-revalidate-20261007/ubuntu-artifact-verification.json),
[independent inspection](../results/p4-revalidate-20261007/ubuntu-independent-inspection/result.json)
and the [validation record](../results/p4-revalidate-20261007/validation.json).
All 62 Layer 4 harness regressions passed before execution. Runtime sources and
installed published extension code stayed identical across the four P4.3
scenarios. No package binary or installed extension was patched to produce a pass.

## P4.1

The [first batch](../results/p41-recheck-20261007-a/summary.json) passed **12/12**
on three independent clones. Its [complete-bundle audit](../results/p4-revalidate-20261007/p41-a-acceptance-audit.json)
and [independent file/process audit](../results/p4-revalidate-20261007/p41-a-raw-audit.json)
also passed. The [second batch](../results/p41-recheck-20261007-b/summary.json)
and its [independent raw audit](../results/p4-revalidate-20261007/p41-b-raw-audit.json)
also passed 12/12. The [qualification audit](../results/p4-revalidate-20261007/p41-qualification-audit.json)
confirms **24/24** across two comparable full batches, with all six clones removed.

Direct `apt remove` and `apt purge` returned the intended refusal (exit 100),
with the package installed and hosts/process blocking still working. The GUI
journey verified wrong-phrase rejection, schedule/Detox protection, natural
expiry, the real leased native purge and complete managed cleanup.

Firefox's unrelated `DisableTelemetry: true` setting survived activation and
was present immediately before uninstall. After uninstall the shared document
matched the original seeded policy exactly; BlocKuntu's managed entries were
gone. Unrelated hosts rows and the Chromium sentinel also survived. Both
original browser processes retained access to management pages and formerly
blocked HTTP routes beyond the post-uninstall failure tolerance. The recovered
browser screenshots were visually checked.

## P4.3

The [four-scenario batch](../results/p43-recheck-20261007-a/summary.json) completed
with **11/12** observations passing. The [independent analysis](../results/p4-revalidate-20261007/p43-observation-analysis.json)
checks candidate identity, captured-source and installed-code hashes, fault
evidence, recovery identities and clone safety. The [full acceptance audit](../results/p4-revalidate-20261007/p43-acceptance-audit.json)
correctly rejects the failed batch. No second full qualification batch is run
while the required manifest observation fails.

| Scenario | Result |
| --- | --- |
| Policy repair | 3/3: both documents repaired automatically; original browsers recover fresh heartbeats and allowed/named-blocked pages. Clone removed. |
| Broken manifests | 2/3: effective fault, observed browser closure and declared same-profile recovery; required strict-termination event missing. Clone stopped and retained. |
| Daemon crash | 3/3: automatic restart preserves the RPC socket and both original browser processes recover. Clone removed. |
| Sustained outage | 3/3: existing/new pages fail closed during the pause; both original sessions recover automatically. Clone removed. |

### Sustained-outage recovery now works

The observed pause lasted **155.308 seconds**, exceeding the installed
**105-second** observation requirement. Both existing tabs and new navigations
showed actual daemon-unavailable/heartbeat-missing fail-closed pages while the
daemon was paused. All four screenshots were visually checked.

On resume, the daemon recorded a 60-second reconnect window for Firefox and
Chrome at `16:27:45.900140 UTC`; both completed at `16:27:48.453114 UTC`, about
**2.553 seconds** later. Firefox PID `5332` and Chrome PID `7712` remained
unchanged, as did their session IDs, profiles and installed extension code.
No diagnostic profile reopening occurred. Fresh current-session heartbeats and
allowed/named-blocked navigation returned; both recovered block-page screenshots
were visually checked. See the [independent outage audit](../results/p4-revalidate-20261007/outage-raw-audit.json).

The [crash audit](../results/p4-revalidate-20261007/crash-raw-audit.json) separately
confirms daemon PID `1416` changed to `17070`, the RPC pathname remained a socket,
and the first sampled healthy post-fault RPC arrived after 7.596 seconds,
including the six-second pre-pause. Both original browser processes survived.

### Remaining failure: process exits during a kill batch

`DES-BROWSER-002` fails because its required `browser_killed_extension_stale`
event is absent. Both browsers actually closed, and restoring the original
manifests plus reopening the original profiles recovered them without extension
reinstallation. The test's expected closure behavior worked; its required
auditable termination record did not.

The daemon reported `process scan failed: process 19634 could not be killed:
errno 3`. Linux error 3 is `ESRCH`, meaning the target process had already gone.
The source treats that normal race as fatal: `LinuxSignalKiller::kill` returns
an error, `kill_processes` aborts and drops its earlier successful signal events,
and `scan_processes_once` returns before persisting them. Source inspection
matches the exact candidate. The error is present in both the independent
journal and exported diagnostics; no strict-termination event exists in the
final event log either.

See the [failure analysis](../results/p4-revalidate-20261007/manifest-failure-analysis.json)
and the [retained manifest bundle](../results/p43-recheck-20261007-a-manifest/ubuntu/acceptance/result.json).
The next source fix should handle an already-gone process as an expected race,
continue enforcing the remaining processes, and preserve truthful events for
successful signals. The evidence requirement stays intact. This needs a new
build and fresh manifest/full-P4.3 validation before acceptance or qualification.

## Scope and safety

These results cover native Firefox/Chrome and the selected Ubuntu variants.
P4.2, other distributions, confined-browser runtime/revisions, policy/allowance
loss and upgrades were not rerun. Earlier candidate failures remain historical
in the [previous fresh validation report](phase-4-fresh-validation.md).
The [final safety audit](../results/p4-revalidate-20261007/final-safety.json)
passed. Of the ten new scenario clones, nine successful clones were removed and
the failed manifest clone remains stopped for diagnosis. The prior four retained
Phase 4 clones also remain stopped. The scoped inventory checked 61 Phase 4
ownership records and their storage: 56 removed, five retained and stopped.
All three base VMs remain stopped and their definitions/storage metadata match
preflight. No guests or acceptance runners remain active; 37.654 GiB is free.
Older unrelated Layer 3 ownership records are outside this inventory.
