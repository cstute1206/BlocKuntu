# BlocKuntu Uninstall

BlocKuntu is installed and removed as a package. Debian/Ubuntu use `.deb`; the
Fedora validation path uses an RPM.

## Protected package uninstall

Use **Settings → Protected Changes and Uninstall**. Enter the recovery uninstall
phrase, then select **Uninstall BlocKuntu**. The GUI asks the daemon for a short,
one-time removal lease before it runs the matching package-manager command:

- Debian/Ubuntu: `pkexec dpkg --purge blockuntu`
- Fedora RPM: `pkexec dnf remove --assumeyes blockuntu`

Direct `dpkg`, `apt`, `rpm`, and `dnf` removal calls are intentionally refused
because they do not have that lease. This prevents a package-manager invocation
from stopping enforcement without the protected GUI flow.

In Settings, choose when the protected GUI uninstall is available: Sunday from
20:00 through 23:59 local time, only while no schedule or Detox is active, or
at any time.

## Recovery credentials

The package creates these files on first installation:

```text
/etc/blockuntu/uninstall-recovery.txt
/etc/blockuntu/tier1-edit-key.txt
```

They are shown in the first-run welcome modal. Store them before choosing
**Hide and remove recovery credentials** in Settings: that action permanently
removes both files and prevents the package from recreating them on upgrade.

The Tier 1 edit key unlocks protected Tier 1 changes for five minutes. The
recovery uninstall phrase authorizes the normal package uninstall flow.

## What package purge removes

The authorized purge stops and disables BlocKuntu services, removes the managed
BlocKuntu section from `/etc/hosts`, and removes BlocKuntu-owned browser-policy
files. It then purges the package files and its configuration/runtime data,
including:

- `blockuntu.socket`, `blockuntu.service`, `blockuntu-watchdog.service`, and
  hosts repair units;
- `/run/blockuntu`, `/etc/blockuntu`, and `/var/lib/blockuntu`;
- package binaries, launcher, icons, and system Native Messaging manifests;
- BlocKuntu-owned Firefox, LibreWolf, Waterfox, Chrome, Chromium, Brave, Opera, Microsoft Edge, and Vivaldi policy files. LibreWolf and Waterfox policy entries are restored without removing their browser-owned policy files.

Restart affected browsers after uninstall if they continue to show managed
policy state. Per-user Firefox Snap/Flatpak and Chromium-family Snap integration
may remain and can be removed manually if no longer needed.
