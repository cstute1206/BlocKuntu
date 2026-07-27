# BlocKuntu Production Installation

This document describes a manual production-style install on a new Linux device.
It installs the daemon, Native Messaging host, systemd units, and the Tauri GUI
into system paths. Browser extensions are installed manually by the user.
Browser managed policy is not written at install time; the daemon defers it
until the matching extension sends its first heartbeat through Native Messaging.

This is not yet a polished installer. Read the commands before running them:
they write to `/usr/local`, `/etc`, `/var/lib`, `/run`, and browser managed
policy locations.

## Quick Current-Code Debian Package Guide

Use this path when you changed the repository and want a new `.deb` that
contains the current daemon, native host, Tauri GUI, browser artifacts, systemd
units, and packaging metadata from this checkout.

Run from the repository root on the build machine:

```bash
./scripts/package-deb.sh
```

Do not pass `--no-build` when you want the package to include current source
changes. The default build path compiles:

- `focusd/target/release/blockuntud`
- `native-host/target/release/blockuntu-native`
- `focus-gui/src-tauri/target/release/blockuntu-gui`

The GUI build goes through `npm run tauri -- build --no-bundle`, which embeds
the current frontend assets. That is what prevents a packaged GUI from opening
the development URL at `http://localhost:1420`.

The current default Debian package version is `0.1.0-18`, and the artifact is:

```bash
target/debian/blockuntu_0.1.0-18_$(dpkg --print-architecture).deb
```

Inspect the package before copying it to a target machine:

```bash
dpkg-deb -I target/debian/blockuntu_0.1.0-18_$(dpkg --print-architecture).deb
dpkg-deb -c target/debian/blockuntu_0.1.0-18_$(dpkg --print-architecture).deb | less
```

Install the package on the target Ubuntu/Debian machine with `apt`, not raw
`dpkg -i`:

```bash
sudo apt install ./target/debian/blockuntu_0.1.0-18_$(dpkg --print-architecture).deb
sudo usermod -aG blockuntu "$USER"
```

Log out and back in after the `usermod` command so the GUI, browsers, and
shells receive the `blockuntu` socket-group membership.

When the GUI is running, closing the window hides it to the BlocKuntu tray icon
instead of stopping the GUI process. Use the tray menu to show the window,
open Detox/Settings, refresh daemon status, or quit only the GUI. Enforcement
cannot be stopped from the tray, GUI, or daemon RPC. GNOME sessions may need
AppIndicator/KStatusNotifierItem support before the tray icon is visible; KDE
Plasma, XFCE, Cinnamon, MATE, and Ubuntu-style GNOME sessions are typically the
smoother path.

For Debian-package installs, uninstall through the GUI Settings page when
possible: type the saved per-user uninstall phrase and run the uninstall action.
The GUI uses `pkexec` to execute the package purge. The equivalent terminal
command is:

```bash
sudo dpkg --purge blockuntu
```

Package purge stops and disables the BlocKuntu services, removes the managed
`/etc/hosts` block, removes BlocKuntu-owned browser policies, and removes
`/etc/blockuntu`, `/var/lib/blockuntu`, and `/run/blockuntu`.

See [UNINSTALL.md](UNINSTALL.md) for the full uninstall phrase and cleanup
behavior.

Set a Tier 1 credential in **Protected Changes and Uninstall** before you need
to edit an active Tier 1 list. The daemon stores only the credential verifier;
the credential is never displayed by the GUI.

The package creates a random installation serial at
`/etc/blockuntu/installation-id`. Settings displays it in Health.
Package upgrades preserve the serial, while removal or purge deletes it so a
later reinstall receives a new identity.

The runtime layout table below describes the scripted/manual production install
under `/usr/local`. The Debian package uses Debian package paths instead,
including `/usr/bin/blockuntud`, `/usr/bin/blockuntu-native`, and
`/usr/bin/blockuntu-gui`.

If you used `./scripts/install-production.sh` instead of the `.deb`, use the
scripted uninstaller:

```bash
./scripts/uninstall-production.sh
```

Use destructive cleanup flags only when you intentionally want to remove data,
browser policies, or the system group:

```bash
./scripts/uninstall-production.sh --purge-data
./scripts/uninstall-production.sh --remove-browser-policies
./scripts/uninstall-production.sh --remove-group
```

## Target System

Expected environment:

- Linux with systemd.
- A normal desktop user account.
- Root access through `sudo`.
- Firefox installed as a normal system package, Snap, or Flatpak.
- Google Chrome or Chromium only if you want Chrome enforcement.

On Ubuntu, check the Firefox packaging before testing browser integration:

```bash
which firefox || true
readlink -f "$(command -v firefox)" 2>/dev/null || true
snap list firefox 2>/dev/null || true
flatpak info org.mozilla.firefox 2>/dev/null || true
```

BlocKuntu supports system Firefox, Firefox Snap, and Firefox Flatpak, but the
policy locations differ. System Firefox and Firefox Snap can read the host
`/etc/firefox/policies/policies.json` policy path. Firefox Flatpak reads policy
through Flatpak's `org.mozilla.firefox.systemconfig` extension, so BlocKuntu
writes a per-user policy extension under the user's Flatpak data directory.

Runtime layout:

| Purpose | Path |
| --- | --- |
| Daemon binary | `/usr/local/bin/blockuntud` |
| Native host binary | `/usr/local/bin/blockuntu-native` |
| Config | `/etc/blockuntu/config.toml` |
| Event log | `/etc/blockuntu/blockuntu.log` |
| SQLite database | `/var/lib/blockuntu/blockuntu.sqlite3` |
| Daemon socket | `/run/blockuntu/blockuntud.sock` |
| Hosts fallback | `/etc/hosts` |
| Firefox Native Messaging manifest | `/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json` |
| Firefox Flatpak Native Messaging manifest | `~/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/blockuntu_native.json` |
| Firefox Flatpak managed policy | `$XDG_DATA_HOME/flatpak/extension/org.mozilla.firefox.systemconfig/<arch>/stable/policies/policies.json`, defaulting to `~/.local/share/flatpak/extension/...` |
| Firefox Flatpak copied XPI | `~/.var/app/org.mozilla.firefox/data/blockuntu/BlocKuntu-Signed.xpi` |
| Firefox Snap Native Messaging manifest | `~/snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json` |
| Chrome Native Messaging manifest | `/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json` |
| Chromium Native Messaging manifest | `/etc/chromium/native-messaging-hosts/blockuntu_native.json` |

Firefox enterprise policy locations on Linux are package-dependent. Firefox
checks `distribution/policies.json` under its installation directory, and
system-wide policy can also be placed at
`/etc/firefox/policies/policies.json`. If `/etc/firefox` does not exist on a
normal system Firefox install, creating it is fine:

```bash
sudo install -d -m 0755 /etc/firefox/policies
```

When the BlocKuntu GUI starts, it automatically runs the confined-browser
helper for the logged-in user. It copies `blockuntu-native` into the browser's
writable app area and writes the matching per-user manifest. For Firefox
Flatpak it also copies the signed Firefox XPI and writes the
`org.mozilla.firefox.systemconfig` managed policy that force-installs and locks
the extension. For a manual production installation, run the helper explicitly:

```bash
./scripts/setup-confined-firefox-native-host.sh --native-host /usr/local/bin/blockuntu-native
```

The helper also applies this Flatpak override when Firefox Flatpak is present:

```bash
flatpak override --user --filesystem=/run/blockuntu org.mozilla.firefox
```

That override exposes only the BlocKuntu daemon socket directory to Firefox
Flatpak. The Flatpak policy appears inside the Firefox sandbox at
`/app/etc/firefox/policies/policies.json`. Firefox Snap can execute the copied
host from `~/snap/firefox/common/.local/share/blockuntu/blockuntu-native` and
uses the normal host `/etc/firefox/policies/policies.json` managed policy path.

The scripted installer and Debian package start the daemon with
`--defer-browser-policy-repair-until-heartbeat`. That means they do not create
`/etc/firefox/policies/policies.json` or Chrome managed policy at install time
for system Firefox, Firefox Snap, or Chrome. This is intentional: install and
enable the browser extension manually first, then the first heartbeat activates
managed browser policy repair. Firefox Flatpak is the exception because it
cannot use that host policy path; the confined-browser helper writes the
Flatpak systemconfig policy immediately when the signed XPI is available.

## Install Prerequisites

The scripted installer checks for required commands and Linux GUI build
libraries. If something is missing, it installs the matching packages through
the system package manager. Supported package managers are `apt-get`, `dnf`,
`pacman`, and `zypper`.

On Debian or Ubuntu, the equivalent manual baseline is:

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  cargo \
  curl \
  e2fsprogs \
  file \
  git \
  jq \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  nodejs \
  npm \
  pkg-config \
  rustc \
  socat \
  systemd \
  unzip \
  wget \
  wmctrl \
  zip
```

Install Rust through your preferred system package or `rustup`, then verify:

```bash
rustc --version
cargo --version
node --version
npm --version
```

For the Tauri GUI build, the installer also installs the distribution-specific
WebKitGTK, app indicator, SVG, OpenSSL, and X11 helper packages. The daemon and
browser enforcement do not require the GUI to be running.

## Scripted Install

For a normal production-style install, run the installer from the repository
root:

```bash
./scripts/install-production.sh
```

By default, the script:

- Builds `blockuntud` and `blockuntu-native` as release binaries.
- Builds the Tauri GUI and installs it as `/usr/local/bin/blockuntu-gui`.
- Installs missing build/runtime prerequisites through the system package
  manager.
- Installs the GUI desktop launcher as
  `/usr/share/applications/local.blockuntu.gui.desktop`, matching the Tauri
  application identifier so GNOME can associate and retain notifications.
- Installs a minimal `/etc/blockuntu/config.toml` if it does not already exist.
  It contains strict browser enforcement only; site lists, schedules,
  allowances, and user app rules start empty.
- Installs systemd units and the Native Messaging manifests.
- Installs per-user Firefox Snap/Flatpak Native Messaging manifests when those
  confined browser builds are present. For Firefox Flatpak, it also installs
  the per-user systemconfig policy and copied XPI.
- Adds the current desktop user to the `blockuntu` socket group.
- Starts the daemon with browser policy repair deferred until the first
  extension heartbeat.
- Enables and starts `blockuntu.socket`, `blockuntu.service`,
  `blockuntu-watchdog.service`, and `blockuntu-hosts.path`.

Browser extensions are intentionally not installed or force-installed for
system Firefox, Firefox Snap, or Chrome by this script. The user must install
and enable those extensions manually. The script installs the Native Messaging
manifests so the manually installed extension can reach
`/run/blockuntu/blockuntud.sock` through `blockuntu-native`. For Firefox
Snap/Flatpak, the script installs per-user manifests and native-host copies for
the selected desktop user. For Firefox Flatpak, it also writes the per-user
systemconfig policy because Flatpak cannot consume the host
`/etc/firefox/policies` policy file.

Because browser policy repair is deferred, a missing
`/etc/firefox/policies/policies.json` or Chrome
`/etc/opt/chrome/policies/managed/blockuntu.json` is expected immediately after
install. The daemon writes the matching policy after the first extension
heartbeat.

Installer options:

```bash
./scripts/install-production.sh --help
./scripts/install-production.sh --no-build
./scripts/install-production.sh --no-start
./scripts/install-production.sh --skip-prereqs
./scripts/install-production.sh --skip-gui
./scripts/install-production.sh --overwrite-config
./scripts/install-production.sh --user christian
```

Uninstall the scripted production install:

```bash
./scripts/uninstall-production.sh
```

Uninstall options:

```bash
./scripts/uninstall-production.sh --help
./scripts/uninstall-production.sh --purge-data
./scripts/uninstall-production.sh --remove-browser-policies
./scripts/uninstall-production.sh --remove-group
./scripts/uninstall-production.sh --yes
```

After the script completes, log out and back in so the desktop session receives
the new `blockuntu` group membership.

## Debian Package Build

Build a complete Debian package from the repository root:

```bash
./scripts/package-deb.sh
```

The package is written to `target/debian`, for example:

```bash
target/debian/blockuntu_0.1.0-18_$(dpkg --print-architecture).deb
```

On a target Ubuntu/Debian machine, install it with:

```bash
sudo apt install ./target/debian/blockuntu_0.1.0-18_$(dpkg --print-architecture).deb
```

Use `apt install ./...deb`, not `dpkg -i`, for normal installs. `dpkg -i`
only unpacks the local package and leaves it unconfigured when dependencies are
missing. If you already ran `dpkg -i` and saw dependency errors, recover with:

```bash
sudo apt install -f
```

The build machine needs `node`, `npm`, `rustc`, `cargo`, and the Tauri build
libraries. The target machine does not need those build tools; it only needs
the runtime dependencies declared by the package.

The `.deb` installs:

- `blockuntud`, `blockuntu-native`, and `blockuntu-gui`
- systemd units
- Native Messaging manifests
- the GUI desktop launcher and icons
- a minimal config with only strict browser enforcement enabled
- a random installation serial at `/etc/blockuntu/installation-id`
- local extension artifacts used later as managed-policy install sources

It does not install or enable browser extensions inside Firefox or Chrome, and
it does not create browser policy files during package installation. Add the
desktop user to the socket group after package install:

```bash
sudo usermod -aG blockuntu "$USER"
```

Then log out and back in, open the GUI once for the first-run overview, and
configure protected changes before editing Tier 1 rules. Install and enable the
browser extension manually, then restart the browser. The daemon writes the
matching managed policy after the first heartbeat.
Closing the GUI window keeps BlocKuntu available from the tray icon. On vanilla
GNOME, install or enable AppIndicator/KStatusNotifierItem support if the tray
icon is not visible.

## Manual Build And Install

The following steps are the manual equivalent of the scripted install. Use them
when you need to inspect or customize individual installation steps.

Run from the repository root:

```bash
./scripts/verify-focus-core.sh
./scripts/verify-focusd.sh
./scripts/verify-native-host.sh
./scripts/verify-firefox-extension.sh
./scripts/verify-chrome-extension.sh
```

Build release binaries:

```bash
cargo build --manifest-path focusd/Cargo.toml --release --locked
cargo build --manifest-path native-host/Cargo.toml --release --locked
```

Build the Firefox extension package:

```bash
cd browser-extension-firefox
npm ci
npm run package:amo
cd ..
```

For production Firefox, use a signed XPI. If
`browser-extension-firefox/BlocKuntu-Signed.xpi` is missing, sign the generated
`browser-extension-firefox/BlocKuntu.xpi` through Mozilla first. The unsigned
local XPI is not a reliable production install artifact for regular Firefox.

Build the Chrome extension if you use Chrome or Chromium:

```bash
cd browser-extension-chrome
npm ci
npm run package:zip
cd ..
```

Chrome managed installation needs a CRX reachable through the update manifest.
The daemon currently defaults to:

```text
https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download
```

If you host the CRX somewhere else, use that URL in the systemd override below.

## Manual Install Files

Create the socket group and add your desktop user. Log out and back in after
this step so group membership reaches Firefox, Chrome, the GUI, and shells:

```bash
sudo groupadd --system blockuntu 2>/dev/null || true
sudo usermod -aG blockuntu "$USER"
```

Install binaries, config, browser artifacts, and Native Messaging manifests:

```bash
sudo install -Dm755 focusd/target/release/blockuntud \
  /usr/local/bin/blockuntud
sudo install -Dm755 native-host/target/release/blockuntu-native \
  /usr/local/bin/blockuntu-native

sudo install -Dm644 packaging/deb/blockuntu.toml \
  /etc/blockuntu/config.toml
sudo install -Dm644 browser-extension-firefox/BlocKuntu-Signed.xpi \
  /usr/local/share/blockuntu/BlocKuntu-Signed.xpi

sudo install -Dm644 packaging/native-messaging/blockuntu_native.json \
  /usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json
sudo install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json \
  /etc/opt/chrome/native-messaging-hosts/blockuntu_native.json
sudo install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json \
  /etc/chromium/native-messaging-hosts/blockuntu_native.json

./scripts/setup-confined-firefox-native-host.sh \
  --native-host /usr/local/bin/blockuntu-native
```

Install systemd units:

```bash
sudo install -Dm644 packaging/systemd/blockuntu.socket \
  /etc/systemd/system/blockuntu.socket
sudo install -Dm644 packaging/systemd/blockuntu.service \
  /etc/systemd/system/blockuntu.service
sudo install -Dm644 packaging/systemd/blockuntu-watchdog.service \
  /etc/systemd/system/blockuntu-watchdog.service
sudo install -Dm644 packaging/systemd/blockuntu-hosts.path \
  /etc/systemd/system/blockuntu-hosts.path
sudo install -Dm644 packaging/systemd/blockuntu-hosts.service \
  /etc/systemd/system/blockuntu-hosts.service
```

Add a production path override. This avoids the daemon using the development
default XPI path from the source tree and defers browser policy repair until
the first browser-extension heartbeat.

```bash
sudo install -d -m 0755 /etc/systemd/system/blockuntu.service.d
sudo tee /etc/systemd/system/blockuntu.service.d/10-production-paths.conf >/dev/null <<'EOF'
[Service]
ExecStart=
ExecStart=/usr/local/bin/blockuntud --extension-xpi /usr/local/share/blockuntu/BlocKuntu-Signed.xpi --chrome-extension-crx-url https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download --defer-browser-policy-repair-until-heartbeat serve
EOF
```

If you host the Chrome CRX somewhere else, replace the URL in the override.

If you do not use Chrome or Chromium on this device, the managed Chrome policy
file is harmless. To avoid strict-mode Chrome heartbeat requirements if Chrome
is later started without a working extension, set this value in
`/etc/blockuntu/config.toml`:

```toml
[strict_mode]
require_chrome_extension = false
```

## Manual Enable Services

Verify unit syntax:

```bash
sudo systemd-analyze verify \
  /etc/systemd/system/blockuntu.socket \
  /etc/systemd/system/blockuntu.service \
  /etc/systemd/system/blockuntu-watchdog.service \
  /etc/systemd/system/blockuntu-hosts.path \
  /etc/systemd/system/blockuntu-hosts.service
```

Enable and start the production units:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now \
  blockuntu.socket \
  blockuntu.service \
  blockuntu-watchdog.service \
  blockuntu-hosts.path
```

Do not enable `blockuntu-hosts.service` directly. It is a oneshot repair unit
triggered by `blockuntu-hosts.path`.

## Verify Installation

Check systemd state:

```bash
systemctl status blockuntu.socket
systemctl status blockuntu.service
systemctl status blockuntu-watchdog.service
systemctl status blockuntu-hosts.path
```

Check the daemon socket permissions:

```bash
ls -l /run/blockuntu/blockuntud.sock
id
```

The socket should be owned by `root:blockuntu` with mode `srw-rw----`, and your
desktop user should be in the `blockuntu` group after a fresh login.

Ask the daemon for status:

```bash
printf '%s' '{"jsonrpc":"2.0","id":1,"method":"status","params":{}}' \
  | socat - UNIX-CONNECT:/run/blockuntu/blockuntud.sock \
  | jq .
```

Ask for enforcement status:

```bash
printf '%s' '{"jsonrpc":"2.0","id":2,"method":"enforcement_status","params":{}}' \
  | socat - UNIX-CONNECT:/run/blockuntu/blockuntud.sock \
  | jq .
```

Before the first extension heartbeat, missing browser policy files are expected.
After the first Firefox heartbeat, verify Firefox policy:

```bash
sudo test -f /etc/firefox/policies/policies.json
sudo jq . /etc/firefox/policies/policies.json
```

In Firefox, open `about:policies` and confirm that the BlocKuntu extension is
force-installed. Restart Firefox after the policy appears.

For Firefox Flatpak, verify the per-user systemconfig policy instead:

```bash
flatpak run --command=sh org.mozilla.firefox -c \
  'test -f /app/etc/firefox/policies/policies.json && sed -n "1,120p" /app/etc/firefox/policies/policies.json'
```

In Firefox Flatpak, open `about:policies` and confirm the policy is active.
Then restart Firefox Flatpak and confirm `about:addons` does not offer a disable
button for BlocKuntu.

After the first Chrome heartbeat, verify Chrome policy:

```bash
sudo test -f /etc/opt/chrome/policies/managed/blockuntu.json
sudo jq . /etc/opt/chrome/policies/managed/blockuntu.json
sudo test -f /usr/local/share/blockuntu/chrome-extension-updates.xml
```

In Chrome, open `chrome://policy`, reload policies, and confirm that the
BlocKuntu extension is force-installed. Restart Chrome after the policy appears.

Verify hosts fallback:

```bash
sudo grep -n 'BLOCKUNTU MANAGED' /etc/hosts
lsattr -d /etc/hosts
```

In production mode the daemon repairs the managed hosts block and reapplies the
immutable flag when it manages the default `/etc/hosts` path.

## First Browser Test

After the service is active and your user has re-logged in:

1. Restart Firefox.
2. Install and enable the BlocKuntu extension manually if it is not present.
3. Check daemon status again and confirm the Firefox extension heartbeat becomes
   recent.
4. Confirm `/etc/firefox/policies/policies.json` appears after that heartbeat
   for system Firefox or Firefox Snap. For Firefox Flatpak, confirm
   `/app/etc/firefox/policies/policies.json` is visible inside the Flatpak
   sandbox after running the confined-browser helper.
5. Add a site list in the GUI, then navigate to a configured hard-blocked
   domain.

For Chrome, repeat the same check with `chrome://extensions` and
`chrome://policy`.

If every site blocks, verify this chain in order:

```text
browser extension -> blockuntu-native -> /run/blockuntu/blockuntud.sock -> blockuntud -> focus-core
```

Most failures are one of:

- The user has not logged out and back in after being added to `blockuntu`.
- The Native Messaging manifest path is wrong for the installed browser build.
- Firefox Snap/Flatpak was installed after BlocKuntu and BlocKuntu has not been
  started since; open it once to run the automatic setup, then restart Firefox.
- The signed Firefox XPI is missing or rejected.
- Chrome cannot reach the configured CRX URL from the update manifest.

## Updating An Existing Install

Rebuild, reinstall the changed files, and restart the service:

```bash
cargo build --manifest-path focusd/Cargo.toml --release --locked
cargo build --manifest-path native-host/Cargo.toml --release --locked

sudo install -Dm755 focusd/target/release/blockuntud \
  /usr/local/bin/blockuntud
sudo install -Dm755 native-host/target/release/blockuntu-native \
  /usr/local/bin/blockuntu-native

sudo systemctl restart blockuntu.service
```

If the Firefox XPI changed, install the new signed XPI and restart Firefox:

```bash
sudo install -Dm644 browser-extension-firefox/BlocKuntu-Signed.xpi \
  /usr/local/share/blockuntu/BlocKuntu-Signed.xpi
sudo systemctl restart blockuntu.service
```

If policies or manifests changed, restart the affected browser.

## Uninstall

For a Debian package install, use the GUI Settings uninstall action and type the
saved per-user uninstall phrase. The GUI uses `pkexec` to run:

```bash
dpkg --purge blockuntu
```

Package removal stops the BlocKuntu systemd units, removes the managed
`/etc/hosts` block, removes BlocKuntu-owned browser policies, and purges the
package config/data paths.

See [UNINSTALL.md](UNINSTALL.md) for exact phrase storage, validation, and
cleanup details.

The scripted uninstall removes the production-style system install:

```bash
./scripts/uninstall-production.sh
```

By default it removes:

- systemd units and drop-ins
- `/usr/local/bin/blockuntud`
- `/usr/local/bin/blockuntu-native`
- `/usr/local/bin/blockuntu-gui`
- system Native Messaging manifests
- GUI launcher and icons
- `/run/blockuntu`
- the BlocKuntu managed block from `/etc/hosts`

By default it preserves:

- `/etc/blockuntu`
- `/var/lib/blockuntu`
- browser policy files
- the `blockuntu` group

Use explicit flags for destructive cleanup:

```bash
./scripts/uninstall-production.sh --purge-data
./scripts/uninstall-production.sh --remove-browser-policies
./scripts/uninstall-production.sh --remove-group
```

## Current Production Limits

- The stronger `nftables` fallback from `Docs/TODO.md` is not installed by this
  procedure.
- A user with unrestricted `sudo` can still remove or alter local enforcement.
  The goal of this install is to prevent easy user-level bypasses, not to defeat
  root access.
