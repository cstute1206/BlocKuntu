# Layer 3 validation evidence — updated 2026-10-05

## 2026-10-05: Fedora/CachyOS private-window portability — passed and audited

The existing twelve native-browser private-window observations passed on both
Fedora and CachyOS, with **12/12 selected observations and 21/21 smoke prerequisites
per guest**. The [combined audit](../results/private-port-20261005/audit-final.json)
passed: **24 selected observations plus 42 smoke prerequisites**, exact historical
RPM/Arch artifacts, published store manifests, archived harness/planner sources,
distinct fresh profiles/clones, serial execution and guarded cleanup. All five
owned attempt clones and copied storage files are removed; only the unchanged,
stopped Ubuntu, Fedora and CachyOS bases remain. Earlier failures remain failed.

| Guest | Native browsers / published extensions | Accepted evidence |
| --- | --- | --- |
| Fedora 44 GNOME | Firefox 157.0 / AMO 0.2.8; Chrome 151.0.7922.71 / Web Store 0.2.6 | [Fedora D](../results/fedora-private-20261005-d-group-run/summary.json), [per-guest audit](../results/private-port-20261005/audit-fedora-d.json) |
| Rolling CachyOS KDE | Firefox 156.0.1 / AMO 0.2.8; Chromium 153.0.8010.52 / Web Store 0.2.6 | [CachyOS C](../results/cachyos-private-20261005-c-group-run/summary.json), [per-guest audit](../results/private-port-20261005/audit-cachyos-c.json) |

Both guests verify `VM-BR-004` and `VM-WAL-013`: named regular/private Firefox
block pages and allowed access; Chromium regular protection, the documented
pre-consent incognito gap, private blocking/allowed access after browser-owned
consent and an observed new browser session, disabled-incognito creation
prevention, and active-allowlist GUI locks/daemon rejection. Saved screenshots
show actual incognito block pages with the correct rule name; an extension URL
alone is insufficient. [Fedora](../results/private-port-20261005/store-fedora-d.json)
and [CachyOS](../results/private-port-20261005/store-cachyos-c.json) installed
manifests confirm extension `opfljaancedgklbpnbpjfhdbbhbfpnoc`, version `0.2.6`,
`incognito: split`, SHA-256
`27d3f09524b930a295ce2e6ea55bcafe1025d93bfc147b021bdaa9dafefddad6`.
Manual consent can be withdrawn; disabling private browsing remains recommended
for full allowlist protection. Confined/alternative browsers remain unverified.

The installed RPM `0.2.0-1.fc43` and Arch package `0.2.0-1` come from
provenance-verified package run `36269349435`, commit
`605d707b92cc5fa007674169f0e74ce6fc879c33`. Their SHA-256 values are respectively
`b6bbcba406dd373cf4158cc3f10b90fafd442eda86c71bd3f74293c9647f7bd0`
and `50ffca2a29fcc3400f66c19f46eb94447e57e0b3a7ce279b08827b97c1bbc18c`.
These are historical development artifacts, not arbitrary current product source
or a future release candidate. CachyOS used the supplied rolling baseline without
a system refresh.

Accepted CachyOS C uses archived `source-executed-c`; Fedora D uses
`source-executed-d`. The 20-file harness manifests differ only in
`scripts/vm-acceptance.py` and `fixtures/layer3/browsers.py` for the subsequent
store-navigation fix. The private-window fixture and four supplementary planner
files are identical. Each per-guest audit verified captured/current sources at
its own audit time; the final audit verifies both exact archived capsules.

Initial [local checks](../results/private-port-20261005/local-checks-a.json) passed:
110 Layer 3, 23 VM-runner and 13 saved-smoke checks (**146 total**), plus Python
compilation. Preflight verifies twelve exact guest/browser/scenario dimensions
per guest and the correct native Chromium variant. Other browser packages and
partial case groups remain unavailable. Executed adapter and planner sources
are archived under `results/private-port-20261005/source-executed-a/`.

Fedora attempt A passed eight selected observations, then stopped while
observing the browser-owned incognito toggle. The failure screenshot shows the
toggle enabled; Chrome 151 exposes it as a `toggle button` with AT-SPI `PRESSED`
(20), while the harness required `CHECKED` (4). Consent observation now accepts
`PRESSED` specifically for toggle buttons, or `CHECKED`; focused/enabled controls
and pressed checkbox/switch controls still do not establish consent. Five new
regressions passed. Attempt A remains failed with its clone/storage removed and
bases unchanged. Fresh attempt B repeated the full Fedora group before CachyOS.
Corrected source is archived in `source-executed-b`; its
[local checks](../results/private-port-20261005/local-checks-b.json) passed
**151 checks** (115 Layer 3, 23 VM-runner, 13 saved-smoke) plus compilation.

Fedora attempt B passed all twelve selected observations, including the real
Chrome incognito block page after consent/restart and disabled-incognito locks.
It then failed the following Chrome allowlist smoke prerequisite: after the
last restart, the native URL input did not commit and Chrome remained on `/free`
with its document focused. Regular-window navigation now activates the browser,
selects the address bar with native Ctrl+L, queries a visible bar and observes
its focused state before typing. Two regressions require that observation and
reject a focus acknowledgement without actual focus. B remains failed and its
clone/storage are removed. Fresh C ran CachyOS first, then the complete Fedora
group. [Source-C local checks](../results/private-port-20261005/local-checks-c.json)
passed **153 checks** (117 Layer 3, 23 VM-runner, 13 saved-smoke) and compilation;
exact source is archived in `source-executed-c`. No product code, acceptance
timer, guest clock or database override was changed.

Fedora attempt C passed four Firefox observations, then failed Chrome store
onboarding. Its captured tab list has the fixture, Google consent and an extra
Chrome welcome page; the welcome page remained active. Onboarding now navigates
explicitly to the exact published BlocKuntu store URL after native tab selection,
then returns to the first fixture tab with native Ctrl+1 and verifies its page.
The native-input allowlist admits only that exact owned store URL, with negative
checks for other listings/query variants. Two regressions cover an extra launch
tab and reject other browser families. C remains failed with clean clone removal.
Fresh Fedora D uses archived `source-executed-d`; its
[local checks](../results/private-port-20261005/local-checks-d.json) passed
**155 checks** (119 Layer 3, 23 VM-runner, 13 saved-smoke) and compilation.
CachyOS C retains its accepted source-C evidence; later onboarding changes are
not retroactively attributed to that run.

This completes the separately authorized bounded portability follow-up to the
accepted Ubuntu group below. The **agreed Layer 3 milestone remains complete
for now**. The routine release gate stays at 21 smoke observations per guest;
private groups are targeted integration/selected release-candidate work.
Destructive Layer 4 work is not part of this batch.

## 2026-10-05: published Chrome 0.2.6 private-window retest — passed and audited

Fresh [ubuntu-private-20261005-e](../results/ubuntu-private-20261005-e-group-run/summary.json)
passed **12/12 selected observations and 21/21 smoke prerequisites** on Ubuntu
26.04.1. The [audit](../results/ubuntu-private-20261005/audit-e.json) passed:
exact package and captured harness provenance, installed store manifest,
complete selected/smoke evidence, unchanged bases and guarded clone cleanup.
All earlier owned failed clones/storage are removed; only the unchanged,
stopped Ubuntu, Fedora and CachyOS bases remain. Failed attempts remain failed.

| Browser | Installed version / published extension | Selected behavior |
| --- | --- | --- |
| Native Firefox | 157.0 / AMO 0.2.8 | Regular/private allowlist block pages with the correct rule name and permitted URL access. |
| Native Chrome | 154.0.8037.57 / Web Store 0.2.6 | Regular protection, documented pre-consent incognito gap, private block page and permitted URL after browser-owned consent/restart, disabled-incognito creation prevention, and active-allowlist GUI/daemon setting locks. |

The two selected IDs are `VM-BR-004` and `VM-WAL-013`. The
[captured installed manifest](../results/ubuntu-private-20261005/store-install-e.json)
confirms Chrome extension `opfljaancedgklbpnbpjfhdbbhbfpnoc`, version `0.2.6`,
`incognito: split` and SHA-256
`27d3f09524b930a295ce2e6ea55bcafe1025d93bfc147b021bdaa9dafefddad6`.
Screenshots show the actual private block page and named rule; an extension URL
alone is insufficient. The daemon rejects private-setting changes while the
website allowlist is active, and GUI mode/scope controls are locked.

The installed DEB is `0.2.0-1` from provenance-verified package run
`36269349435`, commit `605d707b92cc5fa007674169f0e74ce6fc879c33`, SHA-256
`fc1d5cc00d07e3cd5e540a2626af91a60a62042eab4070ae03d85f5049e4e48a`.
Captured harness hashes match archived `source-executed-e` and current harness
files at audit time. Browser profiles are fresh and store-installed.

This closes the **agreed Layer 3 development milestone for now**. The routine
21-observation release gate remains unchanged; releases still need exact
candidate artifacts. This run establishes Ubuntu/native-browser private coverage;
the subsequent Fedora/CachyOS passes are recorded above.
Manual incognito protection requires browser-owned consent, which users can
withdraw; disabling private browsing remains recommended for full allowlist
protection. Full catalog/matrix, other private-browser packages, destructive
recovery and selected soaks are not established by this result. Layer 4 remains
separate, except for its already accepted focused Tier 1 website-edit journey.

### Retest harness corrections

All four failed attempts remain failed, with saved evidence and their exact
executed source capsules under `results/ubuntu-private-20261005/`. Guarded
cleanup removed each disposable clone and copied storage; bases are unchanged.

- Attempt A passed four Firefox observations, then stopped at store onboarding:
  AT-SPI reported successful Google consent activation while the consent page
  remained visible. Native focus/Enter now requires the actual store transition.
- Attempt B passed eight selected observations. Its private block page rendered
  correctly, but subsequent allowed-URL input targeted the regular address bar.
  Navigation now scopes the bar to the asserted window and requires that window
  active and its bar focused before sending native input.
- Attempt C passed ten observations, including Chrome's private named block page
  and allowed-site access after consent/restart. It stopped immediately after
  natural expiry because the GUI had not refreshed its disabled mode selector.
  Mode changes now wait for both daemon permission and a freshly queried enabled
  GUI control, without overriding either lock.
- Attempt D again passed ten observations, then stopped after checking the GUI
  lock in disabled mode. The GUI remained in front of Chrome; address-bar focus
  alone did not raise the browser window. Navigation now raises the browser when
  the target window is inactive, retaining the exact target-window focus check.

Fresh attempt E reran and passed the entire group. Its
[local checks](../results/ubuntu-private-20261005/local-checks-e.json) passed
**145 checks**: 109 Layer 3, 23 VM-runner and 13 saved-smoke checks, including
five store-consent, four window-navigation and three settings-readiness
regressions. Python compilation passed. These are harness corrections; no
product implementation, acceptance timer, guest clock or database was changed.

## 2026-10-03–04: bounded Fedora/CachyOS expansion — passed and audited

Ported the existing GUI-editing, helper/two-account and Tier 1 website-edit
journeys to Fedora and CachyOS. All six selected journeys passed on fresh
clones: **19 selected observations plus 42 smoke prerequisites per guest**.
The [combined audit](../results/desktop-port-20261003/audit.json) passed with
**38 selected observations and 84 smoke prerequisites**; every owned clone and
copied storage file is removed, including failed attempts. Only the three
unchanged, stopped base VMs remain.

| Guest | Journey | Selected | Smoke prerequisites | Accepted evidence |
| --- | --- | --- | --- | --- |
| Fedora | GUI protected edits | 12/12 | 21/21 | [fedora-gui-20261003-e](../results/fedora-gui-20261003-e-group-run/summary.json) |
| Fedora | Helpers/two ordinary accounts | 2/2 | 21/21 | [fedora-helper-20261003-a](../results/fedora-helper-20261003-a-group-run/summary.json) |
| Fedora | Tier 1 website edit key | 5/5 | None required | [fedora-tier1-20261003-e](../results/fedora-tier1-20261003-e/fedora/acceptance/result.json) |
| CachyOS | GUI protected edits | 12/12 | 21/21 | [cachyos-gui-20261003-b](../results/cachyos-gui-20261003-b-group-run/summary.json) |
| CachyOS | Helpers/two ordinary accounts | 2/2 | 21/21 | [cachyos-helper-20261003-a](../results/cachyos-helper-20261003-a-group-run/summary.json) |
| CachyOS | Tier 1 website edit key | 5/5 | None required | [cachyos-tier1-20261003-a](../results/cachyos-tier1-20261003-a/cachyos/acceptance/result.json) |

The exact native RPM and Arch artifacts come from the provenance-verified
[run-36269349435 inputs](../results/package-ci/github-36269349435/verified-inputs.json),
source commit `605d707b92cc5fa007674169f0e74ce6fc879c33`. Fedora's RPM is
`0.2.0-1.fc43`, installed on the actual Fedora 44 GNOME guest; CachyOS uses
`0.2.0-1` on its supplied rolling KDE baseline without a system refresh.
These are historical development artifacts, not acceptance of arbitrary
current application source or a future release candidate.

The audit checks strict selected/smoke evidence, package hashes and installed
versions, clean Phase 0 infrastructure, distinct serial clones, base integrity,
cleanup and absence of credential values from saved text evidence. Captured
harness files are verified against archived source capsules. The two accepted
GUI groups match the then-current `source-executed-h`; the earlier helper and Tier 1
runs use their recorded older harness versions. The audit explicitly lists
those differences rather than claiming all six used the current harness.
Pre-expansion sources and all failed attempts remain preserved.

[Local harness checks](../results/desktop-port-20261003/local-checks-h.json)
passed: 97 Layer 3, 23 VM-runner, 13 saved-smoke, 7 protected-edit contract,
3 Tier 1 runner and 1 HTTP fixture check (**144 total**). The archived current
harness hashes match and its Python files compile.

Coverage remains bounded: GUI edits use Tier 2 website/application blocklists
and allowlists under Detox; helper checks cover two ordinary UIDs and the
curated control path, not a complete second-account desktop/browser login.
Tier 1 covers website edits, permitted `all_time` access, active Tier 2/3
website-blocklist boundaries and natural five-minute expiry. Application
edit-key and other protected-window variants remain unverified. No acceptance
timers were shortened and no guest clock or database overrides were used.
This expansion leaves the routine 21-observation release gate unchanged.

Private-window and broader matcher/lifecycle adapters remain Ubuntu-only.
Fresh Fedora store onboarding still reports Firefox 0.2.8 and Chrome 0.2.5.
At the time, Chrome 0.2.6 approval and installed-store private-window verification
were pending; this portability batch does not establish that fix. Broader destructive recovery,
uninstall and catalog permutations remain deferred or selected when affected.

### Portability harness development history

The initial port passed 133 local checks. Subsequent failures below identified
native-UI/input synchronization issues; their fixes added focused regressions.
Partial passing observations from failed attempts do not establish acceptance.

The first Fedora GUI attempt, `fedora-gui-20261003-a`, was interrupted while
its selected checks were running. Recovery retained three passing partial
observations and previous-boot journals under `acceptance/interrupted-recovery`;
they do not establish acceptance. After restoring host read access to its
copied disk/NVRAM, guarded cleanup removed the clone with the immutable bases
unchanged. The host/group/batch records retain this attempt as failed. Fresh
`fedora-gui-20261003-b` started after cleanup; remaining journeys run serially.

Attempt B recorded nine passing GUI observations, then failed before the
application-allowlist journey: the preceding rule editor left the **New
application** button above the viewport. Its saved accessibility tree contains
the enabled button without `SHOWING`, and the failure screenshot confirms the
scroll position. Creation now uses the existing native-focus helper to bring
the button into view before activation. Fourteen focused desktop harness checks
passed. Attempt B remains failed; guarded cleanup removed its clone with bases
unchanged. The updated source is archived in `source-executed-b`; fresh attempt
`fedora-gui-20261003-c` starts the whole GUI group again.

Attempt C passed the same nine observations and successfully created the next
application allowlist. Its **Show identities** action expanded the UI, but
AT-SPI reported a dispatch timeout. Both the screenshot and accessibility tree
show **Hide identities** and **Identity** after the action. The harness now
accepts only this dispatch-timeout message and requires those resulting controls
within the existing deadline. Three regression checks reject missing transitions
and unrelated action errors; the fourteen focused desktop checks also passed.
Attempt C remains failed and its clone was guarded-cleaned. Updated source is
archived in `source-executed-c`. The serial queue prioritizes the untouched
Fedora/helper/Tier 1 and CachyOS journeys before the next full Fedora GUI retry.

The [Fedora helper group](../results/fedora-helper-20261003-a-group-run/summary.json)
passed **2/2 selected observations and 21/21 smoke prerequisites**. Its saved
bundle passes the strict verifier and its owner record confirms copied storage
and clone removal. At that point, the Tier 1 journey and remaining portability
groups were still pending.

`fedora-tier1-20261003-a` failed at the first no-key observation: the GUI frame
was visible but not active, and the focus check timed out. The ordinary account
was UID 1000; this was not an account-policy bypass. Its clone was removed with
bases unchanged. Focus checks now requery accessible frames after modal changes
and use bounded native app switching, still requiring an active target frame.
The Tier 1 adapter also observes welcome dismissal before proceeding and can
save a no-key focus-failure screenshot only after that credential-bearing dialog
has closed. Five native-UI regressions, seven protected-edit contract checks and
two runner checks passed. Source is archived in `source-executed-d`; the queue
runs the short Tier 1 journeys before the remaining browser-dependent groups.

The [CachyOS Tier 1 journey](../results/cachyos-tier1-20261003-a/cachyos/acceptance/result.json)
passed **5/5 selected observations** on the exact Arch package, including native
key entry/save and natural expiry. The strict evidence contract confirms the
CachyOS guest identity, and the runner verified unchanged bases and clone removal.

`fedora-tier1-20261003-b` again stopped at the no-key focus check, with clean
clone removal. The new screenshot identifies the actual shell state: BlocKuntu
was visible inside GNOME overview. Native Alt+Tab did not leave that overview.
The focus helper now sends Escape before native switching and still requires
the target frame to become active. Six UI regressions, seven contract checks
and two runner checks passed. Source is retained in `source-executed-e`, and
fresh `fedora-tier1-20261003-c` retries the complete journey.

Attempt C passed focus, no-key and wrong-key rejection, then failed valid-key
submission. Its masked Tier 1 input contains 69 characters; the verified RPM
scriptlet generates a 74-character credential. The native-input driver now
uses slower credential keystrokes and reports only the expected character count
in its acknowledgement. The guest requires that masked count before submitting
and records only lengths, never the credential. Three runner checks and seven
contract checks passed; updated source is in `source-executed-f`. Attempt C
remains failed with its clone removed; `fedora-tier1-20261003-d` retries.

Attempt D stopped earlier on another WebKit AT-SPI dispatch timeout while
selecting a rule. Its clone was removed. The shared button helper now focuses
and reveals an enabled button, then activates it with native Enter; each caller
still verifies the resulting UI/policy state. Disabled/unfocused controls cannot
send that input. Nine native-UI regressions, seven contract checks and three
runner checks passed. Source is in `source-executed-g`; the complete Fedora
Tier 1 journey retries as `fedora-tier1-20261003-e`.

The [Fedora Tier 1 journey](../results/fedora-tier1-20261003-e/fedora/acceptance/result.json)
passed **5/5 observations**, including natural expiry, with strict evidence
verification and guarded clone removal/base integrity. Masked-input lengths
matched the host's private input lengths: 28 for the invalid fixture and 74 for
the installer key. Earlier failed attempts remain failed and preserved.

The [CachyOS helper group](../results/cachyos-helper-20261003-a-group-run/summary.json)
passed **2/2 selected observations and 21/21 smoke prerequisites**. The strict
bundle verifier passed and guarded cleanup removed its clone and storage.

`cachyos-gui-20261003-a` passed six website observations, then failed while
adding an application matcher with an empty transient accessibility result.
The failure screenshot shows the default Command dropdown open and the new
empty row present. The chooser now leaves an already-selected value unchanged;
tightening waits for the appended row, its selector and any replacement input
before editing. Ten UI regressions and fourteen desktop checks passed. The
failed clone was removed; source is in `source-executed-h`. Fresh CachyOS GUI
attempt B precedes the remaining full Fedora GUI retry.

The [CachyOS GUI group](../results/cachyos-gui-20261003-b-group-run/summary.json)
passed **12/12 selected observations and 21/21 smoke prerequisites** before
the host restart. Its strict bundle verifier passes; the clone and copied
storage are removed, with immutable bases unchanged. All three selected CachyOS
journeys are now accepted.

`fedora-gui-20261003-d` failed DHCP discovery before package installation or
application observations. Its network diagnostics and failed records remain
preserved. On 2026-10-04, guarded cleanup removed the stopped clone and verified
base integrity; fresh full GUI attempt `fedora-gui-20261003-e` used the same
archived harness and passed 12 selected observations plus 21 smoke prerequisites.
No host VPN/firewall settings were changed. The combined portability audit then
passed for all six journeys.

Pre-expansion source copies and the new executed-source capsule are retained
under `results/desktop-port-20261003/`. Older Ubuntu audits prove their recorded
harness versions; this portability work changes shared harness files and does
not retroactively claim those older runs used the current source.

## 2026-10-03: Ubuntu helper and Tier 1 journeys — passed and audited

The GUI group `ubuntu-gui-20261002-i` has accepted evidence: 12 selected checks
and 21 smoke observations. The old helper run `ubuntu-helper-20261002-a` was
interrupted before its selected checks; recovered checkpoints and previous-boot
journals are retained under its `acceptance/interrupted-recovery`. Its host/group
records are explicitly failed, and guarded cleanup removed the clone with bases
unchanged. Fresh [helper run](../results/ubuntu-helper-20261003-b-group-run/summary.json)
passed **2/2 selected observations plus 21/21 smoke observations**. It checked
the same fixture binaries under ordinary UIDs 1000 and 1001: rejected helpers
terminated while permitted parents remained alive, the GUI/RPC/native control
path remained available, and helpers ran again after natural Detox expiry.
This does not claim a full second-account graphical login/browser-profile journey.

The [combined GUI/helper audit](../results/ubuntu-desktop-20261002-audit.json)
passed: **14 selected observations and 42 smoke prerequisites**, distinct clean
clones, exact artifact provenance, guarded clone removal and unchanged immutable
bases. It discloses the accepted GUI run's sole unused private-adapter source
difference and checks its archived original against the captured hash. Earlier
failed/interrupted attempts remain retained as failures.

The independent Ubuntu journey implements `DES-PROTECT-004–007` plus an
initial no-key check. It covers native GUI invalid/valid key input, a confirmed
Tier 1 website name/pattern save, active Tier 2/3 website-blocklist boundaries
under Detox and natural five-minute expiry. It uses the exact run-36269349435 DEB and the ordinary desktop account,
without browser-store onboarding or clock/database overrides. Eight local harness
checks passed, including the guest-completion race.
See the [runbook](../phase-3.md#ubuntu-tier-1-edit-key-journey).

Attempt `ubuntu-tier1-20261003-a` recorded five passing guest observations,
including natural expiry, but the host runner incorrectly failed when the guest
published its final checkpoint between a result read and the service-exit check.
The complete guest result and failed host result are retained; the clone was
removed with unchanged bases. This remains a failed host attempt. The runner now
rereads the atomic checkpoint after service exit, with regressions that still
reject incomplete results. Fresh [run `ubuntu-tier1-20261003-b`](../results/ubuntu-tier1-20261003-b/ubuntu/acceptance/result.json)
passed **5/5 selected observations**. Its ordinary account was UID 1000; the
installer key granted 299 remaining seconds when checked. The GUI confirmed and
saved both the name and exact-URL pattern change. Tier 2 and Tier 3 edits were
rejected while their actual Detox sessions and the Tier 1 grant were active.
After natural expiry, the GUI disabled the saved fields and the daemon rejected
the later edit without changing policy. The expiry observation was at
`11:19:16 UTC`, after the grant ended at `11:19:09 UTC`.

The [Tier 1 audit](../results/ubuntu-tier1-20261003-b-audit.json) verifies complete
selected evidence, exact package provenance/version, captured current harness
hashes, clean Phase 0 infrastructure, clone/storage removal, unchanged immutable
bases and absence of credential values from saved text evidence. Only the three
stopped base VMs remain. This is selected website-edit acceptance, not full Layer
3/4, application edit-key or Sunday/operator-window variant coverage.

At that point, Chrome extension 0.2.6 had been uploaded for review; approval,
installed-store confirmation and a fresh private-window group were pending.
The 2026-10-05 result above establishes that retest. The previous 0.2.5 group
remains failed. Broader recovery and uninstall remain deferred.

## 2026-10-02: Ubuntu GUI, private browsing and helpers — development history

Implemented the bounded [desktop groups](../phase-3.md#ubuntu-desktop-commands):
12 GUI observations (website/application append/remove restrictions and four
protected mode changes), 12 native Firefox/Chrome private-window observations,
and two helper/control-path observations. GUI saves and browser-owned consent
use native accessibility/keyboard actions; daemon rejection uses ordinary-user
RPC. Session services are explicitly curated; BlocKuntu components and required
descendants are omitted from the helper safety list to observe built-in exemptions.
Deliberate desktop degradation and destructive recovery stay in Layer 4.
The acceptance guest reports Ubuntu 26.04.1 LTS; the exact run-36269349435 DEB
was previously built and statically checked in Ubuntu 24.04 CI.

The local Layer 3 suite passed 84 tests and the VM-runner suite passed 23 tests;
13 saved-smoke checks and the HTTP fixture test also passed (121 total).
All three selections passed artifact preflight. Attempt `ubuntu-gui-20261002-a`
stopped before clone creation because the host had moved to kernel `7.0.0-38`.
The preparation script extracted that already installed kernel's package into
the workspace. Attempt `ubuntu-gui-20261002-b` was cancelled before new GUI checks
after review found unserved fixture routes. Original running records are retained;
the group is explicitly failed, its owned clone was removed and bases were
verified unchanged. The corrected fixture uses existing deterministic routes.
Attempt `ubuntu-gui-20261002-c` was also cancelled before new GUI checks after
read-only inspection of the installed editor identified dynamic accessible names
and an open Settings modal. Its layout dump and original records are retained;
the stopped clone was removed under the ownership guard and bases remained
unchanged. The adapter now matches labels with their current values and verifies
native window focus. Attempt `ubuntu-gui-20261002-d` was cancelled before new
GUI checks to add an explicit refresh/reselection after inactive safety curation.
Before activation, the GUI draft must contain every saved session entry; after
removing the fixture target, unrelated entries must remain unchanged. Its original
records and guarded cleanup are retained as failed development evidence.
Attempt `ubuntu-gui-20261002-e` reached the new editor, then failed before selected
observations: WebKit's HTML inputs expose Text/Component, not EditableText.
Diagnostic probes on that failed clone verified native fixture-only typing and
all four inactive GUI creation flows. They also identified the allowlist's
collapsed, initially empty identity list and native select focus behavior.
These probes are not acceptance passes. The failed clone and copied storage
were removed under the ownership guard; bases remained unchanged.
Attempt `ubuntu-gui-20261002-f` was interrupted by a host reboot before the new
GUI observations. Its host/group records are retained in `interrupted-recovery`,
alongside recovered guest checkpoints and the previous guest boot's journal.
The final status is failed, not an acceptance pass. Access to its copied disk was
restored through system authentication, guarded cleanup removed the clone and
storage, and immutable bases were verified unchanged. Private-window assertions
now read only the newly created private window, so a regular-window block page
cannot satisfy the private check.
Attempt `ubuntu-gui-20261002-g` recorded 9/12 passing GUI observations, then its
application allowlist terminated the unlisted regular-user session manager,
PAM helper and SSH agent. The ordinary-user inventory could not read all
non-dumpable executable links and incorrectly used proc-directory ownership as
the account identity. The system daemon remained active; logout did not disable
enforcement. Partial checkpoints and enforcement/root-inventory diagnostics are
retained; this is a failed group, not acceptance. Guarded cleanup removed its
clone and bases stayed unchanged. Root-readable metadata now uses `/proc/status`
real UIDs; all policy changes remain ordinary-user operations. The helper journey
also checks the same fixture binaries under a second ordinary account, without
claiming a complete GUI login/browser-profile switching journey.
Attempt `ubuntu-gui-20261002-h` again recorded 9/12 passing observations, then
failed before application-allowlist activation: the adapter read a transient
empty accessibility tree after changing the matcher type. Its traceback and
control dump are retained; the clone was removed and bases remained unchanged.
The adapter now waits for each newly rendered input row.
The fresh [GUI group](../results/ubuntu-gui-20261002-i-group-run/summary.json)
passed **12/12 selected observations plus 21/21 smoke prerequisites**, finishing
on 2026-10-03 local time. The application allowlist terminated the target while
preserving all unrelated session identities and keeping the GUI available.
Group verification, guarded clone removal and immutable-base integrity passed.
At that point, private-window and helper groups were still pending; the final
batch audit had not passed yet. Current status is recorded above.

Private attempt `ubuntu-private-20261002-a` recorded four passing Firefox
observations, then stopped before Chrome's selected observations because its
accessibility tree also exposes unnamed auxiliary frames. The control dump and
partial checkpoints are retained as failed development evidence; guarded cleanup
removed the clone and bases stayed unchanged. The adapter now counts named
browser frames with visible web documents, while still reading only the new
private window for blocking assertions. Attempt `ubuntu-private-20261002-b`
recorded 8/12 passing observations, including Chrome's documented pre-consent
incognito gap, then failed to locate the consent toggle: Chrome appends warning
text to its accessible name. The retained dump also explicitly requires a
browser restart for this preference. The adapter now matches the label prefix,
observes the checked state, and performs a real restart before the protected
private-window check. The failed clone was removed with bases unchanged;
`ubuntu-private-20261002-c` is the fresh retry.
These private-only corrections do not execute in the accepted GUI group;
its captured old adapter source is archived against its original manifest hash.
The final audit must disclose that unused-file difference, rather than claim
identical manifests or rerun unaffected GUI journeys.

Attempt `ubuntu-private-20261002-c` passed eight observations, then exposed a
product failure after actual consent and a real Chrome restart. The rejected
incognito navigation redirected to the expected rule's extension block-page URL,
but Chrome rendered `ERR_BLOCKED_BY_CLIENT` instead of BlocKuntu's named page.
The original control dump and screenshot are retained; this is a failed group,
not a private-window pass. Its clone was removed and bases remained unchanged.
Chrome 154.0.8037.57 had installed Web Store extension 0.2.5. A read-only download
of that store package confirms the absent `incognito` key and therefore default
spanning mode; its manifest, CRX and checksum are retained in
[diagnostics](../results/chrome-private-20261003-diagnostic/provenance.json).
[Chrome's documented constraint](https://developer.chrome.com/docs/extensions/reference/manifest/incognito)
explains why spanning mode cannot load extension pages into incognito tabs.
Source 0.2.6 now declares split mode and the packaging check requires it.
Source checks and an upload ZIP are separate from installed-store acceptance:
approval and the fresh Ubuntu private-group rerun remain required. The user
has uploaded the 0.2.6 ZIP; this does not establish installed-store acceptance.
The interrupted helper run was recovered; its fresh retry is recorded above.

## 2026-10-01: bounded Ubuntu lifecycle work — passed; audited 2026-10-02

The [new policy](../phase-3-roadmap.md) replaces the mandatory full-matrix
expansion with a small release gate and selected Ubuntu integration journeys.
The package remains the exact provenance-verified run `36269349435` DEB.

Implemented selections: schedule and natural Detox transitions for websites and
processes (`VM-ACT-001/002`), website allowance exhaustion (`VM-WEB-008`),
one-browser website allowlist unlock/expiry (`VM-WAL-012`), and an isolated
application allowed-only accounting case (`VM-AAL-010`) with supplemental
background-process exhaustion/unlock/expiry. Ubuntu smoke now verifies policy
created before reboot using actual process and visible browser enforcement.
Commands and scope limits are in [phase-3.md](../phase-3.md).

Attempt `ubuntu-life-20261001-a` failed before clone creation because the upgraded
host lacked a readable guestfs appliance kernel copy. The preparation script
extracted the installed kernel package into the workspace; it did not update
the host kernel or change boot configuration.

Attempt `ubuntu-life-20261001-b-ubuntu` was interrupted before host finalization.
[Recovered guest checkpoints](../results/ubuntu-life-20261001-b-ubuntu/ubuntu/acceptance/interrupted-recovery/)
contain all six passing lifecycle observations (Firefox and Chrome), the passing
guest smoke result and complete process checkpoint. These are diagnostic evidence,
not a finalized acceptance pass. The stopped clone's disk ownership was restored
through system authentication, evidence recovered, and the ownership-guarded
cleanup removed its copied storage. Immutable bases were verified unchanged.
The original running host/group records are retained beside the recovery files;
the final host/group status explicitly records the interruption as failure.

The [fresh lifecycle group](../results/ubuntu-life-20261001-c-group-run/summary.json)
passed **6/6 selected observations plus 21/21 smoke prerequisites**. Firefox and
Chrome each verified schedule activation/expiry, Detox activation/expiry, and
one-minute website allowance exhaustion. Schedule cases took 329.8/323.7 seconds;
Detox cases 251.4/251.4 seconds; allowance cases 74.3/74.0 seconds. Visible block
pages identified the expected rule; process assertions retained original and
relaunched identities. Post-reboot process enforcement and both persisted-rule
browser checks passed within the smoke prerequisite. The group verifier passed,
the copied clone/storage were removed and immutable base integrity passed.

The [isolated application group](../results/ubuntu-app-20261001-a-group-run/summary.json)
passed `VM-AAL-010` and its supplemental background-process lifecycle in 230.5
seconds. Allowed-only activity left all 60 seconds available; rejected activity
exhausted the allowance in 70.2 seconds and the process terminated. A normal
120-second unlock kept a fresh process alive, then it terminated within 5.1
seconds of expiry while Detox remained active. The allowed control process
survived and the allowance stayed at zero. Process identities, the grant and
read-only usage observations are retained in the
[checkpoint](../results/ubuntu-app-20261001-a-ubuntu/ubuntu/acceptance/application-lifecycle.json).
The group verifier, base-integrity check and guarded cleanup passed.

The [Firefox unlock group](../results/ubuntu-unlock-20261001-a-group-run/summary.json)
passed `VM-WAL-012` in 309.8 seconds plus **21/21 smoke prerequisites**.
Allowed-only browsing stayed uncharged; outside-site browsing exhausted the
allowance in 69.5 seconds. The normal 120-second grant permitted access and
the expected named block page returned after expiry while Detox remained active,
with no allowance refill. Its
[checkpoint](../results/ubuntu-unlock-20261001-a-ubuntu/ubuntu/acceptance/p32-result.json)
and reblocking screenshot retain the actual observations. Chrome completed its
smoke prerequisites but was not selected for the unlock journey.

The [final audit](../results/ubuntu-priority-20261001-audit.json), run on
2026-10-02, revalidated **8/8 selected observations across three fresh clones**
and the two complete smoke prerequisites against the current executable harness.
All three manifests match one another and current source; each bundle retains
the exact DEB provenance, base-integrity and removed-clone evidence. The live
inventory contained only the three stopped Ubuntu/Fedora/CachyOS templates.
The batch completed during the interruption; no further VM restart was needed.
Selected groups were rechecked with `layer3_group_runner.verify_bundle` plus
live Phase 0/base checks; the existing `layer3-audit-groups.py` CLI accepts only
smoke-group summaries and is not the auditor for this selected batch.

This completed the representative first-three Ubuntu priorities. At this
checkpoint, Chrome unlock had an executable selection without an accepted
result; GUI authoring, protected edits, helper safety and private-window journeys
were follow-up targets. The later desktop batch is recorded above. Full-matrix
acceptance is not claimed.

Local checks at this checkpoint: 70 Layer 3 tests, 23 VM-runner tests,
13 saved-smoke checks and the HTTP fixture test passed (107 total). Python
compilation and diff whitespace checks on files changed in this task passed.
A broader package-test run exposed host-environment issues: the Arch fixture
passes with `umask 022`, while the available RPM archive reader rejects the
locally generated RPM. These are not Ubuntu VM acceptance failures; the broader
package suite is not reported as passing. Existing whitespace in `VM-Info.md`
was outside this task and left unchanged.

## P3.2 Ubuntu domain and covered-subdomain group

The [selected domain group](../results/p32domain-36269349435-a-group-run/summary.json)
passed `VM-WEB-001` in native Firefox and Chrome (2/2 observations), plus
21/21 smoke prerequisites, on the exact provenance-verified run-`36269349435`
Ubuntu DEB. A disposable clone's local `dnsmasq` answered fixture names and
forwarded other queries to its original libvirt DNS server. The
[DNS checkpoint](../results/p32domain-36269349435-a-ubuntu/ubuntu/acceptance/domain-dns.json)
records baseline `127.0.0.1` resolution for the browser-specific domain,
`www` subdomain, and near miss, as well as a successful public query. After
each Tier 1 rule, the managed hosts block resolved its domain and subdomain
to `0.0.0.0` while leaving the near miss at `127.0.0.1`; the daemon reported
the managed block compliant. Firefox and Chrome showed the BlocKuntu page
and expected rule name for both blocked hosts, then loaded the near miss.
The group verifier found no errors, and its package, screenshots, timings,
base integrity, and guarded clone removal passed. A live inventory again
showed only the three stopped templates. Together with the matcher group
below and the `VM-WEB-002` smoke regression, Ubuntu native-browser evidence
now covers `VM-WEB-001–007`. Website allowlist cases and other guests remain
outside this result.
The final local harness suite passed 138 tests; the passing domain-group
manifest matches every current executable harness file. All five website
development/probe owner records preserve unchanged immutable bases.

## P3.2 Ubuntu native website matcher group

The [passing selected group](../results/p32web-36269349435-c-group-run/summary.json)
verified `VM-WEB-003–007` in native Firefox and Chrome: 10/10 P3.2
observations, plus all 21 smoke prerequisites. Each browser exercised URL
prefix, URL-containing token, and path prefix with positive and near-miss
routes; activation against an already-open tab; and an in-page
`history.pushState` transition. Positive checks required the visible
BlocKuntu blocked page and expected rule name, and the bundle retained
screenshots, case durations, browser versions, and a separate
[`p32-result.json`](../results/p32web-36269349435-c-ubuntu/ubuntu/acceptance/p32-result.json)
checkpoint. The package was the exact provenance-verified run-`36269349435`
Ubuntu DEB. The group verifier found no errors; Phase 0 identity, base
integrity, and guarded clone removal passed. A live check found only the
three stopped templates, and the immutable bases matched all three attempt
ownership records.

Two earlier attempts remain failed. [Attempt A](../results/p32web-36269349435-a-group-run/summary.json)
passed smoke and 9/10 P3.2 observations; Firefox's AT-SPI button action did
not move the SPA route, so keyboard activation replaced it. [Attempt B](../results/p32web-36269349435-b-group-run/summary.json)
passed Firefox 5/5, then Chrome onboarding timed out while the fixture tab
remained selected and the store consent tab was open; explicit second-tab
keyboard selection resolved this in the fresh passing attempt. Both failed
bundles and screenshots remain, and their stopped clones were removed under
the ownership guard. The matcher runs alone do not establish `VM-WEB-001`, the
website allowlist/schedule cases, other distributions, or complete core acceptance.
At that checkpoint the local harness suite passed 134 tests; the passing VM manifest still
matches every current executable harness file.

## P3.1 two-case isolated Ubuntu batch

The [serialized group result](../results/p31more-36269349435-a-group-run/summary.json)
passed 2/2 selected core observations using separate fresh Ubuntu clones and
the exact run-`36269349435` DEB. In `VM-WAL-005`, a Tier 2 website allowlist
activated through Detox rejected the outside URL for that allowlist, permitted
the listed URL, and left the managed `/etc/hosts` block unchanged and compliant.
In `VM-AAL-001`, the daemon rejected a Tier 1 application allowlist from the
ordinary desktop user; the policy snapshot and active enforcement were unchanged.
Both result bundles verified package provenance, Phase 0 identity, base
integrity, and clone removal. No copied storage remains; live libvirt inventory
showed only the three stopped templates. The batched preflight and focused
Layer 3 suite passed 54 tests before the VM run. These two selected cases do
not establish a complete core-suite or cross-distribution pass.

Because the shared fixture changed, `VM-WAL-001` was also rerun on a separate
fresh Ubuntu clone with the [current harness](../results/p31wal-current-36269349435-group-run/summary.json).
Its selected observation passed, its clone was removed, and the three current
harness manifests match their source files. Libvirt again showed only the
three stopped templates.

The final script-test pass completed 127 tests; the three saved acceptance
manifests still match the current executable harness files.

## P3.1 first isolated Ubuntu scenario

The [selected core group](../results/p31wal-36269349435-a-group-run/summary.json)
ran `VM-WAL-001` on a fresh Ubuntu clone with the exact provenance-verified
run-`36269349435` DEB (`0.2.0-1`, SHA-256
`fc1d5cc00d07e3cd5e540a2626af91a60a62042eab4070ae03d85f5049e4e48a`).
Its [guest checkpoint](../results/p31wal-36269349435-a-ubuntu/ubuntu/acceptance/scenario-result.json)
records a daemon rejection of the unsupported Tier 1 website allowlist,
unchanged policy, active enforcement, and a 0.224-second assertion. The group
verifier passed 1/1 exact observations and checked Phase 0 identity, package
provenance, base integrity, and removed clone ownership. A subsequent live
`virsh list --all --name` showed only the Ubuntu, Fedora, and CachyOS
templates. This one selected Ubuntu case is not a core-suite pass or a Fedora/
CachyOS result; 36 browser cells still need adapters. The local Layer 3 test
suite passed 50 tests before the VM run.

## P3.1 batched harness checkpoint

This batch added atomic process-case checkpoints and recovery of completed
rows when a supervisor has no terminal completion file, strict validation of
the guest-to-host UI action protocol, bounded planning budgets for implemented
isolated observations, and a reusable group audit. One script-test pass after
the batch completed **120 tests**. The executable harness was then checked in
one serialized three-guest VM regression using the exact run-`36269349435`
packages; it was not rerun after each edit.

| Guest / latest successful run | Required observations | Process checkpoint | Clone |
| --- | ---: | --- | --- |
| [Ubuntu](../results/p31e-36269349435-ubuntu/ubuntu/acceptance/result.json) | 21 passed | 4/4, matches completion | Removed |
| [Fedora](../results/p31e-36269349435-fedora/fedora/acceptance/result.json) | 21 passed | 4/4, matches completion | Removed |
| [CachyOS](../results/p31f-36269349435-cachyos/cachyos/acceptance/result.json) | 21 passed | 4/4, matches completion | Removed |

The first [group run](../results/p31e-36269349435-group-run/summary.json)
failed at CachyOS `VM-BR-001`: the saved Chromium screenshot shows Google's
consent endpoint returning HTTP 502 during published-store onboarding.
Dependent observations were blocked. Its stopped disposable clone was removed
through guarded cleanup; the failed bundle remains. The single fresh
[CachyOS retry](../results/p31f-36269349435-group-run/summary.json) passed.
The new [live audit](../results/p31ef-group-audit.json) rechecked all 63
required smoke observations, exact package provenance, common harness files,
serial timing, distinct clone identities, removed copied storage, unchanged
bases, and the three stopped templates. It retains the failed attempt in its
history. Full P3.1 scenario and browser-matrix execution remain open.

## P3.1 serialized smoke-group checkpoint

The new [group runner](../scripts/layer3-run-groups.py) preflighted all three
run-`36269349435` packages, then ran the known smoke group on a fresh clone per
guest. It writes a private incremental group summary, retains per-group logs,
and stops at the first failed or incomplete bundle. The first
[three-guest group run](../results/p31c-36269349435-group-run/summary.json)
passed Ubuntu and Fedora, then correctly failed CachyOS at Chromium onboarding:
the saved [failure screenshot](../results/p31c-36269349435-cachyos/cachyos/acceptance/chromium-vm-br-001-failure.png)
shows Google's consent endpoint returning HTTP 502. `VM-BR-001` failed and
dependent observations were blocked. That disposable clone was stopped and,
after diagnosis, removed by guarded cleanup; its failure evidence remains.

A single fresh [CachyOS retry](../results/p31d-36269349435-group-run/summary.json)
passed without changing the smoke executor or package. The latest successful
run for each guest is:

| Guest / run | Required observations | Minutes | Base integrity | Clone |
| --- | ---: | ---: | --- | --- |
| [Ubuntu](../results/p31c-36269349435-ubuntu/ubuntu/acceptance/result.json) | 21 passed | 15.9 | Verified | Removed |
| [Fedora](../results/p31c-36269349435-fedora/fedora/acceptance/result.json) | 21 passed | 18.5 | Verified | Removed |
| [CachyOS](../results/p31d-36269349435-cachyos/cachyos/acceptance/result.json) | 21 passed | 16.3 | Verified | Removed |

The [final live audit](../results/p31cd-runtime-regression.json) rechecked all
63 exact observations, package/version/commit and browser metadata, matching
smoke harness manifests, serial timing, distinct clone identities, removed
copied storage, unchanged immutable bases, and only three stopped template
domains in libvirt. It explicitly retains the failed CachyOS attempt in the
history. This validates serialized smoke orchestration, not the unimplemented
core, browser-matrix, or full Layer 3 suites. The 36 non-smoke browser cells
still need verified layouts and adapters.

## P3.1 browser-cell, budget, and dependency checkpoint

The revised smoke harness passed one fresh run on each guest with exact
run-`36269349435` packages. Its guest-group budget is derived from bounded
baseline scenarios (**2,520 seconds**); the separate process supervisor is
bounded to **470 seconds**. Dependency guards stop downstream GUI, browser,
and process cases after failed setup. The positive runs below prove that the
new guards preserve working behavior; focused tests exercise the failure paths.

| Guest / run | Required observations | Minutes | Base integrity | Clone |
| --- | ---: | ---: | --- | --- |
| [Ubuntu](../results/p31b-36269349435-ubuntu-a/ubuntu/acceptance/result.json) | 21 passed | 15.4 | Verified | Removed |
| [Fedora](../results/p31b-36269349435-fedora-a/fedora/acceptance/result.json) | 21 passed | 17.5 | Verified | Removed |
| [CachyOS](../results/p31b-36269349435-cachyos-a/cachyos/acceptance/result.json) | 21 passed | 16.2 | Verified | Removed |

The [final regression check](../results/p31b-runtime-regression.json) verified
63 exact required observations, identical current harness manifests, installed
package/version evidence, three distinct clone identities, removed copied
storage, unchanged bases, and only the three stopped templates in libvirt.
The [browser-cell check](../results/p31-browser-cells.json) matched six native
smoke layouts to saved browser metadata and the exact package archives. All
42 required cells are listed; 36 still have unresolved adapter details.

The [read-only group planner](../scripts/layer3-groups.py) assigns the existing
smoke sequence one fresh clone per guest. Other observations receive separate,
non-runnable clone boundaries until their scenario executors and budgets are
implemented. A full Ubuntu plan currently contains 504 required observations
and 504 non-runnable groups; this is a safety plan, not a VM execution estimate.

P3.1 remains in progress. Core/browser-matrix/full runtime execution, safe
group scheduling, and evidence for the 36 new browser cells are still open.
The remaining non-smoke cases and deferred upgrade have not been accepted as a
complete suite; the later Ubuntu `VM-WAL-001` result is one selected exception.

## P3.1 runtime integration checkpoint — modified smoke harness

The registry-derived smoke gate and ordinary-user process RPC path passed one
fresh installed-package smoke run on each agreed guest, using the exact
provenance-verified packages from run `36269349435`. These are regression runs
for the modified P3.1 harness; they do not add the planned cases or establish a
new two-pass repeatability gate for the changed harness.

| Guest / run | Required observations | Minutes | Base integrity | Clone |
| --- | ---: | ---: | --- | --- |
| [Ubuntu](../results/p31-36269349435-ubuntu-b/ubuntu/acceptance/result.json) | 21 passed | 15.2 | Verified | Removed |
| [Fedora](../results/p31-36269349435-fedora-a/fedora/acceptance/result.json) | 21 passed | 17.2 | Verified | Removed |
| [CachyOS](../results/p31-36269349435-cachyos-a/cachyos/acceptance/result.json) | 21 passed | 16.4 | Verified | Removed |

The [aggregate regression check](../results/p31-runtime-regression.json) validates
63 exact case/guest/browser-package/scenario observations, matching harness
files at that checkpoint across all three runs, package hashes, three distinct clone
identities, removed copied storage, unchanged base definitions/storage metadata,
and only stopped `Ubuntu`, `Fedora`, and `CachyOS` domains. The current runner
also rejects a full Ubuntu suite request before VM creation because 483
observations lack executable smoke bindings. A selected case is reported as a
diagnostic projection, not complete suite acceptance.

The first Ubuntu attempt `p31-36269349435-ubuntu-a` was interrupted during
template storage download before a VM was defined or started. The owned partial
copy was removed after checking its recorded path and unchanged bases;
[interruption evidence](../results/p31-36269349435-ubuntu-a/ubuntu/interruption.json)
is retained. It contributed no acceptance observation.

P3.1 remains open: new scenario executors, verified isolation, group budgets,
browser-cell adapter discovery, and expanded evidence are still needed. The
63 passing checks are the existing smoke subset; the remaining non-smoke cases and
deferred upgrade remain outside this checkpoint.

## Roadmap implementation checkpoint — new artifact run 36269349435

Status: **P3.0 complete; P3.1 coverage planning implemented**.
The selected [Layer 2 run 36269349435](https://github.com/cstute1206/BlocKuntu/actions/runs/36269349435)
completed successfully at source commit
`605d707b92cc5fa007674169f0e74ce6fc879c33`. Downloaded its Ubuntu, Fedora, and
Arch bundles and verified passed/provenance-verified reports, source commits,
and the exact package checksums. No BlocKuntu package was rebuilt.

| Guest input | Package SHA-256 |
| --- | --- |
| Ubuntu DEB `blockuntu_0.2.0-1_amd64.deb` | `fc1d5cc00d07e3cd5e540a2626af91a60a62042eab4070ae03d85f5049e4e48a` |
| Fedora RPM `blockuntu-0.2.0-1.fc43.x86_64.rpm` | `b6bbcba406dd373cf4158cc3f10b90fafd442eda86c71bd3f74293c9647f7bd0` |
| CachyOS package `blockuntu-0.2.0-1-x86_64.pkg.tar.zst` | `50ffca2a29fcc3400f66c19f46eb94447e57e0b3a7ce279b08827b97c1bbc18c` |

[Verified input records](../results/package-ci/github-36269349435/verified-inputs.json).

All six runs passed **21/21 required observations**, establishing two
consecutive comparable fresh-clone passes per guest with no failed attempts
in this series. This is 126 passed observations of the existing smoke subset.

| Guest / run | Required observations | Minutes | Base integrity | Clone |
| --- | ---: | ---: | --- | --- |
| [Ubuntu A](../results/p30-36269349435-ubuntu-a/ubuntu/acceptance/result.json) | 21 passed | 15.2 | Verified | Removed |
| [Ubuntu B](../results/p30-36269349435-ubuntu-b/ubuntu/acceptance/result.json) | 21 passed | 16.4 | Verified | Removed |
| [Fedora A](../results/p30-36269349435-fedora-a/fedora/acceptance/result.json) | 21 passed | 17.4 | Verified | Removed |
| [Fedora B](../results/p30-36269349435-fedora-b/fedora/acceptance/result.json) | 21 passed | 17.0 | Verified | Removed |
| [CachyOS A](../results/p30-36269349435-cachyos-a/cachyos/acceptance/result.json) | 21 passed | 16.1 | Verified | Removed |
| [CachyOS B](../results/p30-36269349435-cachyos-b/cachyos/acceptance/result.json) | 21 passed | 15.2 | Verified | Removed |

All six harness manifests are identical and match the executable smoke files.
Each pair has identical package/browser/extension versions and base records.
Firefox was `156.0.1` with store extension `0.2.8` on all guests. Chrome was
`150.0.7871.186` on Ubuntu and `151.0.7922.71` on Fedora; CachyOS used Chromium
`153.0.8010.52`. Chrome/Chromium store extension was `0.2.5` throughout.
The browser harness performed store onboarding without manual intervention.

The new read-only [smoke evidence checker](../scripts/check_vm_smoke.py)
verified the [three pairs](../results/p30-smoke-pairs.json). A separate
[final host verification](../results/p30-final-state.json) confirmed six distinct
clone UUIDs, machine IDs and SSH host fingerprints, removed copied storage,
unchanged base definitions/storage metadata, and unchanged package hashes.
Only the stopped `Ubuntu`, `Fedora`, and `CachyOS` bases remain in libvirt.

P3.0 is complete; full Layer 3 acceptance remains pending. Each run still
records the then-planned non-smoke cases and the deferred upgrade case.
The legacy upgrade-skip text mentions no accepted baseline; the current scope
reason is the explicit user deferral documented in the roadmap. Correcting
that legacy wording is part of runtime integration, not grounds to rewrite
historical result files.

Alongside the run, implemented the [case registry and read-only coverage
planner](../layer3/README.md), without modifying the smoke executor. It covers
all 77 specification IDs, the 30 positive browser/guest cells, and 12 negative
cells. The initial full plan expands to 1,393 required observations; only the
63 existing smoke bindings are implemented. Browser adapter discovery fields
remain explicitly unresolved. Expanded VM execution, isolation, and runtime
result-gate integration are not implemented yet.

Local checks passed: 21 acceptance-harness tests, 14 Phase 0 tests, and 17 new
contract/planner tests. The latter cover specification drift, exact dimension
bindings, missing/duplicate/conflicting results, unimplemented pass claims,
private-mode family distinctions, confined/unsupported coverage, and invalid
selections. Python compilation and documentation-link checks also passed.
Those checks were recorded with the planning checkpoint. This continuation
adds 13 passing smoke-evidence checker tests, including missing/duplicate
observations, setup/cleanup failures, package/version drift, reused identities,
overlapping runs, and retained failed-attempt history. P3.1 execution selection,
registry-based runtime gating, isolation, and expanded adapters are next.

## Previous artifact smoke validation — run 36258082653

**The requested smoke validation round passed:** two consecutive fresh CachyOS
runs, two consecutive fresh Fedora runs, and one fresh Ubuntu regression run.
All five passed 21 required observations, used Firefox store extension `0.2.8`,
verified base metadata integrity, and removed their disposable clones.
This validates the initial smoke subset, not the entire Layer 3 matrix.
Each run still records 59 broader cases as planned and upgrade testing as deferred.

| Guest / run | Required observations | Minutes | Base integrity | Clone |
| --- | ---: | ---: | --- | --- |
| [CachyOS A](../results/layer3-20260927-cachyos-a/cachyos/acceptance/result.json) | 21 passed | 15.6 | Verified | Removed |
| [CachyOS B](../results/layer3-20260927-cachyos-b/cachyos/acceptance/result.json) | 21 passed | 16.0 | Verified | Removed |
| [Fedora B](../results/layer3-20260927-fedora-b/fedora/acceptance/result.json) | 21 passed | 17.4 | Verified | Removed |
| [Fedora C](../results/layer3-20260927-fedora-c/fedora/acceptance/result.json) | 21 passed | 17.6 | Verified | Removed |
| [Ubuntu A regression](../results/layer3-20260927-ubuntu-a/ubuntu/acceptance/result.json) | 21 passed | 15.2 | Verified | Removed |

The CachyOS pair has identical harness manifests, as does the Fedora pair.
Ubuntu used the same manifest as the passing Fedora pair. The only difference
between the CachyOS and Fedora harness manifests is the later Nautilus chooser
fix in `fixtures/layer3/gui.py`; its exception is scoped to Nautilus and does
not change the KDE chooser path. See each run's `harness-files.json` and the
[aggregate verification](../results/layer3-store-20260927/smoke-summary.json).

### Published Firefox fix

Downloaded the public AMO `0.2.8` XPI and verified its SHA-256 against AMO's
metadata before running the VMs. Its compiled background script registers
`browser.runtime.onStartup.addListener`, calls `sendHeartbeat`, and revalidates
open tabs. SHA-256:
`a2ee1593de9087d5586e870738a9abe75899c2c15b8b3abc9d91f209283c9897`.
The browser cases installed the published store extension in fresh profiles;
the downloaded inspection archive was not sideloaded. All five runs confirmed
extension `0.2.8` and passed the unchanged restart-heartbeat deadline.
[Store verification](../results/layer3-store-20260927/verification.json).

Browser versions: Firefox `156.0.1` on all guests; Chromium `153.0.8010.52`
on CachyOS; Chrome `151.0.7922.71` on Fedora and `150.0.7871.186` on Ubuntu.
The Chrome/Chromium store extension was `0.2.5` throughout.

### Package provenance and Fedora correction

All five runs consumed the exact, provenance-verified native packages from
[Layer 2 run 36258082653](https://github.com/cstute1206/BlocKuntu/actions/runs/36258082653),
source commit `4b85aa97287ee8fd28433f05b2ad1142ab7ab3ff`, including PR #9's
Fedora native-messaging manifest fix. No application package was rebuilt.

| Package | SHA-256 |
| --- | --- |
| Arch `blockuntu-0.2.0-1-x86_64.pkg.tar.zst` | `c7adf3be84776ac733521986368ef4002bafda5e8d2a4250b2b9add5869096cf` |
| RPM `blockuntu-0.2.0-1.fc43.x86_64.rpm` | `1891414d82edc23c8effec8bc1d6a4ca5bc70ce43a4767dab094120e4ca3c7d6` |
| DEB `blockuntu_0.2.0-1_amd64.deb` | `3dc4578e83f2bb8798bb56e5ff353eb2796c1fc97dc4c04f9d2f9dc3e020f886` |

Initial Fedora run `layer3-20260927-fedora-a` failed because Nautilus exposed
the open native chooser as an AT-SPI `filler`, while the harness accepted only
dialog/frame/file-chooser roles. The unhandled dialog caused subsequent GUI
failures; browser failures and missing-fixture cases remain recorded in that
failed report. Added a Nautilus-only filler exception, retaining exact title
and visibility checks. A regression test rejects similarly named desktop/taskbar
entries. Fresh Fedora B and C then passed the full subset. Failed A's evidence
is retained and its clone was removed using guarded cleanup.

All 21 acceptance-harness tests and 14 Phase 0 tests pass. Final libvirt
inventory contains only stopped `Ubuntu`, `Fedora`, and `CachyOS` bases.

## Historical status — September 26

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
