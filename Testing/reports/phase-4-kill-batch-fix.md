# Process kill batch outcomes and Ubuntu P4.3 validation

Status: **implemented, built and validated. Ubuntu P4.3 is qualified with 24/24
across two complete fresh batches; the separate focused manifest run passed 3/3.**

The previous manifest fault closed both browsers but failed `DES-BROWSER-002`
because no `browser_killed_extension_stale` event was persisted. A process exited
between the procfs snapshot and its SIGTERM; `ESRCH` aborted the batch and discarded
successful signal events. The [previous validation](phase-4-revalidation.md)
remains historical evidence for commit `15a3248`.

## Implementation

Commit [`9df72e0749a01d2019ed3116eb693a1f5e573980`](https://github.com/cstute1206/BlocKuntu/commit/9df72e0749a01d2019ed3116eb693a1f5e573980)
on `fix/process-kill-batch` returns a report containing successful signal events,
already-gone PIDs and per-target failures. Every distinct selected target is
attempted in its original order. Only `ESRCH` is classified as already gone;
genuine signal errors remain failures. Already-gone targets receive no kill event
or block notification.

The daemon persists successful events and their existing notifications before
returning an aggregate error for failed targets. The existing process-scan error
path records that diagnostic. A successful signal event retains its existing
meaning; the VM suite separately verifies visible browser closure.

Four deterministic regression tests cover mixed success/disappearance/failure,
error classification, audit persistence before error reporting, and a batch where
all targets already exited. The complete daemon check passed formatting, Clippy
with warnings denied, 126 tests and doctests. All 62 Layer 4 harness tests passed.
The working checkout has the same three changed source files as the clean
candidate; existing staged changes were preserved.

See [implementation and local checks](../results/p4-kill-batch-20261009/implementation.json).

## Exact build

- [Source CI: 7/7 passed](https://github.com/cstute1206/BlocKuntu/actions/runs/37907525866)
- [Package CI/build: 7/7 passed](https://github.com/cstute1206/BlocKuntu/actions/runs/37907529116)

The tested Ubuntu DEB is version `0.2.0-1`, SHA-256
`8cb234574d94538848ecb3699c369da2961f926a3b795565dcd1a944b531b76a`.
Its GitHub archive digest, clean build provenance and package checksum were
verified before installation. [Artifact verification](../results/p4-kill-batch-20261009/ubuntu-artifact-verification.json)
and [independent static inspection](../results/p4-kill-batch-20261009/ubuntu-independent-inspection/result.json)
passed. No installed package or extension was patched to obtain a pass.

## Fresh Ubuntu runtime results

The [focused manifest run](../results/p43-kill-20261009-focus/summary.json) passed
3/3 and its clone was removed. Its [independent raw audit](../results/p4-kill-batch-20261009/focused-raw-audit.json)
confirmed current-fault strict-termination events for both browsers, no process-scan
error, and same-profile recovery without extension reinstallation.

Both [complete batch A](../results/p43-kill-20261009-a/summary.json) and
[complete batch B](../results/p43-kill-20261009-b/summary.json) passed 12/12.
Their acceptance audits and independent raw audits passed:
[A acceptance](../results/p4-kill-batch-20261009/p43-a-acceptance-audit.json),
[A raw](../results/p4-kill-batch-20261009/p43-a-raw-audit.json),
[B acceptance](../results/p4-kill-batch-20261009/p43-b-acceptance-audit.json),
[B raw](../results/p4-kill-batch-20261009/p43-b-raw-audit.json).

| Scenario | Batch A | Batch B | Verified behavior |
| --- | --- | --- | --- |
| Policy repair | 3/3 | 3/3 | Both managed documents repaired automatically; original browser sessions recover fresh heartbeats and allowed/named-blocked pages. |
| Manifest damage | 3/3 | 3/3 | Both damaged paths remain effective during the fault; strict closure and current-fault termination events agree. Explicit restoration and same-profile reopening recover both browsers. |
| Daemon crash | 3/3 | 3/3 | Daemon restarts, RPC pathname remains a socket, original browser processes recover. |
| Sustained outage | 3/3 | 3/3 | Existing/new pages fail closed while paused; original browser sessions recover automatically without profile reopening. |

All three manifest clones satisfy the original evidence requirement. The focused,
A and B runs recorded 27, 26 and 27 current-fault strict-termination events,
respectively, covering Firefox and Chrome. No process-scan error was found in
those observations. The deterministic regression tests separately prove the
already-gone and genuine-failure branches; these VM logs do not establish how
many already-gone outcomes occurred.

The two sustained pauses lasted **159.057 and 164.996 seconds**, exceeding the
installed **105-second** observation requirement. Both preserved original daemon
and browser identities through the pause and automatic recovery, with fresh
current-session heartbeats, unchanged extension code and no diagnostic profile
reopening. The strict default and bounded reconnect window remain 60 seconds.
The crash runs independently observed daemon PIDs `1414 → 16847` and
`1401 → 16462`; the RPC pathname was a socket during and after both recoveries.

The [qualification audit](../results/p4-kill-batch-20261009/p43-qualification-audit.json)
passed **24/24**, checking comparable complete batches, identical captured harness
sources, matching published extension identities, exact candidate provenance,
required observations and guarded clone/storage removal. With the focused run,
this selection passed **27/27** across nine independent clones. The 32 fault and
recovered screenshots were [visually reviewed](../results/p4-kill-batch-20261009/screenshot-review.json).
See the [final validation record](../results/p4-kill-batch-20261009/validation.json).

## Scope and safety

The [final safety audit](../results/p4-kill-batch-20261009/final-safety.json) passed.
All nine new clones and their copied storage were removed. All three base VMs
remain stopped and their definitions/storage metadata match preflight. The scoped
Phase 4 inventory checked 70 ownership records: 65 removed, five older diagnostic
clones retained and stopped. No guests remain running; 37.492 GiB is free.
Older unrelated Layer 3 ownership records are outside this inventory.

This selection qualifies the four documented native Firefox/Chrome scenarios on
Ubuntu. P4.1's prior 24/24 qualification remains tied to `15a3248`; P4.1, P4.2,
other-distribution runtime, confined-browser runtime, policy/allowance loss and
upgrades were not rerun. This is selected P4.3 acceptance, not full Layer 4 acceptance.
