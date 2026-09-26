# Layer 3 validation progress — 2026-09-26

## Current status

Ubuntu's initial smoke milestone remains validated by the September 21 runs
below. The refreshed CachyOS run `layer3-20260926-cachyos-b` completed with
**20 of 21 required observations passing**. It is a failed acceptance run:
Firefox's restart heartbeat missed the unchanged 30-second deadline. Two
consecutive complete passes are still required. Upgrades remain deferred.

- [CachyOS terminal report](../results/layer3-20260926-cachyos-b/cachyos/acceptance/result.json)
- [CachyOS harness manifest](../results/layer3-20260926-cachyos-b/cachyos/acceptance/harness-files.json)
- [Separate Firefox restart diagnostic](../results/layer3-20260926-cachyos-b/cachyos/diagnostic-firefox-restart/README.md)

The new supplied base has a 30 GiB virtual disk and a 26 GiB Btrfs filesystem
with approximately 17 GiB free. It runs kernel `7.2.7-1-cachyos`, native
Firefox `156.0.1-1.1`, and native Chromium `153.0.8010.52-1.1`. The runner now
discovers the single Btrfs partition instead of assuming partition 2, preserves
the base, uses its installed Firefox, and records its package/filesystem state.
The full rolling update is opt-in via `--refresh-cachyos`; this run did not
perform it. Preparation A failed on the changed partition layout before boot;
its copied storage was removed and its evidence retained.

CachyOS used the exact Arch package from Layer 2 run `35539295807`, source
commit `d4814ac8c538dce0c3dac184a540163d99e5306c`, SHA-256
`3ef44e3336b237d959dd560f96906de3a55c83be53d505bec18b2a79705e0eda`.
All installation, GUI import/export, process blocking/allowlisting, Firefox
onboarding/exact-URL/website-allowlist, and Chromium browser cases passed.
The pre-reboot KDE keyboard configuration fixed the earlier navigation issue.
Published store extensions were Firefox `0.2.6` and Chromium `0.2.5`.
Base metadata integrity was verified at the end of acceptance.

The separate restart diagnostic used the same signed Firefox store extension.
With all Detox sessions expired, launch 0 had no new heartbeat for 69 seconds.
Launch 1 also had none until deliberate navigation; it became active at
56.778 seconds. The initial diagnostic attempt overlapped the process Detox
and is explicitly invalid. The installed XPI has no `runtime.onStartup`
listener. Mozilla documents that an event page needs this listener to run at
least once per browser session:
[runtime.onStartup](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/runtime/onStartup).
A source fix is committed as `d1cf9be` on `fix/firefox-startup-heartbeat` in
[draft PR #10](https://github.com/cstute1206/BlocKuntu/pull/10); its
unit tests do not establish signed-store runtime acceptance. Publish a new
signed store version, then repeat the fresh-clone runs before claiming a pass.

Fedora remains dependent on the RPM native messaging path fix in
[PR #9](https://github.com/cstute1206/BlocKuntu/pull/9) and a new verified Layer 2
RPM. Its chooser changes still need a full fresh acceptance run with that RPM.
The September 25 CachyOS run failed during the OS refresh after SSH stopped
responding; no acceptance cases ran. Its cleanup guard also found the base
running. The September 26 runs use a fresh baseline record for the updated base.

Validation of the current harness: 20 acceptance-runner unit tests and 14
Phase 0 unit tests pass; Python compilation passes. The Firefox fix separately
passes all eight extension unit tests and the manifest check.
The shared Chrome harness passes seven tests, with the Firefox-only startup
case skipped. After collecting diagnostics, the CachyOS test clone was removed
through guarded cleanup; only the three stopped bases remain.

## Ubuntu milestone — September 21

Status: **initial Ubuntu smoke milestone validated**. Two consecutive fresh-clone
runs passed with identical harness manifests and the same verified CI package.
This covers the agreed Ubuntu/native Firefox/native Chrome subset, not the
complete Layer 3 matrix. Upgrade testing remains explicitly deferred.

| Run | Required observations | Failures | Base integrity | Clone cleanup |
| --- | ---: | ---: | --- | --- |
| `layer3-20260921-c` | 21 passed | 0 | Verified | Removed |
| `layer3-20260921-d` | 21 passed | 0 | Verified | Removed |

Each run took approximately 15 minutes, recorded one deferred upgrade case,
and left 59 broader acceptance case IDs planned. Both used Firefox 156.0 with
store extension 0.2.6 and Chrome 150.0.7871.186 with store extension 0.2.5.
Firefox navigation now passes managed-policy visibility, exact-URL blocking
with its near-miss, and Detox website allowlisting through native key input.

Terminal reports and harness manifests:

- [Run C](../results/layer3-20260921-c/ubuntu/acceptance/result.json)
- [Run D](../results/layer3-20260921-d/ubuntu/acceptance/result.json)
- [Harness manifest C](../results/layer3-20260921-c/ubuntu/acceptance/harness-files.json)
- [Harness manifest D](../results/layer3-20260921-d/ubuntu/acceptance/harness-files.json)

All 49 local Python harness tests pass. Obsolete diagnostic clones I, A and B
were removed using ownership-guarded cleanup; successful C and D removed
themselves. Final libvirt inventory contains only the stopped Ubuntu, Fedora
and CachyOS base guests. Historical failure evidence is retained below and in
local result directories; diagnostic probes are not counted as smoke passes.

## Artifact and scope

Current input: exact DEB from Layer 2 run
[35539295807](https://github.com/cstute1206/BlocKuntu/actions/runs/35539295807),
source commit `d4814ac8c538dce0c3dac184a540163d99e5306c`, package `0.2.0-1`.
SHA-256: `942897c2cbed659f7cb2a539236fe4d892d8f1495d7697c52676ab1ff13daec1`.
The archive contains the corrected watcher unit without `After=blockuntu.service`.
Historical September 20 runs below used the previous defective artifact from
run `35437630033` (commit `a18a1f5ad5c957bf16f94dca7d0589e30a39345f`).
No BlocKuntu build was performed. The application on the host is not modified.

The repeatable entry point and target subset are documented in
[phase-3.md](../phase-3.md). Fedora, CachyOS and the remaining acceptance matrix
are pending; upgrade testing remains explicitly deferred.

## Historical development evidence

`layer3-20260920-c` completed with a failing acceptance result:

- Phase 0 infrastructure and reboot passed.
- Installation, services, first-run credentials, real GUI Health panel and
  post-install reboot checks passed (`VM-INSTALL-001` through `005`).
- GUI append and duplicate append passed (`VM-DATA-001`, `002`).
- Conflict import failed because the harness assumed the native chooser would
  remember its directory; the open dialog then prevented export.
- Browser setup failed because the fixture HTTP server was polled before it
  was listening and connection refusal was not retried.
- Process blocking/allowlisting failed because fixture executable names began
  with the application's protected `blockuntu` prefix. The near-miss remained
  alive, but that observation alone is not accepted enforcement coverage.

These harness issues were corrected: explicit chooser navigation, bounded HTTP
readiness polling, `bk-test-*` executable copies, and waiting for final process
names before asserting identities. Browser assertions now inspect shown
documents rather than searching all browser text, including background tabs.

`layer3-20260920-d` passed baseline/install/services but stopped during guest
prerequisites: Ubuntu's unattended updater held the dpkg frontend lock.
Installation commands now wait up to 180 seconds for that lock.

Failed runs retain local evidence under
`Testing/results/RUN_ID/ubuntu/acceptance/`. Base integrity checks passed.
The stopped clones C and D were subsequently removed with the guarded cleanup
command to free storage for retries; their evidence remains available.
Development probes on the earlier `layer3-20260919-b` clone installed the real
Firefox and Chrome store extensions and observed their daemon heartbeats;
those probes do not count as end-to-end smoke passes.

## Harness validation

- `python3 Testing/scripts/test_vm_acceptance.py`: 13 checks passed, including
  artifact trust boundaries and rejection of background-tab page evidence.
- `python3 Testing/scripts/test_phase0_vm.py`: 12 checks passed.
- `python3 Testing/scripts/test_package_ci.py`: 19 checks passed, including the
  added regression for the hosts-watcher ordering dependency. These use temporary
  fixture archives; no application compilation/build was performed.
- `bash Testing/scripts/verify-phase0.sh`: passed outside the sandbox (the local
  HTTP server cannot bind in the sandbox).
- Python compilation and `git diff --check`: passed.

`layer3-20260920-e` passed all installation, GUI import/export and process cases
in the smoke subset. Existing and new blocked processes exited after 5.146 and
10.008 seconds respectively; the near-miss survived 30 seconds. The Detox
application allowlist preserved A and terminated B. Store onboarding failed
because the ordinary test tab remained selected and the store was a background
tab. The harness now explicitly selects the store tab and later returns to the
original ordinary tab. Overall acceptance remains failed for this run.

`layer3-20260920-f` exposed a welcome-dialog race: the dialog appeared before
its credential content. The content was present in a later redacted accessibility
capture. Credential assertions now wait up to the 60-second GUI deadline.
Browser permission dialogs appeared, but their confirmation actions did not
install extensions; controls are now required to be shown and sensitive
before activation. The host runner was interrupted during an environment
transition; guest evidence was recovered and the clone safely stopped. This run
is explicitly failed, not a complete host acceptance result.

`layer3-20260920-g` passed installation, recovery presentation, Health, reboot,
store onboarding for both browsers, and all four process assertions. Firefox
0.2.6 and Chrome 0.2.5 extensions produced current-session heartbeats and
preserved the original ordinary tab through the startup grace period.
The overall run failed: GTK4's chooser exposes SENSITIVE without ENABLED, so
the generic readiness check prevented import; browser restart assertions read
the old session timestamp before the daemon recorded shutdown. Chrome's
omnibox lacks EditableText, and the synchronous process wait wrapper returned
failure despite a successful root supervisor journal and passing assertions.

Corrections use SENSITIVE for action readiness, keep browsers absent across a
normal process scan before restart, type only allowlisted fixture/policy URLs
through libvirt using a US guest keyboard layout, and read an atomic completion
record written by the root process supervisor after cleanup. A development
probe on G verified actual keyboard navigation to Chrome's managed policy page.
G was then safely stopped; probes do not change its failed acceptance result.

`layer3-20260920-h` passed the process assertions with the asynchronous supervisor
handoff. It exposed a transient AT-SPI read timeout during welcome handling and
a missing Close Settings action before checking imported rules in Websites.
The host runner was interrupted by another environment transition; evidence was
recovered and H stopped. Both harness issues have been addressed for the next
run, `layer3-20260920-i`.

## Package observation requiring follow-up

The installed CI package repeatedly logs a systemd ordering cycle involving
`blockuntu-hosts.path`, `blockuntu.service`, `paths.target` and `basic.target`.
Systemd drops the `paths.target/start` job to break it (see the first two lines
of the E, G and H `journal.log` artifacts). The explicitly checked BlocKuntu
units subsequently become active, so those narrow service assertions passed;
this does not establish that the ordering warning is harmless. Run I then
demonstrated a functional failure: systemd deleted `blockuntu-hosts.path/start`
itself to break the cycle, rather than deleting `paths.target/start`.
Its original acceptance journal retains this evidence. A reporting dependency
initially masked the failing service assertion; traceback redaction now uses
only the standard library and works before AT-SPI installation.

Removed `After=blockuntu.service` from the watcher source unit. Path units have
default ordering before `paths.target`, while the daemon starts after
`basic.target`; the repair service retains its own ordering after the daemon.
Layer 2 `PKG-HARD-002` now rejects the old dependency, and a mutation regression
test verifies the rejection. Reinspection of the original CI DEB fails this
new check. The VM service check also rejects related boot-ordering cycles even
if the individually observed units eventually become active.

In a **separate diagnostic experiment**, replaced only the watcher unit in
clone I, then rebooted twice. Both boots activated the watcher and logged no
BlocKuntu ordering cycle. Evidence and the exact driver/unit are under
`Testing/results/layer3-20260920-i/ubuntu/diagnostic-unit-fix/`.
This modified installation is explicitly **not package acceptance**.

The subsequent diagnostic suite passed installation/service/recovery/GUI-health
checks and all four process assertions, but failed GUI list-name inspection and
browser onboarding. The list entries expose accessible button names rather
than text fields; corrected selectors were verified against the live GUI.
Browser confirmations require ENABLED as well as SENSITIVE, whereas GTK4
chooser buttons expose only SENSITIVE; the adapter and regression tests now
distinguish those cases. These final corrections have not yet passed a complete
smoke rerun. No newly compiled application package was created.

## Disposable VM cleanup

At the user's request, removed obsolete A, B, E, F and G Layer 3 clones and the
two retained September 17 Phase 0 diagnostic clones using ownership-guarded
cleanup. All collected evidence remains. The Ubuntu, Fedora and CachyOS bases
were preserved. H was retained temporarily as the latest stopped diagnostic
clone, then removed after I's evidence was collected. Only I remains as the
latest stopped diagnostic VM alongside the three untouched bases. Free space
increased from roughly 12 GB to 100 GB before creating I.

The replacement artifact was supplied on September 21. All 48 Python harness
regression tests pass. `layer3-20260921-a` uses the documented single entry point
and rebuilt package. Two consecutive fresh-clone passes are still required.
No new application build was started.

## Rebuilt-package validation — September 21

`layer3-20260921-a` used the rebuilt artifact. All five installation/service
cases, all four GUI transfer cases, and all four process cases passed. The
watcher was active with no BlocKuntu ordering cycle. Chrome store onboarding,
restart/policy visibility and the Detox website allowlist also passed.
The complete smoke result remained failed:

- Firefox's Add permission action returned successfully while the prompt
  remained open. The adapter now retries only the exact visible confirmation,
  with a bounded deadline, and observes its dismissal before timing heartbeat.
- Chrome's address bar autocompleted the blocked URL to the previously visited
  `/exact/blocked/child` near-miss. A direct diagnostic observed the selected
  `/child` suffix; Delete removed just that suffix, and navigation then produced
  the expected Tier 1 block page and rule name. URL typing now includes that
  Delete before Enter.

A fresh Firefox diagnostic profile installed store extension 0.2.6 and produced
an active current-session heartbeat with the corrected confirmation helper.
These probes do not change A's failed result. Browser-case failures now capture
an accessibility dump and screenshot at the point of failure.
All 49 Python harness tests pass, including confirmation dismissal/no-op
regression coverage. Fresh run `layer3-20260921-b` exercises the corrected suite.

Run B passed all installation and GUI cases, Firefox store onboarding, and all
four Chrome cases, including exact-URL blocking with its near-miss and the Detox
website allowlist. Firefox's remaining three cases failed because AT-SPI
EditableText reported success without committing address-bar navigation. The
adapter now uses native URL key input for both browsers; this latest Firefox
correction still needs live validation.

During an execution-environment transition the host runner session disappeared;
the owned B clone was subsequently observed stopped. Its last collected interim
result is marked failed/interrupted, not a complete acceptance pass. Final
process results and evidence collection for B were not verified. Run A retains
the completed passing process evidence. Two fresh complete passes remain open.

The Firefox native-key diagnostic on retained clone B passed the managed-policy
page, allowed near-miss route and exact-URL block with the expected rule name.
Evidence is in `layer3-20260921-b/ubuntu/diagnostic-firefox-navigation/` and is
explicitly separate from acceptance. Obsolete clones I, A and B were removed
through guarded cleanup; reports remain and all three base guests are preserved.
Fresh run `layer3-20260921-c` starts clean acceptance with the corrected harness.
