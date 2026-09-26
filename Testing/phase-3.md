# Phase 3: Installed-package smoke acceptance

Status: **Ubuntu smoke validated** on 2026-09-21. Runs `layer3-20260921-c` and
`layer3-20260921-d` each passed all 21 required observations on fresh clones
with identical harness manifests. Both used the provenance-verified artifact
from Layer 2 run `35539295807`, including the watcher boot-ordering fix.
Each run took approximately 15 minutes and removed its clone on success.
See the [validation report](reports/phase-3-validation.md) for evidence.
CachyOS run `layer3-20260926-cachyos-b` passed 20/21 observations on the refreshed
base; Firefox store extension 0.2.6 missed the restart-heartbeat deadline.
The separate startup fix needs a signed store release before fresh acceptance
reruns. Fedora awaits the corrected Layer 2 RPM. Upgrades are deferred.

## Single entry point

From the repository root, with the host VPN disabled:

```bash
python3 Testing/scripts/vm-acceptance.py \
  --guest ubuntu \
  --run-id ubuntu-smoke-unique-id \
  --package-report /path/to/new-layer2/ubuntu/result.json
```

Set `--guest` to `ubuntu`, `fedora`, or `cachyos`, and supply that guest's
provenance-verified Layer 2 report and sibling artifact directory. The default
guest is Ubuntu.
Run IDs contain 1–40 lowercase letters, digits or hyphens and must be new.
The report's `artifacts/` sibling directory must contain its exact DEB, RPM,
or Arch package.
A failed or source-unverified report, ambiguous package selection, symlink,
or checksum mismatch is rejected before creating a VM. The guest independently
checks the transferred package hash and installed version.

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

Initial automated target:

- `VM-INSTALL-001` through `005`;
- `VM-DATA-001` through `004` through real GUI dialogs;
- `VM-BR-001`, `VM-BR-002`, `VM-WEB-002`, `VM-WAL-004` separately for native
  Firefox and native Chrome (Ubuntu/Fedora) or Chromium (CachyOS);
- `VM-APP-001` through `003`, and `VM-AAL-004`.

Remaining Layer 3 cases stay planned. `VM-UPGRADE-001` is explicitly skipped
under the agreed initial-release deferral. Browser results must include the
browser dimension; a Firefox pass cannot cover a missing Chrome or Chromium result.

A suite returns nonzero on failed or incomplete acceptance. Setup failures and
missing cases stay visible. Two consecutive fresh-clone passes with the same
final harness are required before declaring that guest's smoke milestone complete.
The remaining browser matrix and advanced policy/timing cases follow in later
milestones.

Results include package/harness provenance, per-case observations, service and
GUI journals, browser versions and policy paths, exported test policy, and
screenshots. Credential values are omitted from accessibility dumps; private
local evidence directories are not automatically published.

## Harness checks

```bash
python3 Testing/scripts/test_vm_acceptance.py
python3 Testing/scripts/test_phase0_vm.py
```

Inline commands in the ignored `Testing/runtime/layer3-dev.py` were development
probes only. They are not a required step of the final smoke command and do not
establish full acceptance.
