# Phase 3: Release smoke and focused integration

Status: **Agreed Layer 3 development milestone complete for now**, as of
2026-10-05. The bounded Ubuntu queue and selected Fedora/CachyOS portability
checks have saved passing evidence. The final Ubuntu private-window group passed
12/12 selected observations plus 21/21 smoke prerequisites with published
Chrome 0.2.6; its [audit](results/ubuntu-private-20261005/audit-e.json) confirms
provenance, base integrity and clone cleanup. The same private-window group
also passed on Fedora and CachyOS: 12/12 selected observations and 21/21 smoke
prerequisites each. Their [combined audit](results/private-port-20261005/audit-final.json)
verifies 24 selected observations plus 42 smoke prerequisites, archived sources
and cleanup. Full-catalog acceptance and
[Layer 4 destructive/recovery work](phase-4.md) remain separate. Keep the
change-triggered testing policy and validate actual release artifacts.

**Historical P3.0 baseline: smoke subset validated on the selected artifacts.**
On 2026-09-27, runs `p30-36269349435-{ubuntu,fedora,cachyos}-{a,b}` each passed
21/21 required observations. Each guest has two comparable fresh-clone passes;
all six runs have identical harness manifests. Packages came from the exact,
provenance-verified Layer 2 run `36269349435`. Published store extensions were
Firefox `0.2.8` and Chrome/Chromium `0.2.5`, unchanged within each pair.
All test clones and copied storage were removed; base integrity was verified.
See the [validation report](reports/phase-3-validation.md),
[pair comparison](results/p30-smoke-pairs.json), and
[final host verification](results/p30-final-state.json).
These are historical artifact-specific results. The [roadmap](phase-3-roadmap.md)
now defines a small release gate and change-triggered integration coverage.
Use the actual candidate package reports for releases. The 77-case catalog and
42-cell browser inventory are not mandatory release matrices. New coverage is
Ubuntu-first. The existing GUI/helper and Tier 1 website-edit journeys now have
accepted Fedora/CachyOS evidence: 19 selected observations plus 42 smoke
prerequisites per guest. The [portability audit](results/desktop-port-20261003/audit.json)
passed on 2026-10-04, with all owned clones removed and bases unchanged.
The existing release smoke gate remains unchanged.

For the implemented smoke subset, the serialized
[group runner](layer3/README.md#serialized-smoke-groups) checks all selected
artifact reports and fresh run IDs before cloning. It then runs each guest in
sequence, retains a per-group log and summary, and stops on incomplete or
invalid evidence. Unimplemented core, browser-matrix, and full groups fail
preflight. This coordinator does not establish full Layer 3 acceptance.

## Single entry point

From the repository root, with the configured libvirt/SSH environment available:

```bash
python3 Testing/scripts/vm-acceptance.py \
  --guest ubuntu \
  --run-id ubuntu-smoke-unique-id \
  --package-report /path/to/new-layer2/debian/result.json
```

Set `--guest` to `ubuntu`, `fedora`, or `cachyos`, and supply that guest's
provenance-verified Layer 2 report and sibling artifact directory. Ubuntu uses
the single Debian-built DEB report; Fedora uses the RPM and CachyOS uses the
Arch report. The default guest is Ubuntu. Smoke and selected/full E2E selections
reuse this same exact package; removing the Ubuntu build does not remove Ubuntu
runtime coverage or make unimplemented full-catalog groups executable.
Run IDs contain 1–40 lowercase letters, digits or hyphens and must be new.
The report's `artifacts/` sibling directory must contain its exact DEB, RPM,
or Arch package.
A failed or source-unverified report, ambiguous package selection, symlink,
or checksum mismatch is rejected before creating a VM. The guest independently
checks the transferred package hash and installed version.

The read-only probe waiting for a browser allowlist Detox to expire uses the
plan's RPC deadline (60 seconds), capped by the remaining 225-second expiry
wait. It does not cancel Detox or retry mutations. Other fixture RPC calls
retain their existing five-second limit. RPC timeout errors identify the method
without exposing request parameters.

The command creates an independent clone of the selected guest, verifies the
Phase 0 baseline and reboot, installs the existing CI package, refreshes group
membership through a reboot, and drives the installed application. CachyOS
uses the supplied, updated base and records its packages and filesystem capacity.
Use `--refresh-cachyos` when the rolling base needs a full update; that option
clears package caches, updates the clone, and reboots before package installation.
Without it, prerequisites install from the existing package indexes; unavailable
versions fail visibly instead of triggering a partial system update.
OS prerequisite updates are separate from deferred BlocKuntu upgrade testing.
The runner does not build BlocKuntu.
Successful clones are removed unless `--keep` is supplied. Failed/incomplete
clones are stopped and retained for diagnosis. Evidence remains in
`Testing/results/RUN_ID/GUEST/acceptance/`; CachyOS also retains cache-clean
and OS-update logs.

`--prepared` accepts a previously prepared, untouched clone of the selected guest with the same
run ID. It still verifies the pristine baseline; it is not a resume option for
an already installed application. Never rerun a completed acceptance directory.
For guarded cleanup of a retained, stopped clone:

```bash
python3 Testing/scripts/phase0-vm.py cleanup GUEST --run-id RUN_ID
```

## Automation stack

- Python standard library: orchestration, assertions, bounded waits, JSON results,
  subprocess/procfs checks and daemon RPC observations.
- Existing libvirt/SSH harness: full copies, pinned SSH host keys, ownership/base
  integrity guards, screenshots, reboot, shutdown and cleanup.
- `pyatspi`/AT-SPI: semantic controls in the actual Tauri/WebKit application,
  native file chooser and installed browsers. No application test build or
  replacement web frontend is used.
- A small host/guest action protocol: whitelisted libvirt key combinations for
  native dialogs/navigation, keyboard entry restricted to fixture/policy URLs,
  and screenshots with validated basenames. It cannot
  execute guest-supplied shell commands on the host.
- Python `unittest`: regression checks for harness trust/safety boundaries.

Dogtail is a possible higher-level replacement for the accessibility adapter;
its GNOME Wayland input support requires gnome-ponytail-daemon. It is not a
current dependency. PyAutoGUI's Linux backend uses X11 and is not the selected
foundation for the Wayland guests. Neither framework replaces the package,
libvirt, systemd or actual process-termination assertions.

References: [Dogtail](https://gitlab.com/dogtail/dogtail),
[PyAutoGUI](https://github.com/asweigart/pyautogui).

## Guest setup and scope

Use the existing graphical Wayland guests. The runner installs Python AT-SPI
bindings in the clone, enables accessibility, and prevents idle screen locking
there. A US keyboard layout is selected in the clone for deterministic native
URL input. Chrome lacks AT-SPI EditableText, and Firefox can accept it without
committing navigation, so both use native keys. Selected inline autocomplete
suffixes are removed before submitting the exact URL. Native
Firefox is downloaded from Mozilla into `/opt/firefox`; its
version is retained in the host browser metadata; its download archive hash
is written inside the disposable guest. CachyOS instead uses the template's
native `/usr/bin/firefox` package and records its package version.
Native Google Chrome is provided by
the Ubuntu template and installed on Fedora; CachyOS uses its native Chromium
package. A missing prerequisite is not an application pass.

GUI form entry waits for the text field to be focused and visible. Dropdown
selection explicitly opens the native popup and observes both its closure and
the requested selected option before moving to the next field. An input dispatch
acknowledgement alone is insufficient: Fedora's WebKit/GTK popup can retain its
keyboard grab while accessibility already reports focus on the next field.
Ubuntu's WebKit can omit `EXPANDED` even while the native popup is visible.
The helper also recognizes an active native popup containing the select's exact
option labels. A temporarily unavailable accessibility tree is not treated as
proof that the popup closed.

Only dedicated profiles under `Testing/layer3/` are used. Chrome uses
`--password-store=basic` to avoid an autologin keyring prompt; no personal account
or credentials are placed in the profile. Extension installation is from the
public Firefox Add-ons and Chrome Web Store pages, with actual installed
versions and current-session heartbeats recorded. No unsigned or developer-loaded
extension substitutes for these checks.

The local HTTP fixture uses reserved `.test` hostnames. The runner adds their
loopback mappings before installing BlocKuntu, outside its managed hosts block.
The initial website cases use exact-URL and allowlist extension enforcement;
these mappings are not evidence for domain-level hosts fallback behavior.

Application enforcement uses dedicated C fixtures and observes their PIDs,
start times, identities and exits. A root-owned supervisor survives restrictive
regular-user allowlists. The smoke safety list records individual executable
paths from the guest session; it is not full desktop-safety acceptance.

## Coverage and results

Release smoke coverage:

- `VM-INSTALL-001` through `005`;
- `VM-DATA-001` through `004` through real GUI dialogs;
- `VM-BR-001`, `VM-BR-002`, `VM-WEB-002`, `VM-WAL-004` separately for native
  Firefox and native Chrome (Ubuntu/Fedora) or Chromium (CachyOS);
- `VM-APP-001` through `003`, and `VM-AAL-004`.

Other catalog cases remain unverified unless selected and executed. `VM-UPGRADE-001` is explicitly skipped
under the agreed initial-release deferral. Browser results must include the
browser dimension; a Firefox pass cannot cover a missing Chrome or Chromium result.

A suite returns nonzero on failed or incomplete acceptance. Setup failures and
missing cases stay visible. One complete clean fresh-clone pass per required release configuration is sufficient.
Use two comparable passes for repeatability investigations or new adapters; the
existing pair checker is retained for that purpose. Broader cases are selected
by changed feature or release-candidate risk, not required as a complete matrix.

Results include package/harness provenance, per-case observations, service and
GUI journals, browser versions and policy paths, exported test policy, and
screenshots. Credential values are omitted from accessibility dumps; private
local evidence directories are not automatically published.

## Coverage selection

The [coverage contract and planner](layer3/README.md) distinguish executable
selections from the broader catalog. `smoke` is the release gate. `core`,
`browser-matrix` and `full` are retained planner interfaces, not mandatory
implementation targets. Unimplemented selections fail before cloning.

Executable Ubuntu groups include the earlier website matcher/domain groups,
Tier 1 allowlist rejections and hosts-free allowlist case, plus the following
new focused journeys. Implementation does not imply a saved passing VM result;
consult the [validation report](reports/phase-3-validation.md).

The bounded 2026-10-01 Ubuntu batch passed schedule/Detox transitions and website
allowance exhaustion in Firefox/Chrome, Firefox allowlist unlock/expiry, and the
background application-allowlist lifecycle. A live audit on 2026-10-02 verified
all eight selected observations, current harness identity, base integrity and
clone cleanup. Chrome unlock remains unverified. Subsequent GUI/helper and
private-window passes are recorded separately below and in the validation
report. These results cover the recorded development artifact.

## Ubuntu lifecycle commands

Use a new prefix and the desired exact-artifact input file for each attempt.
The examples retain the historical development package, not a release candidate.

```bash
python3 Testing/scripts/layer3-run-groups.py --suite core --guest ubuntu \
  --case VM-ACT-001 --case VM-ACT-002 --case VM-WEB-008 \
  --run-prefix ubuntu-lifecycle-new \
  --verified-inputs Testing/results/package-ci/github-36269349435/verified-inputs.json
```

This group checks real schedule and Detox activation/expiry for a browser and
process, then one-minute website allowance exhaustion in Firefox and Chrome.
Each of the three IDs can also be selected alone. Schedule uses a website
blocklist; Detox uses a website allowlist. Application activation uses a blocklist.
The browser groups still execute the existing 21-observation smoke prerequisite.

```bash
python3 Testing/scripts/layer3-run-groups.py --suite core --guest ubuntu \
  --case VM-WAL-012 --browser firefox-native --run-prefix ubuntu-unlock-new \
  --verified-inputs Testing/results/package-ci/github-36269349435/verified-inputs.json
```

Choose `chrome-native` in a separate fresh-clone run for Chrome. Exactly one
browser is required for this group because of the global hourly unlock quota.
It checks allowed-only activity without usage, actual outside-site usage,
exhaustion, a normal two-minute grant, reblocking while Detox is still active,
and no allowance refill. It never resets the quota or synthesizes visit events.

```bash
python3 Testing/scripts/layer3-run-groups.py --suite core --guest ubuntu \
  --case VM-AAL-010 --run-prefix ubuntu-app-lifecycle-new \
  --verified-inputs Testing/results/package-ci/github-36269349435/verified-inputs.json
```

This isolated root-supervised process journey checks allowed-only activity and
also records supplemental background-process exhaustion, unlock and expiry under
Detox. All product RPC uses the ordinary user. The root supervisor only observes
and retains evidence. This does not claim the unimplemented foreground/minimized
variants or both schedule/Detox variants of `VM-AAL-013/016`.

Ubuntu smoke now seeds a website and application rule before its existing
post-install reboot. `VM-INSTALL-005` checks persistent policy and real process
termination; `VM-WEB-002` also checks the persisted rule's visible block page in
each browser. No additional reboot or Fedora/CachyOS behavior was introduced.

Allowance usage is observed through read-only daemon database state; clocks,
usage and quotas are never changed.

## Ubuntu desktop commands

The following focused groups have executable bindings; consult the
[validation report](reports/phase-3-validation.md) for runtime outcomes.
They each use a fresh clone and the existing smoke prerequisite. Use new prefixes
and the desired verified artifact inputs. Representative scenario selection is
explicit so unimplemented protected-field variants cannot be counted as passes.

```bash
python3 Testing/scripts/layer3-run-groups.py --suite core --guest ubuntu \
  --case VM-EDIT-001 --case VM-EDIT-002 --case VM-EDIT-003 \
  --case VM-EDIT-004 --case VM-EDIT-005 \
  --scenario website --scenario application \
  --scenario website-blocklist-mode --scenario website-allowlist-mode \
  --scenario application-blocklist-mode --scenario application-allowlist-mode \
  --run-prefix ubuntu-gui-new \
  --verified-inputs Testing/results/package-ci/github-36269349435/verified-inputs.json

python3 Testing/scripts/layer3-run-groups.py --suite core --guest ubuntu \
  --case VM-BR-004 --case VM-WAL-013 --run-prefix ubuntu-private-new \
  --verified-inputs Testing/results/package-ci/github-36269349435/verified-inputs.json

python3 Testing/scripts/layer3-run-groups.py --suite core --guest ubuntu \
  --case VM-AAL-005 --case VM-AAL-019 --run-prefix ubuntu-helper-new \
  --verified-inputs Testing/results/package-ci/github-36269349435/verified-inputs.json
```

GUI coverage authors website/application blocklists and allowlists, observes
save warnings, permits append/remove tightening, checks actual enforcement,
and verifies that weakening controls and daemon edits are rejected. The four
mode-change scenarios represent `VM-EDIT-005`; its other variants remain planned.
The application journey curates current session identities before activation
using ordinary-user RPC so it can keep exercising the GUI. A root-readable
metadata inventory includes non-dumpable session processes and uses real UIDs,
independently of proc-directory ownership.

Private-window coverage checks Firefox managed private access and Chrome's
manual-consent gap, explicit browser-owned extension consent, disabled-incognito
behavior, and locked private settings under an active website allowlist. These
are native-browser observations, not Snap/Flatpak coverage.
Fresh `ubuntu-private-20261005-e-ubuntu` passed all 12 selected observations
plus 21 smoke prerequisites on Ubuntu 26.04.1: Firefox 157.0 / store extension
0.2.8 and Chrome 154.0.8037.57 / store extension 0.2.6. The
[audit](results/ubuntu-private-20261005/audit-e.json) checks the captured installed
split-mode manifest, visible named block pages and allowed access after consent
and a native browser restart, the documented pre-consent gap, disabled-incognito
creation prevention and GUI/daemon setting locks. All owned clones/storage
are removed and bases unchanged. Manual consent remains conditional and can be
withdrawn in Chrome; disabling private browsing remains recommended for full
allowlist protection.

The helper group observes a real parent and differently identified child, then
checks GUI/RPC/Native Messaging process availability without explicitly allowing
the BlocKuntu components or their descendants. The curated session remains
usable and a rejected helper can run after natural Detox expiry. This is Layer 3
enforcement and recovery-path availability. Supplemental process checks run the
same fixture binaries under a second ordinary account and require the same
global enforcement and restored access; this is not full GUI login or
browser-profile switching coverage. Deliberate desktop degradation,
broken manifests, daemon outages and database recovery remain
[Layer 4](layers/04-destructive-and-soak.md).

## Fedora and CachyOS desktop checks

The GUI and helper selections above also accept `--guest fedora` and
`--guest cachyos`. Run each guest separately with a fresh run prefix; do not
combine guests or GUI/helper selections into an unbound group. They use the
same 12 GUI and 2 helper observations and each retain the 21-observation smoke
prerequisite. Fedora uses native Firefox/Chrome; CachyOS uses Firefox/Chromium.
The private-window adapters also bind the same twelve observations per guest:

```bash
python3 Testing/scripts/layer3-run-groups.py --suite core --guest fedora \
  --case VM-BR-004 --case VM-WAL-013 --run-prefix fedora-private-new \
  --verified-inputs Testing/results/package-ci/github-36269349435/verified-inputs.json

python3 Testing/scripts/layer3-run-groups.py --suite core --guest cachyos \
  --case VM-BR-004 --case VM-WAL-013 --run-prefix cachyos-private-new \
  --verified-inputs Testing/results/package-ci/github-36269349435/verified-inputs.json
```

Each retains 21 smoke prerequisites. Fedora uses native Firefox/Chrome;
CachyOS uses Firefox/Chromium. Fresh `fedora-private-20261005-d-fedora` and
`cachyos-private-20261005-c-cachyos` each passed 12/12 selected observations
plus 21/21 smoke prerequisites. Their
[combined audit](results/private-port-20261005/audit-final.json) verifies
published Chrome/Chromium 0.2.6 split mode, browser-owned consent/restart,
disabled incognito, active-allowlist settings locks, source/artifact provenance
and guarded cleanup. See the validation report for exact browser versions and
retained failed attempts. Executable bindings alone do not establish acceptance.
Broader
lifecycle/matcher adapters remain Ubuntu-only. These private groups are a bounded
portability selection; they do not enlarge the routine release gate.

The focused Tier 1 runner below accepts `--guest fedora` or `--guest cachyos`
and the corresponding provenance-verified RPM or Arch package report. It reuses
GNOME/KDE accessibility setup; CachyOS selects and verifies its US keyboard
layout after the required installation reboot. Credentials remain excluded
from host-action requests and saved text evidence. Both guests passed all three
journeys and the [evidence/cleanup audit](results/desktop-port-20261003/audit.json).
The validation report records exact runs, artifacts and archived harness versions.
This bounded portability check does not add cases to the routine release gate.

## Ubuntu Tier 1 edit-key journey

Accepted on 2026-10-03: `ubuntu-tier1-20261003-b` passed all five selected
observations and its [evidence/cleanup audit](results/ubuntu-tier1-20261003-b-audit.json).
The earlier completion-race attempt remains failed in the validation history.

Run this focused Layer 4 selection independently of browser/store onboarding:

```bash
python3 Testing/scripts/tier1-edit-acceptance.py \
  --guest ubuntu \
  --run-id ubuntu-tier1-new \
  --package-report Testing/results/package-ci/github-36269349435/package-ubuntu-x86_64-605d707b92cc5fa007674169f0e74ce6fc879c33/ubuntu/result.json
```

Select the actual candidate's provenance-verified Ubuntu package report for a
release. The shown report is the historical development baseline.
The runner creates one guarded disposable clone of the selected guest, checks its clean
baseline, installs/checksums the exact package, reboots, and uses the ordinary
desktop account for GUI and policy operations. It checks no-key rejection and
`DES-PROTECT-004–007`: invalid key, valid GUI key plus confirmed name/pattern save,
active Tier 2/3 weakening rejection during the grant, and rejection after natural
five-minute expiry. Protected access uses the package's permitted `all_time`
setting; Sunday/no-active-session window variants remain deferred. No guest
clock changes, database edits or shortened product timers are used.
The selected edit and tier-boundary checks use website blocklists, with Tier 2/3
activated by Detox. Application edit-key behavior and other activation/list-mode
variants are not established by this journey.

Credential input requests contain fixed slots rather than key values. Saved
screenshots follow welcome dismissal; credential values are excluded from
result evidence. The bundle records package and harness hashes, guest identity,
checkpoints, journals, base integrity and clone removal. Missing observations
fail this selection. It does not run or claim the full Layer 3 smoke or Layer 4
suite. Broad destructive/helper outage recovery and authorized uninstall remain
separate follow-up work.

## Local harness checks

```bash
python3 Testing/scripts/test_vm_acceptance.py
python3 Testing/scripts/test_phase0_vm.py
python3 -m unittest discover -s Testing/scripts -p 'test_layer3*.py'
python3 Testing/scripts/test_check_vm_smoke.py
python3 Testing/scripts/test_p32_site.py
python3 -m unittest discover -s Testing/scripts -p 'test_protected_edit_contract.py'
python3 -m unittest discover -s Testing/scripts -p 'test_tier1_runner.py'
```

Inline commands in the ignored `Testing/runtime/layer3-dev.py` were development
probes only. They are not a required step of the final smoke command and do not
establish full acceptance.
