# Phase 4: Destructive recovery and selected soak acceptance

Status: **smallest Ubuntu P4.0 foundation and P4.1/P4.2/P4.3 runners implemented;
Ubuntu P4.1 qualified on `15a3248` (24/24), P4.2 on the original candidate,
and P4.3 on `9df72e0` (24/24 across two complete fresh batches).
The latest focused manifest run also passed 3/3.**
**Native Fedora and CachyOS P4.1/P4.2/P4.3 are now qualified on `dd6ad80`,
with two complete fresh passes per milestone and distribution (140/140), plus
42/42 prerequisite smoke observations.** See the
[native uninstall fix and validation report](reports/phase-4-native-uninstall-fixes.md).
Existing focused Tier 1 edit-key evidence remains
separate. Later milestones are planned; implementation is not a passing VM result.

The confirmed cleanup and restart failures now have source fixes at commit
`a3162aadb6d6c27cad78835d6bcbbe2274db05c6`; see the
[fix/build report](reports/phase-4-fixes.md). The new exact Ubuntu DEB completed
fresh P4.1/P4.3 validation with 11/12 observations per batch; see the
[fresh validation report](reports/phase-4-fresh-validation.md).

The subsequently approved Firefox preservation and browser recovery changes are
built at `15a3248c37e956532e7f8046f7ac3be0cb555101`, with the strict heartbeat
default and bounded reconnect window set to 60 seconds. Fresh installed-package
validation verifies both changes; see the [revalidation report](reports/phase-4-revalidation.md)
and [implementation report](reports/phase-4-policy-recovery-fixes.md).
The saved `a3162aa` results remain historical failures for those criteria.

The remaining process-exit/logging race is fixed at
`9df72e0749a01d2019ed3116eb693a1f5e573980`. All CI/build jobs passed; the focused
manifest run passed 3/3 and two complete P4.3 batches passed 24/24, with unchanged
acceptance requirements. See the [kill-batch fix and qualification report](reports/phase-4-kill-batch-fix.md).
P4.1 was not rerun on this candidate; its qualification remains bound to `15a3248`.

The initial Fedora/CachyOS P4.1–P4.3 port used the exact `9df72e0` RPM/Arch
candidates. Its historical results are recorded in the
[native port report](reports/phase-4-native-ports.md). Ubuntu historical results
remain bound to their recorded artifacts. Fedora P4.2 is qualified by two fresh
11/11 passes and a successful pair audit. Fedora P4.3 is also qualified by two
fresh 12/12 passes and its complete pair audit. CachyOS P4.3 also passed two fresh
12/12 batches and its complete pair audit. Native P4.1 has confirmed direct-removal
defects on `9df72e0`. The completed fixes and fresh validation are recorded in
the [native uninstall fix report](reports/phase-4-native-uninstall-fixes.md),
which tracks candidate `dd6ad80`, transaction guards, inventory preservation
evidence and the corrected schedule and crash-injection harnesses.
CachyOS P4.2's two prior fresh batches completed
11/11 and 10/11; a schedule observation checked the clock after evidence collection
and failed despite a valid recorded inactive observation. Its pair remains
unqualified on that earlier harness. Those six selected native batches completed
with 69 passes and
one harness timing failure. All 26 new test clones were removed and bases stayed
unchanged; see the native port report for audits and the remaining correction.

The native GUI-uninstall browser baseline waits up to 45 seconds for automatic
communication recovery and requires the named policy rule and target URL in the
visible block page. A generic backend-unhealthy page fails that baseline.
Chromium diagnostic aliases are accepted only when bound to its confirmed active
current-session heartbeat; fault timestamps and recovery evidence remain required.

The case catalog is [Layer 4](layers/04-destructive-and-soak.md). The prerequisite
is a working installed-package path from [Phase 3](phase-3.md), with relevant
feature coverage for the selected scenario. Phase 3's saved passes establish
their recorded artifacts and selections, not every feature or the current source.

## Assessment and scope

These tests are worthwhile because source tests cannot prove package-manager
behavior, protected-file repair, real Native Messaging reconnects, or recovery
after a desktop session is damaged. The highest-value first selections are
authorized uninstall with preservation of unrelated configuration, browser
outage/recovery, hosts repair, and policy recovery.

Keep the existing testing policy: fast checks on every change, focused integration
when affected, and the small three-guest release smoke gate. Layer 4 is selected
hardening and pre-release evidence; the whole catalog is not a mandatory matrix.
Start new suites on Ubuntu. Port package lifecycle cases to Fedora/CachyOS when
their packaging changes or their release claims need verification. Expand desktop
degradation cases only to the affected session environment.

The **first Layer 4 completion target is the bounded Ubuntu P4.0–P4.3 batch**:
the infrastructure needed by those selections, protected uninstall with
preservation of unrelated configuration, hosts repair, and browser repair/outage
recovery. Implement P4.1 first, then P4.2 and P4.3. Record the exact case variants
and artifacts for each accepted suite; completing this batch does not require
every variant in the catalog or a Fedora/CachyOS port.

P4.4 database recovery and P4.5 desktop degradation are separate follow-up
batches with their own prerequisites and expected behavior. P4.6 clock tests,
P4.7 soaks and P4.8 upgrades/confined-browser revisions are selected follow-ups
when affected or explicitly scheduled. They do not block completion of the first
batch or automatically become release requirements.

Root is used by the harness to inject faults in disposable guests. Passing repair
tests does not promise resistance to unrestricted root/sudo; the product boundary
in [README](../README.md) still applies. User-facing authorization and GUI actions
must run as the ordinary desktop user even when that test account has harness sudo.

## Existing implementation and evidence

[`tier1-edit-acceptance.py`](scripts/tier1-edit-acceptance.py) and
[`protected_edits.py`](fixtures/layer4/protected_edits.py) already implement a
focused selection: no-key rejection and `DES-PROTECT-004–007` for website
blocklists, with Tier 2/3 activated by Detox and protected access set to
`all_time`. The journey observes the real five-minute edit expiry.

Ubuntu passed in `ubuntu-tier1-20261003-b`; selected Fedora/CachyOS journeys also
have accepted evidence in the [portability audit](results/desktop-port-20261003/audit.json).
See [Phase 3's command and boundaries](phase-3.md#ubuntu-tier-1-edit-key-journey).
Application edits, other operator windows, and broader recovery remain separate.

The implemented command, using the chosen guest's actual Layer 2 report, is:

```bash
python3 Testing/scripts/tier1-edit-acceptance.py \
  --guest ubuntu \
  --run-id p4-tier1-new \
  --package-report /path/to/candidate-layer2/ubuntu/result.json
```

There is no general destructive/soak runner yet. Later milestones below remain
requirements for future implementation. The focused Ubuntu uninstall, hosts
and browser runners are available. Historical Tier 1 evidence
must not be counted as a fresh candidate pass.

### Ubuntu P4.1 runner

From the repository root:

```bash
python3 Testing/scripts/uninstall-acceptance.py \
  --run-id p41-ubuntu-new \
  --package-report Testing/results/package-ci/github-37634225607/package-ubuntu-x86_64-15a3248c37e956532e7f8046f7ac3be0cb555101/ubuntu/result.json
```

The default runs `remove`, `purge`, and `authorized` on three independent guarded
Ubuntu clones. `--scenario remove`, `--scenario purge`, or `--scenario authorized`
selects one for development; it does not establish the whole P4.1 selection.
Batch IDs use 1–28 lowercase letters, digits or hyphens; child IDs append the
scenario and must also be fresh. The runner reuses the shared Phase 0 host lock.

The ordinary-user guest adapter seeds and witnesses real hosts/process blocks.
For the authorized selection, it uses fresh Firefox/Chrome store profiles,
tests wrong-phrase rejection, and observes natural schedule/Detox expiry under
`no_active_schedule_or_detox` protected access. It then uses the native GUI
uninstall action and observes real post-handoff extension heartbeats, native
`dpkg --purge`, cleanup, and the original browsers beyond their failure tolerance.
No clock, allowance or database resets are used.

Before installation, fixed guest-root setup adds unrelated hosts/Firefox/Chromium
sentinels. It pauses automatic apt jobs in the clone and waits for any in-flight
transaction to finish without deleting package locks or interrupting its owner.
A clone-only Polkit rule grants active local user `akhi` authorization
for the exact leased `/usr/bin/env ... /usr/bin/dpkg --purge blockuntu` invocation.
It substitutes for entering a VM password, not BlocKuntu's credential or lease
checks; the OS authentication-dialog interaction is not covered. No package
binary, maintainer script or browser extension is replaced.
Cleanup inspection also includes user-owned native manifests/copies created by
the GUI's automatic confined-browser helpers; this does not qualify confined
browser behavior or revisions.

A root systemd collector and pinned SSH transport are independent of BlocKuntu.
The host validates named actions, checkpoints before each fault/removal, redacts
credentials/lease tokens and captures the harness sources/checksums. Failed or
incomplete clones are stopped and retained; successful ones are removed only
after required evidence and base-integrity checks. Results are at
`Testing/results/BATCH_ID-SCENARIO/ubuntu/acceptance/`; the batch coordinator writes
`Testing/results/BATCH_ID/summary.json`. Earlier failed attempts remain evidence.

Qualify the new adapter with two complete comparable batches:

```bash
python3 Testing/scripts/check_uninstall_acceptance.py \
  Testing/results/BATCH_A/summary.json Testing/results/BATCH_B/summary.json \
  --qualify --output Testing/results/p41-qualification.json
```

The audit checks exact observations, candidate identity, archived harness hashes,
browser/extension versions, complete evidence, ownership records and removal of
all accepted clones/storage. Once qualified, one audited candidate batch suffices
unless a requalification trigger applies. Use the candidate's own Layer 2 report
for later candidates. See [the validation report](reports/phase-4-validation.md).

Local harness checks:

```bash
python3 Testing/scripts/test_layer4_uninstall.py
python3 Testing/scripts/test_p32_site.py
python3 Testing/scripts/test_vm_acceptance.py
python3 Testing/scripts/test_phase0_vm.py
```

### Ubuntu P4.2 runner

Run the same exact candidate without rebuilding:

```bash
python3 Testing/scripts/hosts-acceptance.py \
  --run-id p42-ubuntu-new \
  --package-report Testing/results/package-ci/github-36269349435/package-ubuntu-x86_64-605d707b92cc5fa007674169f0e74ce6fc879c33/ubuntu/result.json
```

The default selects five independent guarded Ubuntu clones: `remove` and
`modify` for `DES-HOSTS-001`, `append` for `DES-HOSTS-002`, `delete` for
`DES-HOSTS-003`, and `allowlist` for `DES-HOSTS-005`. The removal/repair clone
also observes `DES-HOSTS-004` in order: inactive, naturally active and naturally
ended Tier 2 schedule, with persistent Tier 1 enforcement. This is 11 required
observations per complete batch, including one baseline per clone.
`--scenario remove|modify|append|delete|allowlist` is for development only.

The P4.0 guards, artifact verifier, owner-only checkpoints, redaction,
independent collector, host lock and failure retention are reused. The root
fault CLI has fixed names and `/etc/hosts` as its sole mutation target. It
witnesses the damaged content or absent file immediately after mutation and
leaves the daemon, path watcher and watchdog running to exercise automatic
repair. A missed fault witness is incomplete evidence, not a repair pass.

A clone-only DNS service maps fixture names to `127.0.0.2`; a deterministic
HTTP service listens on that address. Ordinary-user NSS resolution and direct
HTTP probes distinguish real hosts blocking from a broken fixture. Proxies are
disabled for these local probes. Native resolver checks preserve the guest's NSS
configuration. An exact files-only null-address mapping must accompany either a
normal NSS zero-address result or an explicit `EAI_NODATA`/`EAI_NONAME` denial;
temporary/system resolver errors cannot stand in for policy enforcement. Keep the
raw `getent` diagnostics and a bounded native API witness, plus successful control
lookups and actual HTTP observations. Tests cover IPv4 resolution/access and exact
IPv4/IPv6 managed rows; they do not claim IPv6 network/browser enforcement.
For a newly appended external sentinel, capture normal getent, files-only, and
native API lookups until all match the expected address within a 30-second
observation window. This accommodates native hosts-cache propagation; recovery
must not flush caches or change resolver configuration to manufacture the match.
The independent audit requires the matching raw sample and preserved external
row in its root checkpoint.

Every repair checks exact managed content and required immutable state.
Existing external rows must survive removal, modification and append faults.
For whole-file deletion, missing external rows are recorded separately:
BlocKuntu cannot reconstruct deleted third-party content from its policy.
The allowlist-only case activates a real one-minute Detox, checks policy
decisions and verifies empty managed hosts with direct HTTP still accessible.
Browser allowlist enforcement is outside this hosts-layer case.

Evidence uses `Testing/results/BATCH-SCENARIO/ubuntu/acceptance/`; coordinator
results use `Testing/results/BATCH/summary.json`. Audit complete batches and
qualify two comparable fresh passes with:

```bash
python3 Testing/scripts/check_hosts_acceptance.py \
  Testing/results/BATCH_A/summary.json Testing/results/BATCH_B/summary.json \
  --qualify --output Testing/results/p42-qualification.json
python3 Testing/scripts/test_layer4_hosts.py
```

The audit checks complete observations, independent root fault and hosts
checkpoints, archived source checksums, exact candidate identity, owner records
and removal of accepted clones/storage. P4.2 acceptance does not clear the
known P4.1 cleanup failures. See the [validation report](reports/phase-4-validation.md).

Candidate `0.2.0-1` from run `36269349435` passed two complete fresh batches,
`p42-ubuntu-20261006-a` and `p42-ubuntu-20261006-b`: 22/22 observations with
identical captured harness sources. The [qualification audit](results/p42-qualification.json)
passed; all ten P4.2 clones/storage were removed with base metadata unchanged.
This accepts the documented IPv4 network and exact IPv4/IPv6 hosts-row variants.

### Ubuntu P4.3 runner

```bash
python3 Testing/scripts/browser-acceptance.py \
  --run-id p43-ubuntu-new \
  --package-report Testing/results/package-ci/github-37907529116/package-ubuntu-x86_64-9df72e0749a01d2019ed3116eb693a1f5e573980/ubuntu/result.json
python3 Testing/scripts/check_browser_acceptance.py \
  Testing/results/BATCH_A/summary.json Testing/results/BATCH_B/summary.json \
  --qualify --output Testing/results/p43-qualification.json
python3 Testing/scripts/test_layer4_browser.py
```

Four independent Ubuntu clones cover `policy`, `manifest`, `crash` and `outage`.
Each uses both fresh native Firefox and Chrome store profiles. Policy and
manifest variants deliberately damage the two selected browser files together;
they do not establish every supported browser or every individual-file variant.
The proven P4.1 store-onboarding controls are reused, including their recorded
profile/rule names. Browser operations remain ordinary-user operations.

Policy damage removes selected managed extension entries; automatic repair must
restore both canonical policy documents. This tests the installed strict policy
mode and does not claim preservation of custom keys inside those managed files.
Manifest damage changes
the two native executable paths and terminates the original native processes,
then observes health transitions and the configured strict daemon guard closing
both stale-heartbeat browser sessions. Screenshots, process/session status and
actual kill events must agree. Explicit restoration of the original package
manifests and reopening the same profiles test recovery without extension
reinstallation. This does not claim autonomous manifest repair or GUI repair.

The transient variant pauses the verified daemon for twelve seconds, covering
the installed extension request cycle plus its RPC timeout and scheduling margin.
The adapter rejects installed timings that exceed that bounded pause; the root
witness records its actual elapsed duration. It then sends SIGKILL with normal
watchdog/restart configuration enabled. Current-fault transport diagnostics
from both published extensions must prove the interruption. Native Messaging processes
can survive that restart because each request opens a daemon socket. Record
request-path recovery and fresh heartbeats separately from native-process
replacement; a brief restart does not establish sustained fail-closed coverage.

The sustained variant sends SIGSTOP to that verified daemon and leaves the
watchdog enabled. A root-owned lease and independent recovery service apply
SIGCONT automatically after at most 240 seconds. The runner also restores faults
before final collection/shutdown. No service masks or drop-ins are installed.
The installed profile extensions' manifests, background code, SHA-256 values
and timing constants are archived before injection. The observation window
exceeds their heartbeat and existing-tab failure tolerances; unknown or excessive
timing aborts baseline. Both original allowed tabs and new HTTP navigations must
show actual unavailable/fail-closed pages while the fault is still active.

`--scenario` selects one development variant. Complete acceptance needs all four
and two comparable fresh passing batches for initial adapter qualification,
with independent fault/checkpoint evidence, original browser identities,
current-session heartbeats, allowed/named-blocked recovery pages, base integrity
and guarded clone/storage removal. Failed clones remain stopped for diagnosis.
The sustained variant requires original-session recovery. If the strict guard
closes the browsers on resume, the adapter also reopens the same profiles to
record manual recovery, while preserving the automatic-recovery failure. The
approved source change now gives previously connected browser sessions a bounded
60-second reconnect window after daemon restart or a sustained scan gap. Strict
closure resumes if no fresh heartbeat arrives; a repeated gap cannot extend an
outstanding window. The suite must still prove original-session recovery on the
new exact package, with no manual profile reopening counted as an automatic pass.
P4.3 is qualified on the latest `9df72e0` candidate with two complete fresh
passing batches (24/24), including current-fault strict-termination events and
automatic original-session outage recovery. See the
[kill-batch fix and qualification report](reports/phase-4-kill-batch-fix.md) and
[qualification audit](results/p4-kill-batch-20261009/p43-qualification-audit.json).
The [previous revalidation](reports/phase-4-revalidation.md) remains historical
11/12 evidence for `15a3248`; later milestones and full Layer 4 acceptance are separate.

## Inputs and decisions needed from the operator

| Input or decision | Why it is needed | Proposed starting choice |
| --- | --- | --- |
| Candidate Layer 2 package report and sibling `artifacts/` directory | Pin the exact source commit, installed package and SHA-256. Source changes alone do not change a historical package. | Latest P4.3 qualification uses [workflow run 37907529116](https://github.com/cstute1206/BlocKuntu/actions/runs/37907529116), commit `9df72e0749a01d2019ed3116eb693a1f5e573980`, Ubuntu DEB `0.2.0-1`; exact SHA/provenance verified in the kill-batch report. P4.1 qualification remains bound to workflow `37634225607` and commit `15a3248c37e956532e7f8046f7ac3be0cb555101`. Earlier P4.1/P4.3 evidence is bound to `37609821265`; historical P4.2 qualification remains bound to `36269349435`. |
| Initial scope and execution window | Bound guest usage and long observations. | Current request: implement and build process-kill batch handling, then validate the focused Ubuntu manifest scenario and two complete P4.3 qualification batches. Completed on independent serialized clones; upgrades remain deferred. |
| Policy-loss contract | Decide what an existing installation must do when both policy stores disappear. | Treat silent fallback to a permissive baseline as a failure; require a visible recovery-required state. Enforcement guarantees need to be specified per channel. |
| Usage-state-loss contract | Recovery TOML restores configuration, not necessarily consumed allowance, unlock quota, active Detox or clock-integrity state. | Report each lost runtime field and test for renewed access. Decide whether that access is acceptable after privileged fault injection before claiming recovery acceptance. |
| Clock-integrity recovery policy | Current detection is sticky; restoring the wall clock alone does not clear the stored tampered state. | Keep fail-closed behavior through restart/reboot; document an explicit recovery procedure before claiming return to normal operation. |
| Previous accepted package/report, if upgrades are selected | Prove a real old-to-new transaction and migration. | Upgrade testing is explicitly deferred by the operator; no old package is needed for P4.1. |
| Optional network soak scope | YouTube requires reliable internet and has external failure causes. | Deterministic local-page soak first; YouTube as a separate optional run. |

Reuse the configured `Ubuntu`, `Fedora`, and `CachyOS` immutable bases and existing
SSH/libvirt setup after live preflight. New base images, personal browser profiles,
and personal recovery keys are not needed. Read credentials generated inside the
clone into fixed input slots; never put them in requests, logs, screenshots or Git.
No VPN/firewall changes are part of this plan.

These decisions do not block writing or implementing independent fixtures.
Resolve a case's expected behavior before executing it for acceptance; do not
choose a convenient expected result after seeing the outcome.

### Suggested state-loss policy for later P4.4

This is a recommendation, not an accepted or implemented recovery contract:

- Distinguish a genuinely new installation from an existing one using durable
  installation identity. On an existing installation, restore the last valid
  policy snapshot; do not silently substitute the packaged baseline if both
  stores are missing. Enter a visible recovery-required mode with the GUI and
  an explicitly supported repair/uninstall path available. Measure browser and
  application enforcement separately; a stopped daemon is not application
  fail-close, and unknown policy cannot be reconstructed from nothing.
- If policy survives but consumed allowance or unlock accounting does not,
  conservatively exhaust Tier 3 allowance for the current trusted local day,
  revoke temporary grants and treat the current hour's manual-unlock quota as
  used. Resume normal accounting at the next legitimate reset, or through an
  authorized recovery action. Loss of accounting must not silently grant fresh
  access. Confirm this product choice before implementing it.
- Persist active Detox deadlines and the minimum accounting/integrity recovery
  state separately from disposable history/statistics, with atomic durable
  updates. A recovered rule definition alone does not restore an active Detox.
  If this state is also unavailable, report the gap and require recovery rather
  than claiming all previous restrictions survived.

Personal activity logs and historical statistics need not be restored. This
bounded safety state is distinct from those records. These decisions and any
product changes remain follow-up work; P4.1 does not inject database/clock faults.

## Safety and orchestration contract — P4.0

Reuse the Phase 0/3 artifact verifier, ownership records, pinned SSH host keys,
full-copy cloning and host lock. Build P4.0 incrementally: start with the guards,
atomic checkpoints, redaction, failure retention and named actions needed by
P4.1. Add only the fault actions and transport capabilities required by each
subsequent selection. A general fault framework and desktop-destruction
capability are not prerequisites for the initial uninstall suite.

Every selected suite must satisfy this bounded fault protocol:

1. Reject invalid package provenance, reused run IDs, base-domain targets, shared
   disks/NVRAM, backing chains and changed ownership before any mutation. Record
   base definitions and storage metadata before and after; existing metadata
   checks are not full-content disk hashes.
2. Use a fresh full-copy clone for each independent fault scenario. A linked
   sequence such as uninstall/cleanup or outage/reconnect may share a clone;
   record its prerequisite order. Never carry damage into an unrelated case.
   Libvirt snapshots are optional; the current templates use independent copies.
3. Establish the installed-policy and functional baseline before injecting a
   fault: actual blocked/allowed navigation and fixture-process outcomes as
   appropriate, not only an RPC health result.
4. Check host-side console access and a control transport that survives the
   selected fault. For application allowlists, ordinary-user SSH commands can
   themselves be terminated. Validate a guest-agent, root system service or
   host-console collection path before activating a destructive allowlist; a
   live ordinary-user SSH session alone is insufficient.
5. Permit only named guest fault actions with fixed target paths, validated
   clone identity and bounded scope. Guest requests must never become arbitrary
   host shell commands. Install evidence collection before breaking the desktop.
6. Save and transfer baseline evidence before mutation, checkpoint during the
   fault, and collect after recovery. An observation lost with the session is
   incomplete evidence, not a pass.
7. Use host monotonic deadlines, especially during guest clock changes. Stop on
   a failed prerequisite; mark dependent observations blocked with the cause.
8. Preserve failed/incomplete clones stopped for diagnosis and record their
   ownership. Remove successful clones/storage with guarded cleanup and retain
   evidence. This is the new-suite retention contract; the existing focused
   Tier 1 runner currently attempts cleanup on both success and failure.

For P4.1–P4.3, validate console access and that the chosen transport/collector
survives the selected package, hosts or browser/daemon fault. Collection must not
depend on the BlocKuntu component being removed or stopped. These suites do not
require deliberately destroying the desktop during preflight.

Before P4.5 or any selection that can terminate the desktop or user collector,
extend preflight to demonstrate evidence retrieval after both are stopped,
through a guest-agent, root system service or host-console path. Do not activate
an incomplete application allowlist until that independent path is verified.
Add regression checks for target guards, path/action validation, secret
redaction, lost checkpoints and cleanup failures before enabling the corresponding
destructive actions; extend those checks as new actions are added.

## Implementation order and selected cases

The P4.0 subset needed for P4.1–P4.3 and those runners is implemented. Ubuntu
P4.1 is qualified on `15a3248`, P4.2 on the original candidate, and P4.3 on
`9df72e0`, including automatic sustained-outage recovery. Fedora/CachyOS ports
are implemented and undergoing fresh validation on `9df72e0`; their qualification
is pending. Later selections remain planned. Existing case IDs stay in the
catalog; a suite records exact variants rather than claiming all variants of an
ID from one representative pass.

| Milestone | Cases | Required observations and exit criterion |
| --- | --- | --- |
| P4.0 — incremental guarded fault infrastructure | Cross-cutting | Reuse artifact/clone guards; add atomic checkpoints, redaction, failure retention and fault-surviving collection for the selected suite. Qualify desktop-independent collection before P4.5 or another desktop-disrupting selection. |
| P4.1 — protected access and uninstall | Ubuntu: `DES-PROTECT-001–003`, `DES-UNINSTALL-001`, `DES-UNINSTALL-004–007` | Attempt direct remove and purge separately on fresh clones; package remains installed and functional after rejection. Check wrong credential/no lease; schedule and Detox close protected access; valid GUI authorization invokes the native package manager. Verify cleanup and open-browser recovery while preserving unrelated configuration. `DES-UNINSTALL-002/003` identify the Fedora/CachyOS native refusal selections; their adapters are in validation. |
| Existing edit-key journey | `DES-PROTECT-004–007` plus no-key observation | Reuse the accepted runner when editing/authentication changes. Preserve its website-blocklist/Detox/`all_time` scope. Application and operator-window variants need separate observations. |
| P4.2 — hosts repair | `DES-HOSTS-001–005` | Save baseline hosts/attributes; alter only managed rows, add an unrelated sentinel, and delete the file in separate fault scenarios. Check exact managed repair, required immutable state, preserved unrelated rows, actual resolution, schedule transitions and Tier 1 persistence. Allowlists create no catch-all hosts block. |
| P4.3 — browser repair, transient restart and sustained outage | `DES-BROWSER-001–005` | After a real store-extension heartbeat, select policy/manifest repair, automatic daemon restart, and sustained communication outage separately. Record each effective fault trigger, duration, installed thresholds and recovery trigger. Require fail-close after the relevant thresholds in the sustained outage; a quick automatic repair need not enter that state. Verify allowed/blocked navigation and current-session heartbeats after recovery without extension reinstallation. Run Firefox plus native Chrome (Ubuntu/Fedora) or Chromium (CachyOS) in regular profiles; record versions independently. |
| P4.4 — database and recovery snapshot | `DES-DATA-001–006`, `DES-AAL-007` | Export a mixed policy, establish current recovery TOML, then delete/corrupt SQLite or remove both stores in independent clones. Compare recovered policy semantics and actual website/process enforcement. Test allowlist recovery exemptions. Report runtime-state loss and unrecovered states explicitly; a fatal daemon state alone is not proof of application fail-close. |
| P4.5 — incomplete application allowlists | `DES-AAL-001–006` | Omit one eligible helper category per scenario, record PID/identity and termination, then compare with a separately curated complete allowlist. Capture desktop degradation and recovery exemptions through the independent control path. New helpers stay rejected; Tier 3 exhaustion/unlock/expiry uses real elapsed time and never resets usage or quota. |
| P4.6 — clock tampering | `DES-CLOCK-001–004` | Separate clones for backward/forward movement, restore, and reboot. Record integrity baseline, guest timezone, synchronization settings, wall/monotonic samples and protected-operation rejection. Check real browser/process enforcement across schedule, Detox and allowance boundaries. Do not change the host clock. |
| P4.7 — selected real-time soaks | `SOAK-001–007` | Start with local one-hour heartbeat and GUI-closed runs. Then select restart, schedule and Tier 3 accounting cases when affected. Verify intermediate samples/events and final state. YouTube and real local-midnight coverage are separately scheduled selections. |
| P4.8 — optional upgrade and confined-browser revisions | `DES-UPGRADE-001–005`, `DES-BROWSER-006` | Require an accepted old/new artifact pair or the exact supported browser package/revisions. Record native transaction, preserved identity/credentials/policy, runtime-state migration and recovered heartbeats. Do not infer this from reinstalling a candidate or updating OS prerequisites. |

Start with the P4.0 foundation required by P4.1 on Ubuntu, then add P4.2/P4.3
repair and reconnect selections to complete the first batch. P4.4 adds controlled
writer shutdown and data-recovery fixtures; P4.5 additionally requires validated
desktop-independent evidence transport. Their completion and the optional
clock, upgrade and long-duration selections are separate from the first batch.
Package-manager behavior must be observed per distribution:
a maintainer-hook error or command exit code alone does not prove removal was
prevented. Check package database, binaries, services and enforcement afterward.

## Pass criteria that need particular care

### Uninstall preserves other configuration

Seed unrelated hosts entries and browser-policy settings before BlocKuntu setup.
For Firefox, include a non-BlocKuntu setting in the same `policies.json`, not
only a separate file. For Chromium, retain an unrelated managed-policy file.
Verify each sentinel again in the immediate pre-uninstall checkpoint: a
pre-install seed alone cannot establish that uninstall deleted it. The saved
P4.1 Firefox sentinel was already absent before purge; its earlier uninstall
attribution is corrected in the validation report. The latest `15a3248` candidate
removes the previously leaked user-native files and preserves the Firefox
sentinel through activation and authorized uninstall; see the revalidation report.
Compare these after authorized removal, together with system/user Native
Messaging manifests, units, credentials, state directories and hosts attributes.
Record any intentional package-manager residual configuration separately.

The approved source fix merges native Firefox policies and records previous
managed values before changing them. GUI preparation restores only managed
entries; DEB/RPM/Arch removal hooks preserve the restored shared document.
The installed-artifact test must still prove preservation through activation,
repair and authorized uninstall on a fresh candidate.
Both candidates remove the managed Firefox file after native GUI purge, but
their sentinel was already absent immediately before purge. This does not
establish loss of unrelated settings during uninstall; see the
[fresh P4.1 validation](reports/phase-4-fresh-validation.md).

Keep browsers open through the genuine uninstall handoff. Verify ordinary
management pages and HTTP(S) pages recover when the native host is gone; simply
closing the browser avoids the failure being tested. Distinguish intentional
uninstall from an unexpected native-host outage.

### Database loss is not complete runtime restoration

Quiesce relevant writers before the controlled offline deletion/corruption
scenario and account for SQLite WAL/SHM sidecars; do not mistake an open database
handle or leftover WAL for recovery. Record the exact removed/corrupted files
and how services/watchdog/socket activation were controlled in the clone.
Do not modify the exported policy to manufacture a passing recovery.

Compare policy IDs, list modes, tiers, matchers, schedule attachments, allowance
configuration and protected settings semantically. Inspect active Detox, consumed
usage, manual-unlock quota, grants and clock-integrity metadata separately; their
restoration is not implied by configuration round-trip. Configured allowance
survival must not be confused with consumed-allowance survival.

Current `focusd/src/app.rs::load_startup_policy` distinguishes a preexisting empty
database from an absent database. With no recovery file, the former errors, while
the latter loads the packaged configuration. Consequently deleting both stores
on an existing installation may violate `DES-DATA-004`'s recovery-required
expectation. Keep that case explicit and resolve any product gap; do not label
baseline fallback as policy recovery. Corrupt SQLite may also prevent startup
before TOML recovery is reached; the catalog permits an explicit fatal state
for that case, but surviving enforcement must still be measured separately.

For `DES-DATA-005`, first test ordinary-user denial and immutable protection.
Then, if selected, inject a privileged alteration against a running daemon with
intact SQLite and specify the verification trigger and expected repair/error.
The recovery manager uses file permissions/immutability and TOML parsing; those
alone do not prove detection of an arbitrary valid TOML replacement by root.
Do not invent a signed-snapshot or periodic-verification contract.

### Clock and browser timing follow the tested build

Current `focusd/src/clock_guard.rs` detects same-boot drift greater than 300
seconds, checks backward movement across reboot, and persists a sticky tampered
state. Use a jump comfortably beyond the threshold after establishing the
baseline. Reverify the candidate's implementation before choosing thresholds.
Do not expect every small correction, forward reboot jump, or restored wall clock
to imply detection or reset. Protected-window tests use legitimate configuration
and natural time; changing the clock to reach an uninstall window contaminates
the authorization test.

Record the installed extension's timeout, startup grace and consecutive-failure
tolerance. Firefox and Chrome need not share thresholds. Assert named failure
state/pages and subsequent allowed/blocked behavior; a redirect URL or a generic
browser error is insufficient. Source constants alone do not establish the
behavior of a different published extension. Published Chrome/Chromium 0.2.6
private-window acceptance has passed on all three native-browser guests;
Chrome/Chromium private protection still requires the
[documented browser-owned consent](../Docs/FEATURES.md#boundaries-and-limitations).

### Browser repair and effective outage are separate observations

Specify the fault target, effective trigger, intended duration, expected state
and recovery trigger before each P4.3 variant. Capture when communication
actually fails and returns, rather than measuring only time since a file edit
or process signal. Keep these selections distinct:

- **Policy/manifest repair:** damage one managed file and observe its supported
  repair path. Removing a Native Messaging manifest does not by itself prove
  that an existing native connection has failed. For a reconnect variant,
  specify how the existing connection is ended and observe a new connection
  attempt while the manifest is broken; otherwise claim file repair only.
- **Transient daemon crash/restart:** leave the normal service/watchdog recovery
  path enabled. Record automatic restart, the observed interruption and restored
  heartbeats/navigation. Recovery within the installed failure tolerance can
  pass without a fail-closed page; it does not establish sustained-outage coverage.
- **Sustained communication outage:** use a bounded named action to keep the
  selected daemon/native-host path unavailable beyond the installed relevant
  timeout and consecutive-failure tolerance. If automatic repair/restart must
  be inhibited, record that intervention and restore it. Verify failed
  communication, new-navigation fail-close and existing-tab behavior at their
  respective thresholds before releasing the fault.

After restoring the supported path, require successful communication and a
heartbeat belonging to the current browser session, plus actual allowed access
and a named block page for a still-restricted URL. Record whether recovery was
automatic or required a declared operator action. A quick repair cannot count
as a sustained outage, and a surviving connection cannot count as a reconnect.

### Soaks require continuous evidence

For each one-hour case, capture at least 3,600 seconds of host monotonic elapsed
time with periodic heartbeat/session-health samples and complete relevant logs.
Bound sampling gaps and fail acceptance if observation disappears. A healthy
final screenshot cannot erase an intervening false heartbeat failure or browser
kill. During planned browser-close intervals, assert the appropriate stopped
state and a fresh healthy session after restart.

Use the deterministic local fixture as the reproducible baseline. Record network
and YouTube failures independently; an interrupted external run cannot count as
a full passing hour. Midnight accounting uses a natural local-midnight run;
clock injection belongs to P4.6 and cannot substitute for that soak.

## Evidence and completion

The proposed general runner retains bundles under
`Testing/results/RUN_ID/GUEST/acceptance/`, following Phase 3 conventions. Its
`result.json` must record:

- Selected case IDs and variants, dependencies, expected fault/recovery behavior,
  and pass/fail/blocked/skipped status with reasons. Track implementation status
  (`planned`, `automated`, `manual`, `blocked`) separately from execution results.
- Candidate commit, exact package filename/version/SHA-256 and Layer 2 report;
  old/new provenance for upgrades; harness source hashes and captured sources.
- Clone/base identity, guest OS/kernel/session, ordinary-user UID and injector
  identity; browser package/store provenance, versions and profile/session IDs.
- Before/fault/after checkpoints, host monotonic durations and guest timestamps,
  policy exports/diffs, file attributes, package transaction output, process
  outcomes, daemon/watchdog journals, browser logs and redacted screenshots.
- Evidence-collection failures, base-integrity comparison, retained-clone or
  clone/storage-removal status, and final audit result.

A selected suite passes only when every required observation and its evidence
are present, dependent prerequisites passed, and base integrity/cleanup or
declared retention are audited. Missing evidence, skips or blocked cases cannot
be converted into acceptance. A safety/audit failure prevents an overall pass.
Before declaring a new suite/guest adapter repeatable, obtain two comparable
passes on fresh clones for the chosen artifact and exact selection. Repeat that
qualification after substantial harness changes affecting its execution or
evidence collection, or when investigating suspected flakiness; preserve earlier
failed attempts. This qualifies the adapter and does not impose two passes on
every future candidate or unchanged selection. Once qualified, one complete
fresh-clone pass is sufficient for a selected candidate check unless one of
those requalification triggers applies.

Completion is per milestone and tested artifact. Record the implemented command,
case variants, accepted run IDs, limitations and audit in a Phase 4 validation
report as each milestone lands. Do not report full Layer 4 acceptance from a
focused selection, and do not promote this plan into a mandatory all-case release
gate without a separate scope decision.
