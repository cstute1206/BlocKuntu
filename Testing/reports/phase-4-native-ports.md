# Phase 4 native Fedora and CachyOS ports

Status: selected native validation complete. Qualified milestones and remaining
failures are recorded below. Product fixes remain deferred.


| Milestone | Fedora | CachyOS |
| --- | --- | --- |
| Native smoke prerequisite | 21/21 pass; audited | 21/21 pass; audited |
| P4.1 protected uninstall | 11 pass, 1 DNF failure; unqualified | 2 pass, 3 fail, 7 blocked; unqualified |
| P4.2 hosts repair | Qualified: two complete 11/11 passes; pair audit passed | 11/11 then 10 pass, 1 harness schedule-timing failure; pair unqualified |
| P4.3 browser recovery | Qualified: two corrected fresh 12/12 passes; pair audit passed | Qualified: two corrected fresh 12/12 passes; pair audit passed |

CachyOS's confirmed P4.1 product failures are two successful direct Pacman
removals. Fedora loses GUI dependencies after DNF refuses removal. The CachyOS
first authorized supplement reached cleanup, but two harness errors prevented
acceptance: an observer timestamp race and an unsupported HTTP fixture route.
The corrected fresh authorized supplement passed all eight checks and its
independent audit. Whole P4.1 acceptance remains blocked by the direct-removal
defects. Both native P4.3 pairs qualified. CachyOS P4.2 remains unqualified because
its second fresh batch failed the schedule observation's clock assertion after
evidence collection; the recorded initial inactive observation was valid.

The selected scope is P4.0 infrastructure for P4.1 protected uninstall, P4.2 hosts
repair, and P4.3 policy/manifest repair, daemon restart, and sustained outage.
Upgrades and P4.4–P4.8 remain separate selections.

The candidate is `9df72e0749a01d2019ed3116eb693a1f5e573980` from
[package workflow 37907529116](https://github.com/cstute1206/BlocKuntu/actions/runs/37907529116).
Both downloaded archives matched GitHub's SHA-256 digest, passed ZIP integrity
and safe-path checks, and contained passed provenance-verified Layer 2 reports.
Each selected package matched its report checksum.

| Adapter | Candidate package | Native browsers | Direct removal selections |
| --- | --- | --- | --- |
| Fedora | RPM `0.2.0-1.fc43` | Firefox and Google Chrome | `dnf remove --assumeyes blockuntu`; `rpm -e blockuntu` |
| CachyOS | Arch `0.2.0-1` | Firefox and Chromium | `pacman -R --noconfirm blockuntu`; `pacman -Rns --noconfirm blockuntu` |

The historical runner scenario names `remove` and `purge` identify two independent
attempts. Fedora and Arch have no Debian purge transaction: their exact commands
above are the native variants. Fedora refusals use `DES-UNINSTALL-002`, CachyOS
refusals use `DES-UNINSTALL-003`. Refusal requires a native error, the actual refusal
message, no lease, the package still installed, continued hosts/process enforcement,
and a freshly started GUI with working settings after the attempted transaction.

Authorized removal still goes through the actual GUI, its recovery phrase,
protected-access checks, natural schedule/Detox expiry, and daemon-issued lease.
The disposable clone's Polkit rule permits only the exact GUI native command with
a correctly shaped lease argument for the active local test user; it does not
create a lease or replace the package manager. Native removal logs must identify
this invocation and exact installed version. Browser policy paths, extension
components, journal units, screenshots, and recovery identities follow the actual
native browser pair. No installed BlocKuntu binary or published extension is patched.

The shared clone guards, independent root collectors, bounded fault rescue,
checksummed captured harness, current-session heartbeats, visible named pages,
and cleanup/base-integrity checks remain required. Qualification audits reject
mixed distributions, artifact types, native browser pairs, changed harnesses,
and reused clones. New adapters require two complete fresh passing batches each.

Evidence and live execution status:

- [Candidate and run evidence directory](../results/p4-ports-20261009/)
- [Fedora artifact verification](../results/p4-ports-20261009/fedora-artifact-verification.json)
- [Arch artifact verification](../results/p4-ports-20261009/arch-artifact-verification.json)
- [Immutable-base preflight](../results/p4-ports-20261009/preflight.json)
- [Harness regression results](../results/p4-ports-20261009/harness-tests.log)
- [Selected native result index](../results/p4-ports-20261009/native-milestone-status.json)
- [Reviewed recovery screenshots](../results/p4-ports-20261009/visual-reviews.json)
- [Current qualification execution](../results/p4-ports-20261009/qualification-progress.json)
- [Execution progress](../results/p4-ports-20261009/progress.json)
- [Refreshed CachyOS execution progress](../results/p4-ports-20261009/native-refresh-progress.json)

All **96 Layer 4, 29 VM harness, and one HTTP fixture** regression checks pass after
the follow-up corrections. The previous 90-test results and safety receipt are
preserved. Installed-package qualification remains bound to each complete pair
and its captured source checksums.

## First Fedora observations

The fresh smoke prerequisite passed all 21 selected observations, with an
[independent audit](../results/p4-ports-20261009/fedora-smoke-audit.json).
P4.2's first complete batch passed all 11 observations across five fresh clones;
its [audit](../results/p4-ports-20261009/p42-fd-a-audit.json) also passed.
One batch does not qualify a new adapter.

P4.1 revealed a product defect in protected DNF removal. The `%preun` refusal
keeps BlocKuntu installed and its daemon enforcing policy, but DNF still removes
dependencies selected for automatic removal. A separate fresh clone confirmed
that the GUI subsequently panics because `libayatana-appindicator3.so.1` and
compatible appindicator libraries are missing. This breaks the authorized GUI
uninstall path even though the native command returned an error. See the
[post-refusal GUI diagnostic](../results/p4-ports-20261009/fedora-post-refusal-gui.json)
and [native transaction evidence](../results/p41-fd-1009-a-remove/fedora/acceptance/direct-remove.log).
The diagnostic is supplemental evidence, not a qualifying P4.1 pass.

Two harness issues were also identified. A fresh GNOME session can remain in
overview, hiding Firefox's extension permission prompt; onboarding now explicitly
focuses the actual browser before acting. Fedora P4.3's policy variant subsequently
passed baseline, repair, and recovery. A different fresh clone then exposed a
transient null AT-SPI child during tab selection, before fault injection. Its
manifest cases remain blocked in that original batch. The correction was applied
between complete
suites. These failures are distinct from product recovery defects.

Fedora P4.3's crash and sustained-outage variants also passed all three required
observations each. The sustained outage lasted about 164 seconds, produced actual
failed RPC communication and fail-closed pages, and recovered automatically in
the original Firefox and Chrome PIDs/profiles. The first batch remains incomplete
because of its manifest baseline failure; its qualification audit correctly fails.

The refused-removal oracle now requires a freshly started, interactive GUI after
each native removal attempt. A prior running GUI cannot conceal missing libraries.
The native auditor also requires matching baseline/post-refusal GUI evidence.
A complete fresh Fedora P4.1 retry ran with this stronger oracle, the browser
focus correction, and DNF5's completed-uninstall log witness.

The two stopped Fedora clones with diagnosed pre-fault onboarding failures were
[removed after triage](../results/p4-ports-20261009/triaged-harness-clone-cleanup.json).
Their original failed results remain unchanged and their diagnostic evidence is
retained. The DNF product-defect clone remains stopped for inspection.

The fresh P4.1 retry confirmed the same DNF defect under the stronger oracle;
see its [post-refusal GUI journal](../results/p41-fd-1009-b-remove/fedora/acceptance/p41-gui-post-refusal.json).
The independent raw-RPM refusal variant passed baseline and post-refusal checks,
including fresh GUI interaction. Authorized GUI removal passed all eight checks,
including wrong-phrase rejection, active schedule/Detox rejection, natural expiry,
leased GUI removal, unrelated-state preservation, and same-session browser usability
beyond failure tolerance. The complete retry has 11 passes and one DNF failure,
so P4.1 remains unqualified on this candidate.

Recommended Fedora fix direction: reject an unauthorized complete DNF transaction
before RPM execution begins, keeping the existing raw-RPM refusal as well.
[DNF5's Actions plugin](https://dnf5.readthedocs.io/en/stable/libdnf5_plugins/actions.8.html)
supports `pre_transaction`, outgoing-package filters, an explicit stop request,
and `raise_error=1` for helper failures. Its
[documented workflow](https://dnf5.readthedocs.io/en/stable/dnf5_workflow.html)
places that callback before the RPM transaction. A package-owned action should
identify actual BlocKuntu erasure, validate the GUI-issued lease without consuming
it, and abort the entire transaction when authorization is missing or invalid.
The RPM scriptlet can then consume the lease during authorized removal. Upgrade,
replacement, and reinstall actions need explicit handling so they cannot be
confused with an ordinary removal. This is a proposed implementation direction,
not a tested product fix; the candidate and its package dependencies are unchanged.

## Rolling CachyOS fixture preparation

The first CachyOS smoke clone passed VM infrastructure checks, but candidate
installation failed before any acceptance case: its cached repository metadata
requested `webkit2gtk-4.1-2.52.6-1`, which returned HTTP 404 from the mirrors.
The transaction did not commit. This is an environment failure, with no destructive
fault injected; see the [installation log](../results/p4port-cachyos-smoke-1009/cachyos/acceptance/install.log).
Its [obsolete clone was removed](../results/p4-ports-20261009/stale-cachyos-smoke-cleanup.json)
after diagnosis, preserving the failed evidence.

The retry uses the existing clone-only OS refresh/reboot procedure, consistent
with [CachyOS's system-update guidance](https://wiki.cachyos.org/configuration/post_install_setup/#updating-the-system)
and [Arch's requirement to avoid partial upgrades](https://wiki.archlinux.org/title/System_maintenance#Partial_upgrades_are_unsupported).
Layer 4 exposes the same explicit `--refresh-cachyos` option. It records the
preparation choice and raw logs, checks clone ownership before package mutations,
and requires BlocKuntu absent before and after OS preparation. The exact candidate
is installed afterward. This prepares a fresh rolling-distribution fixture;
BlocKuntu upgrades and P4.8 remain deferred. Qualification requires matching
preparation modes as well as captured harness and native browser/extension versions.

The first refresh attempt exposed a separate fixture issue: `pacman -Scc` failed
on a stale `download-ERwY3b` staging directory in the supplied cache, before OS
update or candidate installation. The helper now removes only inactive, narrowly
named staging directories after checking clone ownership, pacman locks/processes,
cache ownership, and symlinks. Unrelated entries remain untouched by this helper.
The [diagnosed clone was removed](../results/p4-ports-20261009/stale-cachyos-cache-cleanup.json)
with its original failure evidence retained. The fresh retry completed full OS
refresh/reboot, installed the exact candidate, and passed all 21 selected smoke
observations. Its [independent audit](../results/p4-ports-20261009/cachyos-smoke-audit.json)
passed and the clone was removed. BlocKuntu upgrade testing remains skipped by
agreement.

The first destructive CachyOS baselines exposed a strict literal-address assumption
in the harness, before any fault was injected. Normal NSS returns no address for
blocked domains; forcing the `files` NSS module confirms the exact `0.0.0.0`
mapping. The supplied system puts `resolve` ahead of `files`, and
[systemd's resolver implementation](https://github.com/systemd/systemd/blob/main/src/resolve/resolved-etc-hosts.c)
intentionally maps null hosts addresses to no address. See the
[normal/files/resolved diagnostic](../results/p4-ports-20261009/cachyos-nss-blocked-diagnostic.log).
This is a harness mismatch, not evidence of a product failure or a successful
destructive test. The correction preserves native resolver configuration, requires
the exact files-only mapping, permits normal NSS's explicit no-address result,
and retains working control-domain and actual HTTP checks. Invalid native results,
missing mappings, and reachable blocked destinations cannot pass.

[Diagnosed pre-fault clones](../results/p4-ports-20261009/triaged-cachyos-nss-cleanup.json)
were removed with original failure bundles preserved. The old hosts batch
finished with its captured harness unchanged: four blocking baselines failed,
five dependent observations were blocked, and the two allowlist observations
passed. The remaining diagnosed clones were
[removed after collection](../results/p4-ports-20261009/remaining-cachyos-nss-cleanup.json);
the successful allowlist clone had already been removed by its runner.
The correction is now applied and its two new resolver regressions pass.
Fresh corrected batches and qualification audits are tracked in
[qualification progress](../results/p4-ports-20261009/qualification-progress.json)
and the [current supervisor log](../results/p4-ports-20261009/qualification-supervisor-d.log).


The first resolver correction exposed a second assumption before uninstall fault
injection: this updated guest's GNU libc 2.44 `getent` returns 15 for no address,
while the harness expected the generic 2. The
[version/exit/API diagnostic](../results/p4-ports-20261009/cachyos-getent-exit-diagnostic.log)
confirms that Python's ordinary NSS API returns `EAI_NODATA`, and the files-only
mapping is correct. The planned replacement runs a bounded, separate
`socket.getaddrinfo` probe, records its actual error category, and rejects temporary
and system errors rather than relying on a utility exit code.
Its [live blocked/control verification](../results/p4-ports-20261009/cachyos-bounded-nss-proof.json)
passed on the stopped diagnostic clone without changing the installed product or
resolver settings. That is supplemental evidence only. Original failed bundles
remain failed; the replacement is applied between complete suites and then tested
on fresh clones.

The bounded API replacement is applied. All 80 Layer 4 and 27 VM harness checks
pass, including actual literal-address API execution, explicit denial/error
classification, bounded child-probe failure, and files-mapping requirements.
The [pre-fault utility-mismatch clones were removed](../results/p4-ports-20261009/cachyos-getent-exit-cleanup.json)
with original failed evidence retained. Fresh native validation now uses
[this captured source pin](../results/p4-ports-20261009/bounded-nss-source-sha256.json).

## KDE preparation and supplemental Pacman finding

The corrected resolver allowed P4.1 to reach native desktop setup. That exposed
missing fixture preparation in the new GUI runners: KDE still had only the
supplied German keyboard layout. The established smoke runner already writes
`kxkbrc` before the post-install reboot. The shared, clone-guarded helper now
performs those same writes for P4.1 and P4.3. Guest-side verification of the active
US layout remains mandatory before keyboard input.

The [live keyboard/GUI proof](../results/p4-ports-20261009/cachyos-keyboard-gui-proof.json)
confirms the active US layout, hosts/process enforcement, healthy clock, and fresh
GUI settings interaction after reboot. It is supplemental evidence, not a
qualifying batch. Two diagnosed pre-fault clones were
[removed](../results/p4-ports-20261009/cachyos-keyboard-preflight-cleanup.json),
with the first retained for native package diagnosis. All 80 Layer 4 and 29 VM
harness checks pass; [final captured source hashes](../results/p4-ports-20261009/native-final-source-sha256.json)
and fresh validation progress are recorded.

A subsequent guarded direct `pacman -R --noconfirm blockuntu` attempt on the
verified diagnostic baseline, explicitly without a removal lease, confirmed a
product defect. The transaction prints the refusal message but removes the
package, its systemd units, and managed hosts rows. The independent root collector
survived. Package absence persisted after reboot. See the
[supplemental finding](../results/p4-ports-20261009/arch-direct-removal-diagnostic.json)
and [before/after transaction evidence](../results/p4-ports-20261009/arch-direct-remove-probe/).
The wrapper's acknowledgement was not retained across the diagnostic hard stop;
its exit code is therefore unclaimed in this supplemental result. The complete
fresh runner will record the actual command result.

The exact candidate's
[install script](../results/p4-ports-20261009/arch-candidate-install-script.txt)
has rejection branches that do not stop the enclosing authorization function.
A correction must also reject the transaction before removal begins:
[ALPM's `PreTransaction` hook with `AbortOnFail`](https://man.archlinux.org/man/alpm-hooks.5.en)
provides that transaction-abort guarantee. A printed refusal inside a package
scriptlet does not prove package retention. Product code and candidate packages
remain unchanged in this test-port round.

The complete fresh `p41-cy-1009-d-remove` runner independently reproduced the
Pacman defect after its functional and GUI baseline passed. The read-only
[retained command receipt](../results/p4-ports-20261009/p41-cy-d-remove-command-ack.json)
records return code **0** and the refusal message. Its
[inspection](../results/p41-cy-1009-d-remove/cachyos/acceptance/inspection.json)
records package absence, inactive/not-found units, and removed managed hosts rows.
The [superseded supplemental clone was removed](../results/p4-ports-20261009/superseded-arch-probe-cleanup.json);
this complete-run product-defect clone remains stopped for inspection.


## Complete CachyOS P4.1 observation

The second independent native variant, `pacman -Rns --noconfirm blockuntu`, also
passed its baseline and then removed the package despite the refusal message.
Its [retained command receipt](../results/p4-ports-20261009/p41-cy-d-purge-command-ack.json)
confirms exit **0**. Both direct variants therefore fail the GUI-only uninstall
requirement. Their stopped clones and complete raw evidence are retained.

The authorized variant failed before any uninstall/protection fault: Firefox
showed **Daemon heartbeat missing**, rather than the required named policy page.
The [actual visible document](../results/p41-cy-1009-d-authorized/cachyos/acceptance/p41-firefox-failure.json)
and [daemon journal](../results/p41-cy-1009-d-authorized/cachyos/acceptance/journal.log)
record real RPC timeouts and broken pipes, including heartbeat and URL evaluation
requests. The baseline is failed and seven dependent observations are blocked;
this is not an authorized-removal result. It remains an unresolved baseline
communication failure rather than a proven uninstall defect. Independent native
P4.2/P4.3 batches continue with the exact captured harness unchanged.

The [complete P4.1 summary](../results/p41-cy-1009-d/summary.json) has two passing
baselines, two failed direct-removal observations, one failed authorized baseline,
and seven blocked observations. No CachyOS P4.1 qualification is claimed.

## Native hosts sentinel observation timing

The fresh CachyOS P4.2 append variant preserved the external sentinel and repaired
the managed rows and immutable flag, with correct blocking and control HTTP.
Its immediate sentinel-resolution assertion nevertheless failed about 1.36 seconds
after the fault. That assertion recorded a boolean rather than the actual answer.
The original failed bundle remains failed.

The [retained-clone NSS diagnostic](../results/p4-ports-20261009/append-sentinel-resolver-diagnostic.json)
confirmed the correct normal/files/API mapping after reboot. A separate
[bounded hot-change experiment](../results/p4-ports-20261009/hot-hosts-nss-experiment.json)
reproduced the delay without changing the installed product: the files module saw
the new external mapping immediately, while normal NSS returned the previous
fixture DNS address for roughly two seconds, then matched the new mapping.
The experiment restored the exact original hosts bytes and immutable flag.
[Systemd's resolver source](https://github.com/systemd/systemd/blob/main/src/resolve/resolved-etc-hosts.c)
limits hosts-file rechecks to once every two seconds, consistent with the observed
delay. This supports a bounded observation-timing correction rather than treating
the preserved row as a product cleanup failure.

The complete batch finished with ten passes and the one timing failure, with its
captured harness unchanged. The bounded correction is now applied: every sentinel
lookup is captured, and normal getent, the files module, and the normal NSS API
must settle on the same expected address within a 30-second observation window.
It does not flush caches or replace native resolver configuration. The independent
auditor also requires the raw matching lookup and the external row in the root
checkpoint. Regression cases reject cached results, inconsistent interfaces,
temporary errors, missing mappings, and unsupported claims of success.

All 86 Layer 4 and 29 VM checks pass. The
[diagnosed timing clone was removed](../results/p4-ports-20261009/cachyos-sentinel-harness-cleanup.json)
with failed evidence preserved. Fresh complete batches use
[the new captured source pin](../results/p4-ports-20261009/native-settled-source-sha256.json),
with [live progress](../results/p4-ports-20261009/qualification-progress.json) and
[historical supervisor log](../results/p4-ports-20261009/qualification-supervisor-e.log).

## First settled hosts batch and browser observation corrections

The fresh CachyOS hosts batch `p42-cy-1009-c` passed all 11 observations;
its [independent audit](../results/p4-ports-20261009/p42-cy-1009-c-audit.json)
passed, and all five disposable clones were removed. At that stage, the required
second comparable complete batch was pending.

After that complete batch finished, two browser observation corrections were
applied. The GUI-uninstall Firefox baseline now allows up to 45 seconds for
automatic recovery and requires the actual named rule and target URL in the
visible block page. A generic backend-unhealthy page cannot satisfy that check.
The original failed CachyOS authorized baseline remains failed; a fresh focused
authorized run was planned to test the complete GUI path separately.

Chromium's published extension reports `chrome` in diagnostics while the daemon
identifies its live component as `chromium_extension`. The
[actual native smoke heartbeat](../results/p4-ports-20261009/chromium-diagnostic-identity.json)
supports accepting that diagnostic alias only when bound to the confirmed active
current-session Chromium heartbeat. Fault kind, timestamps, repeated disconnects,
and actual recovery remain independently required. This changes the test oracle,
not the installed extension or daemon.

The first fresh Chromium manifest run subsequently passed baseline, effective
fault, and recovery. Its [diagnostic proof](../results/p4-ports-20261009/chromium-manifest-diagnostic-proof.json)
counts 23 Firefox and 22 Chromium disconnects during actual manifest damage.
Policy repair and transient daemon restart also passed their three observations
each. The sustained outage subsequently passed all three observations, with
about 163 seconds of actual communication loss, fail-closed pages, and automatic
recovery in the original browser sessions. The
[complete fresh batch audit](../results/p4-ports-20261009/p43-cy-1009-a-audit.json)
passed all 12 observations and confirms all four clones/storage removed. The
second comparable complete batch was still required for adapter qualification.

All 90 Layer 4 and 29 VM regression checks passed at that stage. The
[first-stage source pin](../results/p4-ports-20261009/native-browser-source-sha256.json)
and [first-stage supervisor log](../results/p4-ports-20261009/qualification-supervisor-f.log)
covered the then-remaining serialized runs. [Every captured source file](../results/p4-ports-20261009/p42-cy-c-source-compatibility.json)
in the passing CachyOS P4.2 batch was byte-identical after those browser-only changes,
allowing its prospective second complete batch to be compared by the qualification auditor.

Fedora's first complete final-harness hosts batch subsequently passed all 11
observations with [an independent passing audit](../results/p4-ports-20261009/p42-fd-1009-b-audit.json).
All five clones/storage were removed. Its second complete comparable batch
was pending, as was the fresh Fedora browser qualification pair.

## Fedora Firefox manifest fault diagnosis

The fresh Fedora manifest baseline passed, but its fault observation and dependent
recovery assertion failed. Chrome disconnected and was terminated after stale
heartbeats; Firefox reconnected and remained healthy. The second error described
an unexpected relaunch, but the evidence instead shows a Firefox window remaining
present because the intended communication fault did not persist for that browser.

The [root-collector diagnosis](../results/p4-ports-20261009/fedora-manifest-path-diagnostic.json)
records 37 samples with the selected manifest paths still damaged. A read-only
inspection of the stopped clone found separate regular BlocKuntu Firefox
registrations in `/usr/lib/mozilla` and `/usr/lib64/mozilla`, with different inodes.
The fault damaged only the RPM library path. This fixture runs Mozilla's native
release archive; [Mozilla documents both system registration locations](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_manifests).
The surviving registration explains why this was an incomplete fault rather than
a demonstrated daemon or extension recovery defect. The raw failed bundle stays failed.

An [isolated fixture regression](../results/p4-ports-20261009/fedora-manifest-proposal-check.json)
reproduces the omitted registration and verifies the proposed fixed target set:
both Firefox registrations plus Chrome, original native connections terminated,
then all original bytes restored. Ubuntu and CachyOS target sets are unchanged.
The correction was prepared outside the pinned source for application after
the complete runs finished. Fedora browser qualification then required two new
complete comparable passing batches on the corrected fixture.

The complete Fedora batch finished with ten passes and those two manifest
failures. Policy repair, transient restart, and sustained-outage recovery each
passed all three observations. The focused CachyOS GUI-uninstall supplement
was running on a new clone; it could not qualify a complete P4.1 batch.

## CachyOS authorized supplement and second hosts batch

The focused `p41-cy-1009-e` authorized journey recorded six passes and two failures.
Both browser baselines, invalid phrase, active schedule, active Detox, natural
expiry, and post-removal cleanup passed. This focused selection does not replace
a complete qualifying P4.1 batch.

The two failed assertions are harness defects, rather than demonstrated product
recovery defects. The first compares a heartbeat against the observer's receipt
time, which is later than the daemon's transition: Chromium's heartbeat precedes
the first `uninstalling` observation by just 119 microseconds. A prepared correction
reads the actual mode transition timestamp from the daemon database in read-only
mode and still requires current-session heartbeats after that transition. The old
failed bundle remains failed; journal response lengths alone are not proof of a
received uninstall notification.

The second assertion visits `/chromium/exact/blocked`, while the HTTP fixture only
serves Firefox and Chrome browser routes. The reviewed screenshot shows the fixture's
plain `not found` response at the requested HTTP URL, rather than a BlocKuntu block
page. Both original browser processes remain alive and both extension management
pages are accessible. Firefox's formerly blocked route renders correctly. The
prepared fixture correction also serves the native Chromium route, preserving the
requirement to reopen the exact previously blocked URL. A fresh authorized journey
is required to prove the full recovery result.

- [Focused supplement summary](../results/p41-cy-1009-e/summary.json)
- [Actual handoff samples](../results/p41-cy-1009-e-authorized/cachyos/acceptance/p41-handoff.json)
- [Independent cleanup findings](../results/p41-cy-1009-e-authorized/cachyos/acceptance/p41-cleanup-findings.json)
- [Browser recovery observations](../results/p41-cy-1009-e-authorized/cachyos/acceptance/p41-browser-recovery.json)
- [Reviewed Chromium HTTP screenshot](../results/p41-cy-1009-e-authorized/cachyos/acceptance/p41-chromium-recovered.png)

The second CachyOS P4.2 batch, `p42-cy-1009-d`, caught a similar settling race at
its remove-scenario baseline: the independent files lookup returns the managed
`0.0.0.0` row, while normal DNS still returns the previous `127.0.0.2` and HTTP is
accessible. The baseline currently samples immediately after observing file repair.
A prepared bounded correction waits for normal DNS, actual HTTP denial, and accessible
control sites to agree, retaining every attempted observation. It never flushes DNS
or changes resolver configuration. A timeout remains a failed baseline. The completed batch records eight passes, one failed baseline, and two blocked
remove-scenario observations; its failed baseline prevents pair qualification.
Modify, append, delete, and allowlist all passed their required observations.

These initial corrections and four focused regressions were checked in an isolated proposed
harness. At that stage, they had not changed the pinned running sources.
Fedora's prepared manifest correction and the follow-up corrections were queued
for application between whole batches, followed by fresh selected validation.

Two diagnosed stopped CachyOS authorized clones were removed after complete raw
evidence collection. Their failed result files remain unchanged; the ownership
records and [triage cleanup receipt](../results/p4-ports-20261009/cachyos-uninstall-supplement-triage-cleanup.json)
prove domain/storage removal and unchanged bases. The original direct-removal
product-failure clones remain stopped and retained.

## Second CachyOS browser batch: policy page observation

`p43-cy-1009-b-policy` passed baseline and automatic policy repair, then failed
the recovery assertion for the Firefox rule name. The assertion samples document
text immediately after finding the generic `Blocked by BlocKuntu` title, which
can be shared by a previous or transient page. It does not wait for the expected
rule and URL. No post-failure visible-page snapshot was captured by this harness.
This remains a real failed acceptance observation, with the product/harness
classification unresolved until fresh bounded observation.

The daemon event log later records `url_blocked` decisions for the correct Firefox
URL and `P41 exact firefox`. Those backend decisions do not prove what was visible
in the browser. Read-only extraction of the stopped clone's Firefox session files
produced snapshots from **before** the fault, so they cannot establish recovery.
The diagnostic records their actual update times rather than relabeling baseline
state as a recovered page.

A prepared correction uses the already tested 45-second named-page observation
for P4.3 recovery, requiring both the exact native rule name and the target URL.
It also captures text and screenshots from failed browser journeys. A fifth
isolated regression rejects transient backend pages and a wrong Chrome rule/URL
while testing Firefox and Chromium recovery. All five isolated follow-up regressions
passed; at that stage, the shared running harness remained at its 90-test pin.

After this failure, two fresh complete corrected CachyOS P4.3 batches were required
alongside the planned Fedora browser and CachyOS hosts pairs. The passing first browser batch
and failed second batch cannot be mixed with a later harness to claim qualification.

- [Failed policy recovery observation](../results/p43-cy-1009-b-policy/cachyos/acceptance/guest-results.json)
- [Actual daemon event log](../results/p43-cy-1009-b-policy/cachyos/acceptance/event.log)
- [Read-only session diagnostic](../results/p4-ports-20261009/cachyos-policy-session-recovery-diagnostic.json)
- [Isolated follow-up regressions](../results/p4-ports-20261009/followup-proposal-tests.log)

The second CachyOS browser batch completed with **11 passes and one failed policy
page assertion**. Manifest, daemon crash, and sustained outage each passed all
three observations. The outage recovery kept both original browser processes
and required no diagnostic relaunch. The failed complete batch cannot qualify
the adapter; a complete corrected pair was still required at that stage.

Fedora P4.2 qualified with two complete fresh 11/11 passes (`p42-fd-1009-b` and
`p42-fd-1009-c`) and a passed [pair audit](../results/p4-ports-20261009/p42-fedora-qualification.json).
The first pinned stage ended safely: all original bases stayed unchanged and off,
accepted clones were removed, and staging was unchanged. Its [safety receipt](../results/p4-ports-20261009/before-followup-safety.json)
and [90-test log](../results/p4-ports-20261009/harness-tests-90.log) remain preserved.

The reviewed Fedora manifest, native HTTP route, exact handoff timestamp, bounded
DNS/HTTP connectivity, and named browser-page corrections are now applied between
whole batches. All 96 Layer 4, 29 VM harness, and one HTTP fixture checks pass.
The corrected 34-file [source pin](../results/p4-ports-20261009/native-manifest-source-sha256.json)
and [implementation receipt](../results/p4-ports-20261009/native-followup-implementation.json)
record this transition. Application binaries and the selected candidate remain unchanged.

Fedora's P4.2 qualification remains explicitly bound to its captured first-stage
harness. A [version compatibility check](../results/p4-ports-20261009/fedora-hosts-version-compatibility.json)
examined all ten original clone bundles: unchanged sources match their digests;
unchanged host functions compare structurally; the differences are the added
bounded connectivity observation, optional diagnostic collection, unused uninstall
helpers, and the additional Chromium HTTP route. Every original baseline/repair
functional predicate passed on its first sample, so the added wait would accept
those same measured inputs immediately. This check does not claim identical
source bytes or manufacture a new qualification run.

## Fresh authorized uninstall and October 10 validation

The corrected CachyOS authorized supplement `p41-cy-1009-f` passed **8/8** and
its [independent audit](../results/p4-ports-20261009/p41-cachyos-authorized-supplement-audit.json).
The audit verifies the actual daemon handoff timestamp, current-session browser
heartbeats, native package removal, preservation of unrelated policy/hosts entries,
and recovery in both original browser processes past their installed tolerance.
Visual review confirms both formerly blocked HTTP routes render the deterministic
test site after uninstall. Its disposable clone was removed and bases stayed
unchanged. This focused supplement does not qualify the complete P4.1 adapter.

The interrupted Fedora `p43-fd-1009-c` batch contains no completed destructive
observations and is excluded from qualification. Its stopped disposable VM was
removed with an [ownership-checked receipt](../results/p4-ports-20261009/interrupted-fedora-p43-c-cleanup.json);
raw incomplete evidence remains preserved.

The user selected six complete fresh batches on October 10: Fedora P4.3
`p43-fd-1010-a`/`b`, CachyOS P4.3 `p43-cy-1010-a`/`b`, and CachyOS P4.2
`p42-cy-1010-a`/`b`. They run sequentially on the unchanged candidate and corrected
34-file harness pin, followed by each complete pair's qualification audit.
Product fixes and upgrade testing remain deferred.

The first October 10 Fedora P4.3 batch `p43-fd-1010-a` passed **12/12** and its
[independent audit](../results/p4-ports-20261009/p43-fd-1010-a-audit.json).
Policy, complete native-manifest damage, automatic daemon restart, and sustained
outage each passed three observations. Reviewed screenshots show both named
recovery pages and the outage fail-closed pages. The sustained-outage recovery
kept both original browser processes; all four successful clones were removed.
The second fresh batch was required before qualification.

Fedora's second October 10 batch `p43-fd-1010-b` also passed **12/12**.
The [complete pair audit](../results/p4-ports-20261009/p43-fedora-qualification.json)
passed all **24 observations** and qualified the native Fedora P4.3 adapter.
It verifies independent clone identities, matching browser versions and published
extension code, identical captured harness sources, and complete fault/recovery
evidence. All eight successful clones were removed.

The first corrected October 10 CachyOS P4.3 batch `p43-cy-1010-a` passed **12/12**
and its [independent audit](../results/p4-ports-20261009/p43-cy-1010-a-audit.json).
Both reviewed policy-recovery screenshots show the exact expected native rule and
URL. The sustained outage produced visible fail-closed pages and recovered both
original browser processes to their named policy pages. All four successful
clones were removed. The second fresh batch was still required for qualification;
the historical failed policy observation is not relabeled as a pass.

CachyOS's second corrected batch `p43-cy-1010-b` passed **12/12** as well.
The [complete pair audit](../results/p4-ports-20261009/p43-cachyos-qualification.json)
passed all **24 observations** and qualified CachyOS P4.3 on the unchanged
`9df72e0` Arch candidate. Both fresh policy-recovery journeys have reviewed
Firefox/Chromium named-page screenshots. All eight successful clones were removed.
The historical failed batch and its incomplete visible-page evidence remain
preserved; they are not used to qualify the corrected harness.

The first corrected October 10 CachyOS hosts batch `p42-cy-1010-a` passed **11/11**
and its [independent audit](../results/p4-ports-20261009/p42-cy-1010-a-audit.json).
Removal, modification, appended unrelated entries, hosts-file deletion, schedule
transitions, and allowlist behavior passed. The captured baseline and repair
connectivity diagnostics satisfy the normal-resolution, blocked-HTTP, and
accessible-control predicates. All five successful clones were removed.
The second complete fresh hosts batch followed; its result is recorded below.

## Final hosts batch: schedule observation timestamp

`p42-cy-1010-b-remove` passed baseline and hosts repair, then failed
`DES-HOSTS-004` with `Missed initial inactive observation`. The actual recorded
inactive observation is **22:37:53.449098 +02:00**, before the schedule's
**22:38:00 +02:00** activation, and its three functional predicates all passed.
The test saves that observation, synchronously collects a host checkpoint, then
asserts the *later current clock* is still before activation. Collection crossed
the boundary and incorrectly invalidated the already captured observation.

The [timing diagnostic](../results/p4-ports-20261009/cachyos-hosts-schedule-timing-diagnostic.json)
identifies a harness assertion defect, without claiming the unobserved later
schedule stages passed. The failed result remains failed and cannot qualify the
pair. The stopped diagnosed clone was removed with an
[ownership-checked receipt](../results/p4-ports-20261009/cachyos-hosts-schedule-triage-cleanup.json);
all raw temporal evidence is retained. Remaining variants completed on the unchanged
pin. A correction should validate each captured observation timestamp against its
schedule boundaries, and a changed harness requires a fresh comparable pair.

## Completed October 10 selection

All six requested batches completed on the exact `9df72e0` native packages and
unchanged corrected harness: **69 passes, one harness timing failure, zero blocked
observations** across 26 fresh fault clones. No application fix or build was made.
The [selected validation receipt](../results/p4-ports-20261009/oct10-selected-validation.json)
records the six exact runs, counts, pair audits, and removal verification.

| Requested milestone | First fresh batch | Second fresh batch | Complete pair audit |
| --- | --- | --- | --- |
| Fedora P4.3 | `p43-fd-1010-a`: 12/12 | `p43-fd-1010-b`: 12/12 | Passed; qualified |
| CachyOS P4.3 | `p43-cy-1010-a`: 12/12 | `p43-cy-1010-b`: 12/12 | Passed; qualified |
| CachyOS P4.2 | `p42-cy-1010-a`: 11/11 | `p42-cy-1010-b`: 10 pass, 1 fail | Failed; unqualified |

In the second hosts batch, removal baseline/repair, modification, append, deletion,
and allowlist checks passed. Only `DES-HOSTS-004` failed as diagnosed above. Its
later schedule activation/expiry stages were not completed, so they are not
manufactured from the valid initial observation. The
[pair audit](../results/p4-ports-20261009/p42-cachyos-qualification.json)
correctly rejects that incomplete passing evidence.

The additional timing correction and new hosts pair were offered as an optional
follow-up. This selection ends with the requested six batches; the pinned harness
and failed result remain unchanged. Product fixes and application upgrades are deferred.

The [final safety verification](../results/p4-ports-20261009/final-safety.json)
passed: all 26 selected fresh clones, including the diagnosed failed clone, have
removed domains/storage; the interrupted replacement predecessor is also removed.
Original Ubuntu/Fedora/CachyOS templates remain unchanged and off, the 34-file
harness pin is unchanged, and pre-existing staged changes are preserved.
Older retained product-failure clones remain stopped for later diagnosis.
