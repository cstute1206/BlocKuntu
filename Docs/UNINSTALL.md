# BlocKuntu Uninstall

This document describes the current uninstall behavior for the Debian package
and the older production-style script. The two paths are intentionally separate:
the Debian package is the normal user-facing install path, while
`scripts/install-production.sh` and `scripts/uninstall-production.sh` are manual
development/admin helpers.

## Debian Package Uninstall

For package installs, use the GUI Settings page when possible. Type either valid
uninstall phrase and run the uninstall action.

The GUI validates the phrase locally, then runs:

```bash
pkexec dpkg --purge blockuntu
```

Direct package-manager removal is deliberately refused. Do not run `dpkg -r`,
`dpkg --purge`, `apt remove`, or `apt purge` for BlocKuntu. Open BlocKuntu
Settings and use its uninstall action instead; it is available only on Sunday
between 20:00 and 23:59 local time.

Settings asks the daemon to prepare a short-lived, one-time package-removal
lease before it invokes `dpkg --purge`. The package `prerm` accepts only that
Settings-authorized invocation, so a direct terminal call cannot stop
enforcement or remove package files.

## Uninstall Phrases

The GUI uses the package-generated recovery uninstall phrase at
`/etc/blockuntu/uninstall-recovery.txt`. It is shown in the welcome modal along
with the Tier 1 edit key and is stored with `root:blockuntu` ownership and mode
`0640`. Choosing to hide recovery credentials removes both files and persists
that choice across upgrades.

## Normal Phrase Validation

For the ordinary uninstall path, the frontend only checks that the uninstall
input is non-empty. The Tauri backend is the authority:

1. It trims the input.
2. It compares the input with the recovery phrase.
3. It accepts the input only if it exactly matches that phrase.
4. If it does not match, ordinary phrase authorization fails before `pkexec` is
   invoked.

## What Package Purge Removes

The package `prerm` stops and disables BlocKuntu services, allows them to stop
cleanly even when manual stop is normally refused, and removes runtime
enforcement state before package files disappear.

Package purge removes or cleans:

- `blockuntu.socket`
- `blockuntu.service`
- `blockuntu-watchdog.service`
- `blockuntu-hosts.path`
- `blockuntu-hosts.service`
- `/run/blockuntu`
- the BlocKuntu managed block in `/etc/hosts`
- system Native Messaging manifests installed by the package
- package binaries under `/usr/bin`
- package GUI launcher and icons
- `/etc/blockuntu`
- `/var/lib/blockuntu`

Browser policy cleanup is also part of package removal:

- Firefox: removes `/etc/firefox/policies/policies.json` only if the file exists
and looks BlocKuntu-owned by containing `blockuntu`.
- Chrome: removes `/etc/opt/chrome/policies/managed/blockuntu.json`.
- Chrome update manifest: removes
`/usr/local/share/blockuntu/chrome-extension-updates.xml` if present.

Restart Firefox or Chrome after uninstall if the browser still shows old managed
policy state.

## What Package Purge Does Not Remove

Package purge does not remove user-owned GUI state outside the package, including
the welcome-modal dismissal state under `~/.local/share/blockuntu` or
`$XDG_DATA_HOME/blockuntu`.

Package purge also does not remove stale development Native Messaging manifests
or confined-browser Firefox files from user profile locations such as:

```text
~/.mozilla/native-messaging-hosts/blockuntu_native.json
~/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/blockuntu_native.json
~/.var/app/org.mozilla.firefox/data/blockuntu/blockuntu-native
~/.var/app/org.mozilla.firefox/data/blockuntu/BlocKuntu-Signed.xpi
$XDG_DATA_HOME/flatpak/extension/org.mozilla.firefox.systemconfig/*/stable/policies/policies.json
~/snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json
~/snap/firefox/common/.local/share/blockuntu/blockuntu-native
~/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json
~/.config/chromium/NativeMessagingHosts/blockuntu_native.json
```

Those user-level files can still matter if a browser extension was previously
connected to a development install.

If Firefox Flatpak was configured, remove the user override with:

```bash
flatpak override --user --nofilesystem=/run/blockuntu org.mozilla.firefox
```

## Remove Versus Purge

The GUI uses `dpkg --purge`, not a plain remove. That matters:

- `dpkg --remove blockuntu` removes package files but can preserve package data
and local files under `/etc/blockuntu`.
- `dpkg --purge blockuntu` removes package files and purges `/etc/blockuntu`,
`/var/lib/blockuntu`, and `/run/blockuntu`.

Use purge when the goal is a real package uninstall.

## Manual Production Script

If the machine was installed with `./scripts/install-production.sh`, use:

```bash
./scripts/uninstall-production.sh
```

That script does not use the GUI uninstall phrases. By default it removes the
manual `/usr/local` install, systemd units, Native Messaging manifests, runtime
files, current-user Firefox Snap/Flatpak helper files, and the BlocKuntu-managed
hosts block. For Firefox Flatpak, that includes the copied XPI and the per-user
`org.mozilla.firefox.systemconfig` policy if it looks BlocKuntu-owned.

It preserves system browser policy files by default. Remove them only when you
intentionally want to delete BlocKuntu-owned system browser policy state:

```bash
./scripts/uninstall-production.sh --remove-browser-policies
```

Optional destructive cleanup flags:

```bash
./scripts/uninstall-production.sh --purge-data
./scripts/uninstall-production.sh --remove-browser-policies
./scripts/uninstall-production.sh --remove-group
```

Use the package uninstall path for Debian package installs, and the script path
only for the manual production-style install.
