# Phase 4 validation: Ubuntu uninstall, hosts and browser recovery

Historical candidate status: **implementation complete; selected candidate fails cleanup; adapter
qualification not established for P4.1; Ubuntu P4.2 qualified and accepted;
P4.3 executed and fails daemon-restart recovery; full P4.3 adapter
qualification is withheld**.
This report covers the minimal P4.0 infrastructure and Ubuntu P4.1–P4.3 selections.
P4.4–P4.8 are not established; upgrade testing remains explicitly deferred.

Follow-up source fixes address the confirmed user-native cleanup and RPC socket
restart failures at commit `a3162aadb6d6c27cad78835d6bcbbe2274db05c6`.
See the [fix/build report](phase-4-fixes.md). The results below remain evidence
for the original candidate. The [fresh new-build validation](phase-4-fresh-validation.md)
verifies both fixes; full milestones remain withheld for Firefox preservation
coverage and sustained-outage automatic recovery.

## Candidate and implementation

- Operator-selected [workflow run 36269349435](https://github.com/cstute1206/BlocKuntu/actions/runs/36269349435): completed successfully at commit `605d707b92cc5fa007674169f0e74ce6fc879c33`.
- Ubuntu candidate: `blockuntu_0.2.0-1_amd64.deb`, SHA-256 `fc1d5cc00d07e3cd5e540a2626af91a60a62042eab4070ae03d85f5049e4e48a`.
- Cached provenance-verified Layer 2 report/package rechecked against the live workflow identity; [verification evidence](../results/p41-candidate-36269349435/verified-inputs.json).
- [Runner, command and boundaries](../phase-4.md#ubuntu-p41-runner).
- New regression checks cover action/clone guards, secret redaction, atomic checkpoints, terminal-publication races, failure retention, cleanup/base errors and audit scope/path boundaries.

## Development attempts

| Batch | Observations | Result |
| --- | --- | --- |
| `p41-ubuntu-20261005-a` | Direct `apt remove` and `apt purge`, on separate clones | Both passed 2/2 required observations including real enforcement baselines; both clones/storage removed; base metadata unchanged. |
| `p41-ubuntu-20261005-a-authorized` | GUI/browser baseline | Failed before uninstall: the adapter omitted the local HTTP fixture hostname from guest hosts/DNS. All dependent uninstall cases blocked; failure evidence collected, clone stopped and retained, bases unchanged. |
| `p41-ubuntu-20261005-b` | Direct remove/purge plus GUI/browser baseline | Direct variants both passed 2/2 again and clones were removed. GUI baseline aborted because the action whitelist omitted the shared adapter's Ctrl+2 tab selection. No GUI uninstall performed. |
| `p41-ubuntu-20261005-c-authorized` | GUI/browser baseline | Failed before uninstall because fresh Chrome store consent refused accessibility focus. A bounded native Tab fallback now submits only the observed focused consent button. |
| `p41-ubuntu-20261005-d-authorized` | GUI/browser baseline | Both real browser baselines succeeded; failed when returning to the third application's GUI. Two independent Alt+Tab chords only toggled the browsers. A fixed native switcher action now requires observed GUI activation before input. |
| `p41-ubuntu-20261006-a-authorized` | Functional GUI/browser baseline; wrong phrase | Baseline passed with the focus fix. The wrong phrase was visibly rejected, but the assertion expected backend wording instead of the GUI's translated message. Corrected the mismatch and the analogous protected-access assertion; later observations were blocked. |
| `p41-ubuntu-20261006-b-authorized` | Browser onboarding | Chrome's consent focus returned a timeout rather than the previously handled assertion. Extended the observed-focus native Tab fallback to this timeout only when the visible consent button exists. No uninstall performed. |
| `p41-ubuntu-20261006-c-authorized` | Baseline, wrong phrase and scheduled protection | Baseline and wrong-phrase checks passed. Schedule rejection appeared correctly, but this historical candidate displays the backend message while current source translates it. The assertion now checks the shared restriction phrase. No authorized removal performed. |
| `p41-ubuntu-20261006-d` | Full batch through GUI handoff | Direct variants passed and clones were removed. GUI baseline, wrong phrase, schedule, Detox and natural expiry passed. Native removal waited at OS authentication because Polkit resolves Ubuntu's env symlink to `/usr/lib/cargo/bin/coreutils/env`; the fixture only matched `/usr/bin/env`. Resolved-program support is now included, with the leased purge argv restriction retained. Cleanup/browser cases were blocked. |
| `p41-ubuntu-20261006-e` | Direct variants and browser onboarding | Direct variants passed and clones were removed. Chrome's store appeared while the shared helper was still waiting on a stale consent node. P4.1 now uses a local bounded native Tab flow that requeries consent nodes and accepts an observed store transition. GUI uninstall was not reached. |
| `p41-ubuntu-20261006-f-authorized` | GUI handoff after all protection checks | Baseline, wrong phrase, schedule, Detox and natural expiry passed. Polkit authorization succeeded and native dpkg was invoked, but a concurrent automatic OS upgrade held dpkg's frontend lock. The GUI displayed the busy-package error; the host timeout initially obscured this cause. The fixture now pauses automatic apt jobs and waits for in-flight transactions. No lock deletion or transaction interruption is used. |
| `p41-ubuntu-20261006-g-authorized` | Native purge and post-removal state | Native GUI purge passed, including real extension notice/collector survival. Cleanup left three user-native manifests and three Snap host/launcher copies; the Firefox policy file was absent afterward (the sentinel was already absent before purge; see the attribution correction below). Firefox's Add-ons Manager opened its discovery view, making an Extensions-view assertion premature. Recovery navigation now selects that view explicitly; cleanup and browser observations collect all failures independently. |
| `p41-ubuntu-20261006-h` | Direct variants and native GUI purge | Direct variants passed and clones were removed. GUI package removal completed, but log rotation moved its records to `dpkg.log.1`, making the current-file-only handoff assertion fail. The archived log proves this run's purge. Evidence now reads current/rotated logs and requires current timestamps and candidate version; physical removal also enables independent postcondition checks despite another handoff failure. |
| `p41-ubuntu-20261006-i-authorized` | Purge, cleanup and independent browser recovery | Six observations passed through native purge. Cleanup reproduced the leftover user-native files; the Firefox sentinel comparison used a pre-install seed and its uninstall attribution is corrected below. Original browser processes survived and management pages opened. HTTP recovery returned the fixture's 404 because its route allowlist lacked the browser-specific exact paths. Added those two paths and an HTTP regression check; this run does not establish HTTP recovery failure in the product. |
| `p41-ubuntu-20261006-j-authorized` | Corrected HTTP fixture; GUI input | Both browser baselines passed. The invalid phrase was entered, but native Enter activation did not produce the expected rejection; later cases were blocked. The shared button helper only checked a focus acknowledgement. P4.1 now requires observed `FOCUSED` state before uninstall activation. This attempt is not a product uninstall or recovery result. |
| `p41-ubuntu-20261006-k-authorized` | Final corrected authorized journey | 7/8 required observations passed: functional baseline, wrong phrase, schedule/Detox protection, natural expiry, native GUI purge and both original browsers' management/HTTP recovery. Only cleanup failed, reproducing six leftover user-native files; its earlier Firefox sentinel attribution is corrected below. Complete evidence collected; clone stopped and retained; all bases verified unchanged. |

The fixture omission was corrected before fresh runs. Browser URLs were also
separated by family to prevent matching rules from shadowing each other. The
first failed attempt is a harness failure, not product uninstall evidence and
not an accepted qualification batch. Its [bundle](../results/p41-ubuntu-20261005-a-authorized/ubuntu/acceptance/result.json)
and [batch summary](../results/p41-ubuntu-20261005-a/summary.json) remain preserved.
The first ten failed GUI development clones above were subsequently diagnosed and removed
through guarded cleanup; their failure results remain unchanged as historical
evidence. [Cleanup verification](../results/p41-diagnosed-development-cleanup.json)
confirms copied storage/domain removal and unchanged base metadata.

Local validation: 25 uninstall-foundation regression checks, 23 existing VM
acceptance checks, 14 Phase 0 checks and one HTTP-fixture check passed. These validate the harness;
they do not establish installed-package uninstall acceptance.

## Confirmed candidate cleanup defect and attribution correction

`DES-UNINSTALL-006` failed after successful native GUI purge in independent
fresh clones G, I and K:

- Three user Native Messaging manifests remained, under the Brave profile,
  Snap Chromium profile and Snap Firefox profile. Three native host/launcher
files remained under the two Snap profiles.

Correction discovered during P4.3: the saved immediate `before-uninstall`
inspections in G, I and K already lack the Firefox `DisableTelemetry` sentinel.
The prior cleanup assertion compared against the pre-install seed, so its
earlier attribution of that sentinel's loss to uninstall is not established.
The native package hook does delete the managed Firefox policy file, but these
runtime bundles do not prove removal of a third-party setting present at purge.
The six leftover user-native files remain a confirmed cleanup failure. Historical
verdicts are preserved; unrelated Firefox-setting preservation needs a baseline
that demonstrably survives setup until immediately before removal. See the
[attribution audit](../results/p43-p41-attribution-correction.json).

The original hosts entries, unrelated hosts sentinel and unrelated Chrome
managed-policy sentinel survived. No candidate binary or maintainer script was
modified. See [final cleanup findings](../results/p41-ubuntu-20261006-k-authorized/ubuntu/acceptance/p41-cleanup-findings.json)
and [exact-DEB maintainer-script evidence](../results/p41-candidate-36269349435/maintainer-script-evidence.json).

## P4.1 final browser and safety observations

In final run K, Firefox and Chrome kept their original process IDs and opened
their respective management pages and previously blocked deterministic HTTP
routes after the removal wait exceeded the installed tolerance plus 30 seconds.
No browser restart or replacement profile was used. The corrected HTTP fixture
returned actual route pages, resolving run I's 404 mismatch. See
[browser observations](../results/p41-ubuntu-20261006-k-authorized/ubuntu/acceptance/p41-browser-recovery.json)
and the [complete final bundle](../results/p41-ubuntu-20261006-k-authorized/ubuntu/acceptance/result.json).

At the end of P4.1, all base VMs and retained failed clones were stopped. Successful direct-variant
clones and the ten diagnosed early development clones have no remaining domain
or copied storage. Later failed clones G–K remain stopped with their evidence.
[Final safety inventory](../results/p41-final-safety.json) verifies ownership,
storage isolation and unchanged base definitions/storage metadata for that batch.

## P4.1 scope and acceptance gate

The selected Ubuntu cases are `DES-UNINSTALL-001` with independent remove/purge
variants, `DES-UNINSTALL-004–007`, and `DES-PROTECT-001–003`, plus one functional
baseline per clone. No Fedora/CachyOS uninstall, emergency-code, Sunday-window,
lease replay/expiry, database-loss, clock-tamper or upgrade acceptance is claimed.
The clone-only Polkit fixture exercises actual GUI/credential/handoff/native
package removal; it does not cover entering an OS authentication-dialog password.

Full acceptance requires the final selected observations, complete evidence,
guarded cleanup/base-integrity checks and two comparable fresh batches for
initial adapter qualification. Until that audit passes, do not report Ubuntu
P4.1 as accepted. The [audit](../results/p41-final-audit.json) correctly rejects
the failed complete batch. A corrected package candidate is needed to resolve
the confirmed cleanup failures; source edits would not change this exact DEB.

## Ubuntu P4.2 validation

Status: **runner implemented; selected Ubuntu variants qualified and accepted**. The operator
selected P4.2 on the same exact DEB to collect findings before one combined
product-fix build. No product fix or package rebuild is part of this run.

The selected variants are managed-row removal and modification on independent
clones (`DES-HOSTS-001`), unrelated append (`002`), whole-file deletion (`003`),
natural schedule transitions after removal/repair (`004`), and an active
allowlist-only policy (`005`). Each clone establishes real NSS/HTTP baseline
behavior and required immutable state before fault injection.

Local validation: 19 new hosts regression checks, 25 shared P4.0
uninstall-foundation checks, 23 VM acceptance checks and 14 Phase 0 checks
passed (81 total). The audit additionally checks independent
root fault witnesses and saved hosts states, rather than accepting guest
booleans alone. Missing, altered, partial or contradictory evidence fails.

| Batch | Observations | Result |
| --- | --- | --- |
| `p42-ubuntu-20261006-a` | First complete five-scenario batch | Passed 11/11 observations and the independent audit. All five clones/storage removed, with unchanged base definitions/storage metadata. No new product failure identified. |
| `p42-ubuntu-20261006-b` | Comparable repeat for initial adapter qualification | Passed 11/11 observations; same candidate and captured harness as A. All five clones/storage removed, with bases unchanged. |

The [qualification audit](../results/p42-qualification.json) passed 22/22
observations across ten distinct fresh clones and confirmed matching candidate
identity/harness hashes, complete evidence and guarded removal. This accepts
the selected P4.2 variants on the exact DEB; P4.1 remains failed at cleanup.
No additional P4.2 product failure was found. No product fix or rebuild was made.

Root inspections after repair showed the daemon and hosts path watcher active,
and no failed units or error lines in the collected targeted journals. Managed
repair was observed roughly one second after the witnessed fault in the first
batch; this is observation latency, not a guaranteed maximum repair time.
Both deletion runs recreated the required managed content while losing external
baseline rows, including localhost and the guest hostname mapping. Their exact
lost-row lists are recorded in each `p42-repaired.json`; this is the catalog's
separate whole-file-deletion outcome, not preservation of those deleted rows.

[Final safety inventory](../results/p42-final-safety.json) confirms all ten P4.2
domains/copied volumes are gone and base definitions/storage metadata still
match their preflight records. The five retained P4.1 failure clones and all
three base VMs were stopped at that checkpoint. No acceptance runner remained active.

Whole-file deletion records missing external rows separately and does not
claim reconstruction of deleted third-party content. The HTTP/NSS checks cover
IPv4; both IPv4 and IPv6 managed rows are compared exactly. This selection does
not establish browser allowlist enforcement, P4.3 or full Layer 4 acceptance.

## Cleanup before P4.3

At the operator's request, the five diagnosed P4.1 clones G–K and their exact
owned copied disks were deleted before P4.3. Their evidence bundles and failure
verdicts remain intact. Each deletion revalidated ownership, stopped state,
independent storage and base signatures under the shared VM lock. All three
base VMs remain intact and stopped. See the
[cleanup audit](../results/p43-preflight-old-clone-cleanup.json).

## Ubuntu P4.3 validation

Status: **runner implemented and executed; candidate fails daemon-restart
recovery; full adapter qualification withheld**. The same exact
DEB is used; no product fix or package rebuild is part of this selection.
Four fresh-clone scenarios exercise both native store profiles: paired managed
policy modification, paired manifest damage with terminated native connections,
transient daemon crash, and sustained daemon request-path outage. Manifest
recovery restores original package files explicitly; autonomous manifest repair
and GUI repair are not claimed. The sustained pause has independent root recovery
and uses the installed extensions' archived timing constants.

Seventeen local P4.3 checks passed for action/clone boundaries, installed timing,
bounded rescue, process identity before resume, rejected arbitrary recovery paths
current-fault reconnect evidence and refusal to qualify partial/failed results.
The observed candidate failure
prevents full P4.3 acceptance and the required pair of passing batches.

The first development batch `p43-ubuntu-20261006-a` is not accepted. Both
managed policies repaired, but a fixture assertion incorrectly required custom
keys that had already disappeared under the installed strict policy mode before
injection. Its manifest variant incorrectly expected open browsers after a
sustained native connection failure; both sessions closed around the configured
30-second strict heartbeat grace. The adapter now declares that termination and
reopens the same profiles after manifest restoration. Native process termination
also needs a bounded wait for signal delivery, rather than an immediate PID check.

The transient crash restarted the daemon but the client Unix socket pathname
was absent at the first two observations. The fresh batch B below confirms
that this persists through bounded retries, rather than a brief startup gap.
The sustained daemon pause passed the actual unavailable/fail-closed page checks
in both original tabs and both new navigations before release (about 158 seconds
of witnessed pause against a 105-second observation window). After SIGCONT the
strict guard closed both sessions; automatic browser recovery did not pass.
The corrected adapter records manual same-profile reopening separately.

Development sources changed between scenarios as those fixture issues were
diagnosed; the batch cannot qualify the adapter. Each bundle retains its own
captured source version and original verdict. No candidate binary was modified.

The [read-only offline diagnosis](../results/p43-development-offline-diagnosis.json)
archived each stopped clone's event log before any further action. It confirms
`browser_killed_extension_stale` for both families: heartbeat ages 31/33 seconds
in the manifest scenario and 171/174 seconds after the sustained pause, against
the configured 30-second grace. The two diagnosed fixture-failure clones were
[removed with evidence preserved](../results/p43-diagnosed-development-cleanup.json);
the crash and outage clones remain stopped for diagnosis. The
[development audit](../results/p43-development-audit.json) rejects acceptance.

The likely socket-loss cause is package unit lifecycle: the daemon owns
`RuntimeDirectory=blockuntu`, while the socket unit listens within that directory,
and the exact DEB has no preservation setting. Systemd removes those directories
when a service stops unless preservation is configured. This cause is inferred
from the package units and runtime failure, not a tested fix.
[Systemd execution documentation source](https://raw.githubusercontent.com/systemd/systemd/main/man/systemd.exec.xml),
[exact-DEB unit extraction](../results/p43-candidate-systemd-evidence/result.json).

The fresh complete batch `p43-ubuntu-20261006-b` used identical captured runtime
sources in all four clones and recorded 8/12 passing observations:

| Scenario | Result | Interpretation |
| --- | --- | --- |
| `policy` | 3/3 passed; clone/storage removed | Both canonical policy files repaired and both original browser processes recovered fresh heartbeats and correct navigation. |
| `manifest` | 2/3 passed; diagnosed clone subsequently removed | Declared strict termination passed. Both original profiles recovered unchanged extensions, fresh heartbeats and allowed/named-blocked pages, but a leftover ancillary `same_browsers: false` flag incorrectly failed the declared restart variant. [Harness diagnosis](../results/p43-b-manifest-harness-diagnosis.json); corrected fresh subset C passed below. |
| `crash` | Baseline passed; restart and recovery failed | Confirmed persistent RPC socket loss after automatic daemon restart. The daemon/watchdog/socket units stayed active, while the pathname was absent for about 156 seconds and 149 RPC attempts failed. |
| `outage` | Baseline and sustained fail-close passed; automatic recovery failed | Both original allowed tabs and both new navigations showed unavailable/fail-closed pages before release. The strict guard then closed both sessions. Reopening the original profiles restored fresh current-session heartbeats and allowed/named-blocked navigation with unchanged extension code. |

The [socket-loss evidence](../results/p43-confirmed-socket-loss.json) includes
an open listening descriptor in both systemd and the restarted daemon despite
the missing pathname, plus installed unit files matching the exact DEB. This
is a candidate failure; no source fix or package rebuild has been made.

The outage's post-resume closure is a separate behavior decision: the current
strict guard requires a browser reopen. A bounded reconnect grace while the
extensions continue to fail closed is recommended if automatic recovery should
preserve running windows. The operator choice remains pending; the suite keeps
the automatic-recovery failure separate from successful manual recovery.
The [complete-batch audit](../results/p43-final-audit.json) refuses acceptance.
Two comparable full passing batches cannot be claimed on this candidate.

The corrected fresh subset `p43-ubuntu-20261006-c-manifest` passed **3/3**
observations and [independent selected-evidence validation](../results/p43-manifest-validation.json).
The archived event log contains 17 native-host disconnect diagnostics per browser
observed during manifest damage, establishing repeated failed reconnect attempts.
Both original packaged manifests were restored exactly; the declared relaunch
used the original profiles, unchanged extension code and fresh current-session
heartbeats. Baseline allowed pages and recovered named block pages were observed.
Its domain/copied storage were removed with base integrity verified. This subset
does not replace the failed full batch or qualify the adapter.

After that confirmation, the redundant B manifest fixture-failure clone was
[removed with its evidence and failure verdict preserved](../results/p43-b-manifest-cleanup.json).
The [final safety inventory](../results/p43-final-safety.json) verifies 44 owned
P4.1–P4.3 clone records: 40 domains/copied volumes removed and four stopped
P4.3 crash/outage clones retained for diagnosis. All three base VMs remain
stopped with unchanged definitions/storage metadata; no acceptance runner is active.

The [consolidated findings](../results/p43-findings.json) distinguish the confirmed
P4.1 user-native cleanup defect and P4.3 RPC recovery defect from fixture issues
and the outage behavior decision. The next product-fix candidate should address
those confirmed defects together, then rerun P4.1 and two complete comparable
P4.3 batches. P4.2 has no new confirmed defect. Upgrade testing remains deferred;
no product fix or package rebuild was made during this selection.
