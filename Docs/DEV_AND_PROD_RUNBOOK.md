# BlocKuntu Dev and Production Runbook

This document describes how to run the current BlocKuntu stack during
development, how the intended production installation should work, and what is
still missing before treating the production path as finished.

## Current Status

The development stack is usable today with temporary, non-root paths under
`/tmp/blockuntu`.

The production stack has the main pieces in place:

- `blockuntud` daemon with default root paths.
- Browser Native Messaging bridge.
- Firefox WebExtension source and unsigned XPI packaging helper.
- Chrome/Chromium MV3 extension source plus Chrome force-install policy for
  the hosted CRX.
- Tauri GUI.
- systemd unit files under `packaging/systemd/`.
- Firefox and Chrome Native Messaging manifests under
  `packaging/native-messaging/`.

The production path still needs an audited installer/uninstaller and a final
root-path verification pass before it should be used as a locked-down install.

## Prerequisites

Install the normal Linux development tools:

```bash
rustup default stable
```

Node/npm are needed for the GUI and browser extensions. Firefox and
Chrome/Chromium are needed for extension testing. Tauri also needs the normal
Linux WebKit/GTK development packages for your distribution.

Optional but useful tools:

```bash
socat
systemd-analyze
```

## Development Runtime Paths

The dev helper uses only temporary files:

| Purpose | Path |
| --- | --- |
| Config | `/tmp/blockuntu/config.toml` |
| SQLite database | `/tmp/blockuntu/blockuntu.sqlite3` |
| Daemon socket | `/tmp/blockuntu/blockuntud.sock` |
| Firefox policy sandbox | `/tmp/blockuntu/firefox/policies.json` |
| Chrome policy sandbox | `/tmp/blockuntu/chrome/policies/managed/blockuntu.json` |
| Chrome update manifest sandbox | `/tmp/blockuntu/chrome/updates.xml` |
| Hosts sandbox | `/tmp/blockuntu/hosts` |
| Firefox Native Messaging manifest | `~/.mozilla/native-messaging-hosts/blockuntu_native.json` |
| Chrome Native Messaging manifest | `~/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json` |
| Chromium Native Messaging manifest | `~/.config/chromium/NativeMessagingHosts/blockuntu_native.json` |

The dev daemon copies `examples/blockuntu.toml` into `/tmp/blockuntu/config.toml`
the first time it starts. That example currently hard-blocks Instagram, Reddit,
Twitch, and TikTok, and includes controlled access for YouTube.

## Start The Dev Daemon

From the repository root:

```bash
./scripts/start-dev-daemon.sh
```

Keep this terminal open. The script starts `focusd` with:

```text
socket: /tmp/blockuntu/blockuntud.sock
config: /tmp/blockuntu/config.toml
database: /tmp/blockuntu/blockuntu.sqlite3
firefox policy sandbox: /tmp/blockuntu/firefox/policies.json
chrome policy sandbox: /tmp/blockuntu/chrome/policies/managed/blockuntu.json
chrome update manifest sandbox: /tmp/blockuntu/chrome/updates.xml
hosts sandbox: /tmp/blockuntu/hosts
```

The equivalent manual command is:

```bash
mkdir -p /tmp/blockuntu
cp examples/blockuntu.toml /tmp/blockuntu/config.toml

cargo run --manifest-path focusd/Cargo.toml -- \
  --config /tmp/blockuntu/config.toml \
  --database /tmp/blockuntu/blockuntu.sqlite3 \
  --socket /tmp/blockuntu/blockuntud.sock \
  --firefox-policy /tmp/blockuntu/firefox/policies.json \
  --chrome-policy /tmp/blockuntu/chrome/policies/managed/blockuntu.json \
  --chrome-update-manifest /tmp/blockuntu/chrome/updates.xml \
  --hosts /tmp/blockuntu/hosts \
  --dev-bind-socket \
  serve
```

Use the helper script for normal work; it avoids accidentally touching `/etc`.

## Install The Dev Native Host

In a second terminal, from the repository root:

```bash
./scripts/install-dev-native-host.sh
```

This builds `native-host`, writes a wrapper to:

```text
~/.local/share/blockuntu/blockuntu-native-dev
```

and writes the per-user browser Native Messaging manifests to:

```text
~/.mozilla/native-messaging-hosts/blockuntu_native.json
~/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json
~/.config/chromium/NativeMessagingHosts/blockuntu_native.json
```

The wrapper forces the native host to connect to:

```text
/tmp/blockuntu/blockuntud.sock
```

It also passes a development-only revival command:

```text
./scripts/start-dev-daemon.sh
```

If a browser relaunches `blockuntu-native`, or if the existing native host sees
a missing/stale dev socket, the native host starts that script and retries the
daemon request. The dev daemon script holds `/tmp/blockuntu/dev-daemon.lock` so
repeated heartbeats do not start competing daemons.

Restart Firefox and Chrome after installing or changing these manifests. Browsers
read Native Messaging manifests at process startup.

## Build And Load The Firefox Extension

Build the extension:

```bash
cd browser-extension-firefox
npm install
npm run build
```

Load it temporarily in Firefox:

1. Open `about:debugging#/runtime/this-firefox`.
2. Click "Load Temporary Add-on".
3. Select `browser-extension-firefox/manifest.json`.

The signed extension ID is `blockuntu-poc@example.local`, and the Native
Messaging host name is `blockuntu_native`.

Important behavior: the extension is fail-closed. It blocks all top-level
HTTP/HTTPS navigation until it receives heartbeat acknowledgements through this
chain:

```text
Firefox extension -> blockuntu_native -> blockuntud -> focus-core
```

If every website is blocked, check the daemon terminal first, then reinstall the
dev native host and restart Firefox.

Package a local XPI when needed:

```bash
cd browser-extension-firefox
npm run build
npm run package:xpi
```

This creates:

```text
browser-extension-firefox/BlocKuntu.xpi
```

That file is useful for local packaging tests. Confirm signing/installability
for the target Firefox channel before using it as a production deployment
artifact.

The current signed local artifact used by the daemon policy is:

```text
browser-extension-firefox/BlocKuntu-Signed.xpi
```

## Build And Load The Chrome Extension

Build the extension:

```bash
cd browser-extension-chrome
npm install
npm run build
```

Load it temporarily in Chrome or Chromium:

1. Open `chrome://extensions`.
2. Enable Developer mode.
3. Click "Load unpacked".
4. Select `browser-extension-chrome/`.

The development extension ID is stable because `manifest.json` embeds a fixed
key:

```text
odedgejjcdilkoibeljkeohekonmdfea
```

The Chrome extension is fail-closed through the same daemon path:

```text
Chrome extension -> blockuntu_native -> blockuntud -> focus-core
```

Package a local ZIP when needed:

```bash
cd browser-extension-chrome
npm run build
npm run package:zip
```

The hosted CRX currently used by Chrome force-install policy is:

```text
https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download
```

The daemon writes a local Chrome update manifest that points at that CRX and
then force-installs the extension through Chrome managed policy. Future CRX
uploads must be signed with the same Chrome packaging key, otherwise the
extension ID changes and the policy/native-host origin must be updated again.

## Start The Dev GUI

In another terminal:

```bash
cd focus-gui
npm install
npm run tauri dev
```

The GUI socket field can be left empty. The Tauri backend tries:

```text
/run/blockuntu/blockuntud.sock
/tmp/blockuntu/blockuntud.sock
```

in that order. During development it should settle on `/tmp/blockuntu`.

The GUI is a presentation and admin client only. Policy decisions stay in
`focusd` and `focus-core`.

## Edit TOML From The GUI

Use the GUI Config page. It talks to the daemon through:

```text
config_file
write_config_file
```

The daemon validates TOML with `focus-core`, writes the config atomically, and
reloads the running policy state. Active Tier 1 hard-block rules cannot be
removed or weakened through the unprivileged GUI editor.

For the dev daemon, the edited file is:

```text
/tmp/blockuntu/config.toml
```

For production, the default config path is:

```text
/etc/blockuntu/config.toml
```

## Check The Dev Stack

From the GUI Admin page, the healthy dev path should show:

- Daemon socket: ok.
- Development runtime: ok.
- Firefox policy sandbox: ok.
- Chrome policy: ok.
- Native host manifest: ok.
- Chrome Native host: ok if Chrome/Chromium integration is installed.
- Unsupported browsers: ok when the mandatory Tier 1 hard app rule is loaded.
- Firefox extension and Chrome extension: ok only after each extension has sent
  a recent heartbeat through the daemon.

You can also test the daemon directly:

```bash
printf '%s' '{"jsonrpc":"2.0","id":1,"method":"status","params":{}}' \
  | socat - UNIX-CONNECT:/tmp/blockuntu/blockuntud.sock
```

Evaluate a URL:

```bash
printf '%s' '{"jsonrpc":"2.0","id":1,"method":"evaluate_url","params":{"url":"https://reddit.com/"}}' \
  | socat - UNIX-CONNECT:/tmp/blockuntu/blockuntud.sock
```

Expected result for Reddit, Twitch, and TikTok with the example config is a
hard-block decision.

## Stop The Dev Stack

Stop the daemon with `Ctrl+C` in the terminal running
`./scripts/start-dev-daemon.sh`.

The dev files can be removed when you want a clean state:

```bash
rm -rf /tmp/blockuntu
```

Do not remove Native Messaging manifests while a browser is running and expect
the current browser process to notice immediately. Restart Firefox and Chrome
after manifest changes.

## Verification Commands

From the repository root:

```bash
./scripts/verify-focus-core.sh
./scripts/verify-focusd.sh
./scripts/verify-native-host.sh
./scripts/verify-firefox-extension.sh
./scripts/verify-chrome-extension.sh
./scripts/verify-focus-gui.sh
```

These scripts are normal local build/test checks.

## Production Runtime Paths

The daemon defaults are:

| Purpose | Path |
| --- | --- |
| Config | `/etc/blockuntu/config.toml` |
| SQLite database | `/var/lib/blockuntu/blockuntu.sqlite3` |
| Daemon socket | `/run/blockuntu/blockuntud.sock` |
| Firefox policy | `/etc/firefox/policies/policies.json` |
| Chrome policy | `/etc/opt/chrome/policies/managed/blockuntu.json` |
| Chrome update manifest | `/usr/local/share/blockuntu/chrome-extension-updates.xml` |
| Extension XPI | `/home/christian/Desktop/HostFileModifier/browser-extension-firefox/BlocKuntu-Signed.xpi` |
| Hosts file | `/etc/hosts` |
| Native host binary | `/usr/local/bin/blockuntu-native` |
| Daemon binary | `/usr/local/bin/blockuntud` |
| Firefox Native Messaging manifest | `/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json` |
| Chrome Native Messaging manifest | `/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json` |
| Chromium Native Messaging manifest | `/etc/chromium/native-messaging-hosts/blockuntu_native.json` |

Production socket access is intended to be group-gated:

```text
SocketMode=0660
SocketUser=root
SocketGroup=blockuntu
```

The local desktop user must be a member of the `blockuntu` group for the GUI and
native host to reach the daemon socket without root privileges.

## Production Build

Build release binaries:

```bash
cargo build --manifest-path focusd/Cargo.toml --release
cargo build --manifest-path native-host/Cargo.toml --release
```

Build and package the Firefox extension:

```bash
cd browser-extension-firefox
npm install
npm run build
npm run package:xpi
```

Build and package the Chrome extension:

```bash
cd browser-extension-chrome
npm install
npm run build
npm run package:zip
```

Build the GUI:

```bash
cd focus-gui
npm install
npm run tauri -- build --no-bundle
```

## Production Manual Install Target

These commands describe the intended file layout. Review them before using them
on a real machine because this path writes to `/etc`, `/run`, `/var/lib`, and
system browser locations.

Create the socket group and add your desktop user:

```bash
sudo groupadd --system blockuntu
sudo usermod -aG blockuntu "$USER"
```

Log out and back in after changing group membership.

Install binaries and configuration:

```bash
sudo install -Dm755 focusd/target/release/blockuntud /usr/local/bin/blockuntud
sudo install -Dm755 native-host/target/release/blockuntu-native /usr/local/bin/blockuntu-native
sudo install -Dm644 packaging/deb/blockuntu.toml /etc/blockuntu/config.toml
sudo install -Dm644 packaging/native-messaging/blockuntu_native.json /usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json
sudo install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json /etc/opt/chrome/native-messaging-hosts/blockuntu_native.json
sudo install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json /etc/chromium/native-messaging-hosts/blockuntu_native.json
```

Install systemd units:

```bash
sudo install -Dm644 packaging/systemd/blockuntu.socket /etc/systemd/system/blockuntu.socket
sudo install -Dm644 packaging/systemd/blockuntu.service /etc/systemd/system/blockuntu.service
sudo install -Dm644 packaging/systemd/blockuntu-watchdog.service /etc/systemd/system/blockuntu-watchdog.service
sudo install -Dm644 packaging/systemd/blockuntu-hosts.path /etc/systemd/system/blockuntu-hosts.path
sudo install -Dm644 packaging/systemd/blockuntu-hosts.service /etc/systemd/system/blockuntu-hosts.service
```

Verify unit syntax before enabling:

```bash
systemd-analyze verify \
  packaging/systemd/blockuntu.socket \
  packaging/systemd/blockuntu.service \
  packaging/systemd/blockuntu-watchdog.service \
  packaging/systemd/blockuntu-hosts.path \
  packaging/systemd/blockuntu-hosts.service
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now blockuntu.socket
sudo systemctl enable --now blockuntu.service
sudo systemctl enable --now blockuntu-watchdog.service
sudo systemctl enable --now blockuntu-hosts.path
```

Check status:

```bash
systemctl status blockuntu.socket
systemctl status blockuntu.service
systemctl status blockuntu-watchdog.service
systemctl status blockuntu-hosts.path
```

The current production installer and `.deb` flow defer Firefox and Chrome policy
repair until the matching extension sends its first heartbeat. After that, the
daemon repairs the browser policy and hosts file periodically. The expected
Firefox policy force-installs the extension ID `blockuntu-poc@example.local`
from:

```text
/home/christian/Desktop/HostFileModifier/browser-extension-firefox/BlocKuntu-Signed.xpi
```

The expected Chrome policy force-installs the extension ID
`odedgejjcdilkoibeljkeohekonmdfea` from the local update manifest:

```text
/usr/local/share/blockuntu/chrome-extension-updates.xml
```

That update manifest points at:

```text
https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download
```

For `/etc/hosts`, production repair clears `chattr -i`, rewrites the managed
Tier 1 block if needed, and reapplies `chattr +i`. The GUI Admin view exposes
the Firefox policy, Chrome policy, hosts-file compliance, browser-extension
health, Chrome Native Messaging status, unsupported-browser hard-block status,
and an explicit start/stop enforcement control.

The daemon also injects a mandatory hard app rule for unsupported browsers. The
supported browsers are Firefox and Google Chrome. Other common browsers such as
Chromium, Brave, Edge, Opera, Vivaldi, LibreWolf, Waterfox, Epiphany, Falkon,
qutebrowser, Midori, Min, Nyxt, and Tor Browser are treated as Tier 1
application blocks by the process scanner.

Restart Firefox and Chrome after installing or changing production policy or
native host manifests.

## Production Gaps Before Lockdown

Before calling production complete, the following items should be finished and
tested end to end:

- Test `scripts/install-production.sh`, `scripts/uninstall-production.sh`, and
  `scripts/package-deb.sh` on clean disposable VMs.
- Re-run `systemd-analyze verify` on installed units and test the mutual
  watchdog behavior on a disposable machine.
- Confirm the target Firefox channel accepts the force-installed local XPI or
  add a signing/distribution step.
- Verify Chrome force-install on a disposable production-path install with the
  hosted CRX and local update manifest.
- Verify `/etc/hosts` repair behavior on a disposable machine, including
  preservation of unrelated hosts entries.
- Add production packaging for the GUI, not just `npm run tauri dev`.
- Implement `focus-cli` for admin/debug commands outside the GUI.
- Expand root-path integration tests or scripted dry-runs for `/etc`,
  `/var/lib/blockuntu`, `/run/blockuntu`, Firefox policy handling, and Chrome
  policy/update-manifest handling.
- Decide how strict production should be about stopping or uninstalling the
  watchdog units during maintenance.

Until those are complete, prefer the `/tmp/blockuntu` development workflow for
iteration and browser testing.
