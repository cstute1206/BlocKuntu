# Phase 3 roadmap: small release gate and targeted integration

Updated: 2026-10-05. This policy supersedes the mandatory P3.1–P3.7 expansion.
The [previous roadmap](archive/phase-3-roadmap-20260929.md) is historical only; its evidence and implementation history are preserved.

## When to test

| Trigger | Required work |
| --- | --- |
| Every code change | Layer 1 matching, allowlists, schedules, precedence, allowances/unlocks, extension and component checks. |
| Relevant integration changes | Focused real-browser, daemon/RPC, hosts or process tests. Ubuntu first; other distributions/package variants when affected. |
| Release | Layer 2 package validation and the existing 21-observation installed-package smoke suite on Ubuntu, Fedora and CachyOS. |
| Affected features or release candidates | Explicitly selected broader compatibility, timing, recovery or desktop-safety cases. |

The layers describe evidence boundaries; these groups describe execution frequency.
Do not duplicate every deterministic policy permutation across VMs.

## Release gate

Keep the existing 17 smoke IDs / 21 observations per guest. Use native Firefox
and Chrome on Ubuntu/Fedora, Firefox and Chromium on CachyOS. One complete clean
fresh-clone pass per required configuration is sufficient. Repeatability pairs
are reserved for new adapters, substantial harness changes or suspected flakiness.
A failed attempt remains evidence; resolve it and use a new run ID.

Require exact verified release artifacts, recorded browser/store-extension and
harness versions, real visible blocking/process termination, complete results,
base integrity and guarded clone cleanup. Select the actual release candidate's
package reports; run 36269349435 is a historical development baseline, not a
permanent release target. Published extensions verify their published version;
source-extension changes need targeted coverage of the changed version as well.

The 77 specification IDs are a scenario catalog, and the 42 browser/package cells
are a compatibility inventory. Neither is a mandatory completion matrix.
Missing required observations fail the selected gate; unselected catalog cases
remain unverified and do not block it. Never call a smoke pass full-matrix acceptance.

## Verified feature baseline

Recorded results refer to package run 36269349435, not arbitrary current source.
See the [validation report](reports/phase-3-validation.md) for exact evidence.

| Feature | Recorded coverage | Boundary |
| --- | --- | --- |
| Installation, services, GUI Health, credentials | All three guests | Credential presentation does not prove edit-key/uninstall workflows. |
| Post-install reboot | GUI/RPC recovery on all three guests; pre-existing website/process enforcement on Ubuntu | Ubuntu additionally checks real process termination and persisted-rule block pages in Firefox/Chrome. |
| GUI policy append/duplicate/conflict/export | All three guests | Website fixtures; application-policy variants not established. |
| Store onboarding, heartbeat, browser restart and policy visibility | Two native browsers per guest | Private behavior is covered separately below; extension disable/remove protection remains unverified. |
| Private-window allowlists and Chromium settings locks | All three guests, 12 selected observations plus 21 smoke prerequisites each | Native Firefox/Chrome on Ubuntu/Fedora, Firefox/Chromium on CachyOS. Firefox private access; published Chrome/Chromium 0.2.6 split mode after explicit consent/restart; pre-consent gap, disabled incognito, GUI lock and daemon rejection. Confined/alternative browsers remain unverified. |
| Exact URL, real process blocking and near misses | All three guests | Process block matcher is command name. |
| Tier 2 website/application allowlist via Detox | All three guests | Path-based process allowlist; no complete desktop-safety claim. |
| Other website matchers, subdomains, open tabs, SPA | Ubuntu Firefox/Chrome | Includes managed hosts resolution; not every input variant. |
| Tier 1 allowlist rejection; allowlists leave hosts unchanged | Ubuntu ordinary-user RPC | Not GUI authoring or private-window coverage. |
| Schedule and natural Detox activation/expiry | Ubuntu Firefox/Chrome plus real processes | Website blocklist under schedule, website allowlist under Detox, application blocklist for both; not every schedule or rule variant. |
| Tier 3 website allowance exhaustion | Ubuntu Firefox/Chrome | Real browsing and named block page; one-minute allowance under Detox. |
| Tier 3 website allowlist accounting/unlock/expiry | Ubuntu Firefox | Allowed-only browsing stays uncharged; outside browsing exhausts allowance; two-minute unlock expires without refill. Chrome unlock remains unverified; no GUI unlock-flow claim. |
| Tier 3 application allowlist accounting/unlock/expiry | Ubuntu background processes | Allowed-only activity stays uncharged; rejected activity exhausts allowance; two-minute unlock expires without refill. No foreground/minimized variants or GUI unlock flow. |
| GUI list authoring and active protected edits | All three guests, 12 selected observations plus smoke each | Tier 2 website/application blocklists and allowlists under Detox; warning dialogs, tightening, weakening rejection and protected mode changes. Tier 1 edit-key authorization is a separate journey. |
| Independent helper identity and product control path | All three guests, 2 selected observations plus smoke each | Rejected helpers terminate under two ordinary accounts; permitted parents and GUI/native control remain available; helpers run again after natural expiry. No full second-account desktop/browser-login or destructive recovery claim. |
| Tier 1 website edit-key authorization | All three guests, 5 focused Layer 4 observations each | No-key/wrong-key rejection, valid native GUI save, natural five-minute expiry and active Tier 2/3 website-blocklist boundaries under Detox. Uses permitted `all_time` protected access; application and other window/activation variants remain unverified. |

## Bounded Ubuntu implementation queue

Implement representative journeys using normal product operations and real time.
Keep outcomes separate from implementation status. Do not shorten product timers,
reset usage/quota, simulate extension visit heartbeats, or alter the guest clock
for acceptance. Additional browser groups currently include smoke setup costs.

1. **Schedule and Detox transitions — verified:** before/during/after website and process
   enforcement (`VM-ACT-001`, `VM-ACT-002`), with natural expiry and restored access.
   Website Detox includes allowlist activation; further schedule/allowlist combinations
   are selected when affected, not required to finish this batch.
2. **Tier 3 lifecycle — representative journeys verified:** website allowance
   exhaustion in Firefox/Chrome; allowlist accounting, normal manual unlock and
   reblocking on expiry in Firefox and background application processes. These
   use fresh-state isolation and the global hourly unlock quota. Further browser,
   activation and foreground/minimized variants are selected when affected.
3. **Active-policy reboot — verified:** seed rules before reboot and verify actual browser and
   process enforcement afterward, extending the existing reboot coverage.
4. **GUI authoring/protected edits — verified:** one website and application creation journey,
   warnings, permitted tightening and rejected weakening for both modes.
5. **Process helpers/control path — verified:** independent helper identity, BlocKuntu
   exemptions and control-path availability belong to focused Layer 3 checks,
   with a supplemental process check under a second ordinary account.
   Deliberate desktop degradation, broken integration and data recovery remain
   Layer 4 (`DES-AAL-*`, `DES-BROWSER-*`, `DES-DATA-*`).
6. **Private browsing — verified:** documented consent/disabled behavior and active-allowlist
   protection of the relevant settings.

All six have saved passing Ubuntu evidence. Further private-browser variants and
feature journeys are targeted integration work when those features change or
for selected release candidates,
not automatic additions to every distribution's release gate. The runbook
records which selections are actually executable and which have saved VM passes.

The GUI group `ubuntu-gui-20261002-i` passed. The interrupted helper group
`ubuntu-helper-20261002-a` was recovered and removed; fresh group
`ubuntu-helper-20261003-b` passed 2 selected observations plus 21 smoke checks.
The combined GUI/helper audit passed with all owned clones removed and bases
unchanged. The earlier Chrome private group failed on Web Store 0.2.5's incognito
block-page rendering. Fresh `ubuntu-private-20261005-e-ubuntu` passed all
12 selected observations plus 21 smoke prerequisites with published store
0.2.6 and a captured split-mode manifest. The
[private-window audit](results/ubuntu-private-20261005/audit-e.json) verifies
visible named block pages, allowed access, consent/restart, disabled incognito,
settings locks, exact provenance, unchanged bases and clone cleanup. Earlier
failures remain failed and preserved. See the
[desktop commands](phase-3.md#ubuntu-desktop-commands) and validation report.

## Focused pre-release checks

The focused Ubuntu journey now verifies no-key and invalid-key rejection, valid
GUI authorization/save, real five-minute expiry and Tier 2/3 edit boundaries
(`DES-PROTECT-004–007`). Its [small Layer 4 runner](phase-3.md#ubuntu-tier-1-edit-key-journey)
passed in `ubuntu-tier1-20261003-b`, with a separate evidence/cleanup audit.
It uses the installed package without browser-store prerequisites. Keep authorized
uninstall with cleanup as a later pre-release check. Expand either journey to
other distributions when its integration changes; neither adds a mandatory
full Layer 4 matrix. Further timer/window/list-mode permutations remain selected
by affected features, rather than added to routine VM requirements.

## Selected Fedora and CachyOS expansion

Reuse the accepted representative Ubuntu journeys: 12 GUI-editing observations,
2 helper/two-account observations and 5 Tier 1 website-edit observations per
guest. Validate the native RPM/Arch artifacts on fresh clones, including their
GNOME/KDE session integration, ordinary-user permissions and cleanup. The GUI
and helper groups each include existing smoke prerequisites; Tier 1 remains a
separate small Layer 4 selection without browser-store onboarding. This is a
bounded portability check, not an expansion of the routine release gate or all
scenario permutations.

Completed on 2026-10-04: both guests passed all three journeys, **19 selected
observations plus 42 smoke prerequisites each**. The
[portability audit](results/desktop-port-20261003/audit.json) verifies all six
accepted bundles, exact artifacts, archived captured harness sources, base
integrity and removal of every owned clone/storage file. Earlier failures remain
preserved. See the [validation report](reports/phase-3-validation.md) for run IDs
and harness-version differences; this verifies the historical development
artifacts, not arbitrary current source or a new release candidate.

Published Chrome 0.2.6 private-window verification passed separately on Ubuntu
on 2026-10-05. The bounded portability follow-up also passed on Fedora
(Firefox/Chrome) and CachyOS (Firefox/Chromium): twelve selected observations
and 21 smoke prerequisites each. Accepted runs are `fedora-private-20261005-d-fedora`
and `cachyos-private-20261005-c-cachyos`. The
[combined audit](results/private-port-20261005/audit-final.json) verifies
**24 selected observations plus 42 smoke prerequisites**, exact artifacts,
published 0.2.6 split-mode manifests, archived harness/planner sources, base
integrity and removal of all five owned attempt clones/storage files. Earlier
failed attempts remain failed. The validation report records the accepted
source-C/source-D differences. This does not enlarge the routine release smoke
gate or add private-browser permutations.

## Deferred from routine VM expansion

- Exhaustive schedule/day/overnight/AM-PM combinations, matcher inputs, precedence,
  allowance amounts and unlock permutations: deterministic source/component tests.
- Additional application matcher and process-discovery UI variants: targeted when changed.
- Full Snap/Flatpak/alternative-browser matrix: affected variants or selected RC checks;
  untested variants stay explicitly unverified.
- Full desktop audio/clipboard/IDE/portal permutations and long soaks: separate hardening.
- Overview, URL-test UI, statistics, Detox history, log export, notification preferences,
  formatting/build/update/welcome settings: component/RPC tests or manual release review.
- Twelve-week Detox and clock tampering: optional hardening, not routine release smoke.
- Upgrade: remains deferred until an accepted old-to-new package pair is selected;
  revisit before promising upgrade support.

## Completion and change selection

The **agreed Layer 3 development milestone is complete for now**, following
the audited private-window passes on Ubuntu, Fedora and CachyOS on 2026-10-05.
This closes the bounded
implementation queue and selected portability checks on the recorded development
artifacts. It does not establish full-catalog acceptance or validate a future
release candidate. Keep fast tests per change, focused integration selections
when affected, and the small three-distribution smoke gate on actual release
artifacts. Destructive recovery and selected soaks belong to
[Layer 4](phase-4.md), which remains separately planned.
No obligation remains to make core/full or every browser-matrix plan executable.
Preserve useful existing tests and evidence.

| Changed area | Additional selection |
| --- | --- |
| Matching/precedence | Source regressions; representative browser integration if enforcement changes. |
| Extension/navigation/Native Messaging | Ubuntu Firefox and Chrome. |
| Package scripts/systemd/permissions | Affected guests; all three for shared changes. |
| Processes/exemptions | Real process/helper enforcement and affected desktop checks. |
| Schedule/metering/unlock | Deterministic clocks plus representative real-time transitions. |
| Browser-package paths | The exact affected native/confined package. |

Local execution remains the delivery path. Self-hosted CI is optional follow-up;
there is no mandatory nightly full matrix or all-Layer-4 release gate.
