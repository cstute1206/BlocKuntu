# BlocKuntu Production Installation

This document describes a manual production-style install on a new Linux device.
It installs the daemon, Native Messaging host, browser policy files, systemd
units, and the Firefox extension artifact into system paths.

This is not yet a polished installer. Read the commands before running them:
they write to `/usr/local`, `/etc`, `/var/lib`, `/run`, and browser managed
policy locations.

## Target System

Expected environment:

- Linux with systemd.
- A normal desktop user account.
- Root access through `sudo`.
- Firefox installed as a normal system package, not only as Snap or Flatpak.
- Google Chrome or Chromium only if you want Chrome enforcement.

Runtime layout:

| Purpose | Path |
| --- | --- |
| Daemon binary | `/usr/local/bin/blockuntud` |
| Native host binary | `/usr/local/bin/blockuntu-native` |
| Config | `/etc/blockuntu/config.toml` |
| SQLite database | `/var/lib/blockuntu/blockuntu.sqlite3` |
| Daemon socket | `/run/blockuntu/blockuntud.sock` |
| Firefox policy | `/etc/firefox/policies/policies.json` |
| Firefox signed XPI | `/usr/local/share/blockuntu/BlocKuntu-Signed.xpi` |
| Chrome policy | `/etc/opt/chrome/policies/managed/blockuntu.json` |
| Chrome update manifest | `/usr/local/share/blockuntu/chrome-extension-updates.xml` |
| Hosts fallback | `/etc/hosts` |
| Firefox Native Messaging manifest | `/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json` |
| Chrome Native Messaging manifest | `/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json` |
| Chromium Native Messaging manifest | `/etc/chromium/native-messaging-hosts/blockuntu_native.json` |

## Install Prerequisites

Install the build and verification tools for your distribution. On Debian or
Ubuntu, this is a reasonable baseline:

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  curl \
  e2fsprogs \
  git \
  jq \
  nodejs \
  npm \
  pkg-config \
  socat \
  systemd \
  unzip \
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

For the optional Tauri GUI package, install the Tauri Linux dependencies for
your distribution as well. The daemon and browser enforcement do not require
the GUI to be running.

## Build And Verify

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

## Install Files

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

sudo install -Dm644 examples/blockuntu.toml \
  /etc/blockuntu/config.toml
sudo install -Dm644 browser-extension-firefox/BlocKuntu-Signed.xpi \
  /usr/local/share/blockuntu/BlocKuntu-Signed.xpi

sudo install -Dm644 packaging/native-messaging/blockuntu_native.json \
  /usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json
sudo install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json \
  /etc/opt/chrome/native-messaging-hosts/blockuntu_native.json
sudo install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json \
  /etc/chromium/native-messaging-hosts/blockuntu_native.json
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
default XPI path from the source tree.

```bash
sudo install -d -m 0755 /etc/systemd/system/blockuntu.service.d
sudo tee /etc/systemd/system/blockuntu.service.d/10-production-paths.conf >/dev/null <<'EOF'
[Service]
ExecStart=
ExecStart=/usr/local/bin/blockuntud --extension-xpi /usr/local/share/blockuntu/BlocKuntu-Signed.xpi --chrome-extension-crx-url https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download serve
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

## Enable Services

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

Verify Firefox policy:

```bash
sudo test -f /etc/firefox/policies/policies.json
sudo jq . /etc/firefox/policies/policies.json
```

In Firefox, open `about:policies` and confirm that the BlocKuntu extension is
force-installed. Restart Firefox after installing or changing policy files.

Verify Chrome policy if Chrome enforcement is enabled:

```bash
sudo test -f /etc/opt/chrome/policies/managed/blockuntu.json
sudo jq . /etc/opt/chrome/policies/managed/blockuntu.json
sudo test -f /usr/local/share/blockuntu/chrome-extension-updates.xml
```

In Chrome, open `chrome://policy`, reload policies, and confirm that the
BlocKuntu extension is force-installed. Restart Chrome after installing or
changing policy files.

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
2. Open `about:addons` and confirm the BlocKuntu extension is installed by
   policy.
3. Navigate to a configured hard-blocked domain such as `https://reddit.com/`.
4. Check daemon status again and confirm the Firefox extension heartbeat becomes
   recent.

For Chrome, repeat the same check with `chrome://extensions` and
`chrome://policy`.

If every site blocks, verify this chain in order:

```text
browser extension -> blockuntu-native -> /run/blockuntu/blockuntud.sock -> blockuntud -> focus-core
```

Most failures are one of:

- The user has not logged out and back in after being added to `blockuntu`.
- The Native Messaging manifest path is wrong for the installed browser build.
- Firefox is installed as Snap or Flatpak and cannot see the system manifest.
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

## Current Production Limits

- There is no repo-root production installer yet. This document is the manual
  install path.
- The repo-root daemon does not yet have a single production uninstall command.
  Removal should stay deliberate and privileged.
- The stronger `nftables` fallback from `Docs/STRICT_MODE_TODO.md` is not
  installed by this procedure.
- A user with unrestricted `sudo` can still remove or alter local enforcement.
  The goal of this install is to prevent easy user-level bypasses, not to defeat
  root access.
