# BlocKuntu Uninstall

The Debian package and the manual `/usr/local` installation are different
installation types. Use the matching removal path.

## Debian package uninstall

Use **Settings → Protected Changes and Uninstall**. Enter the recovery uninstall
phrase, then select **Uninstall BlocKuntu**. The GUI asks the daemon for a short,
one-time removal lease before it runs `pkexec dpkg --purge blockuntu`.

Direct `dpkg`, `apt remove`, and `apt purge` calls are intentionally refused
because they do not have that lease. This prevents a package-manager invocation
from stopping enforcement without the protected GUI flow.

The Sunday restriction is optional and off by default. If it has been enabled,
uninstall is available only on Sunday from 20:00 through 23:59 local time.

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
- BlocKuntu-owned Firefox and Chrome policy files.

Restart affected browsers after uninstall if they continue to show managed
policy state. User-installed development manifests and per-user Firefox
Snap/Flatpak integration may remain and can be removed manually if no longer
needed.

## Manual developer installation

If the machine was installed with `./scripts/install-production.sh`, remove it
with:

```bash
./scripts/uninstall-production.sh
```

Review the available cleanup options first:

```bash
./scripts/uninstall-production.sh --help
```

The script normally preserves configuration/data and browser policy files unless
its explicit cleanup flags are used. Do not use it for a Debian package install.
