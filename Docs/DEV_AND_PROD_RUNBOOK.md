# BlocKuntu Dev and Production Runbook

This document describes how to run the current BlocKuntu stack during
development, how the intended production installation should work, and what is
still missing before treating the production path as finished.

## Current Status

The development stack is usable today with temporary, non-root paths under
`/tmp/blockuntu`.

The production stack has the main pieces in place:

- `blockuntud` daemon with default root paths.
- Firefox Native Messaging bridge.
- Firefox WebExtension source and unsigned XPI packaging helper.
- Tauri GUI.
- systemd unit files under `packaging/systemd/`.
- Native Messaging manifest under `packaging/native-messaging/`.

The production path still needs an audited installer/uninstaller and a final
root-path verification pass before it should be used as a locked-down install.

## Prerequisites

Install the normal Linux development tools:

```bash
rustup default stable
```

Node/npm are needed for the GUI and Firefox extension. Firefox is needed for
extension testing. Tauri also needs the normal Linux WebKit/GTK development
packages for your distribution.

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
| Hosts sandbox | `/tmp/blockuntu/hosts` |
| Firefox Native Messaging manifest | `~/.mozilla/native-messaging-hosts/blockuntu_native.json` |

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

and writes the per-user Firefox Native Messaging manifest to:

```text
~/.mozilla/native-messaging-hosts/blockuntu_native.json
```

The wrapper forces the native host to connect to:

```text
/tmp/blockuntu/blockuntud.sock
```

It also passes a development-only revival command:

```text
./scripts/start-dev-daemon.sh
```

If Firefox relaunches `blockuntu-native`, or if the existing native host sees a
missing/stale dev socket, the native host starts that script and retries the
daemon request. The dev daemon script holds `/tmp/blockuntu/dev-daemon.lock` so
repeated heartbeats do not start competing daemons.

Restart Firefox after installing or changing this manifest. Firefox reads Native
Messaging manifests at process startup.

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

The extension ID is `blockuntu@example.local`, and the Native Messaging host
name is `blockuntu_native`.

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
- Native host manifest: ok.

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

Do not remove the Firefox Native Messaging manifest while Firefox is running and
expect the current browser process to notice immediately. Restart Firefox after
manifest changes.

## Verification Commands

From the repository root:

```bash
./scripts/verify-focus-core.sh
./scripts/verify-focusd.sh
./scripts/verify-native-host.sh
./scripts/verify-firefox-extension.sh
./scripts/verify-focus-gui.sh
```

The Firefox extension verification script has already been approved for this
workspace. The other scripts are normal local build/test checks.

## Production Runtime Paths

The daemon defaults are:

| Purpose | Path |
| --- | --- |
| Config | `/etc/blockuntu/config.toml` |
| SQLite database | `/var/lib/blockuntu/blockuntu.sqlite3` |
| Daemon socket | `/run/blockuntu/blockuntud.sock` |
| Firefox policy | `/etc/firefox/policies/policies.json` |
| Extension XPI | `/usr/local/share/blockuntu/BlocKuntu.xpi` |
| Hosts file | `/etc/hosts` |
| Native host binary | `/usr/local/bin/blockuntu-native` |
| Daemon binary | `/usr/local/bin/blockuntud` |
| Native Messaging manifest | `/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json` |

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

Build and package the extension:

```bash
cd browser-extension-firefox
npm install
npm run build
npm run package:xpi
```

Build the GUI:

```bash
cd focus-gui
npm install
npm run tauri -- build
```

## Production Manual Install Target

These commands describe the intended file layout. Review them before using them
on a real machine because this path writes to `/etc`, `/run`, `/var/lib`, and
system Firefox locations.

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
sudo install -Dm644 examples/blockuntu.toml /etc/blockuntu/config.toml
sudo install -Dm644 browser-extension-firefox/BlocKuntu.xpi /usr/local/share/blockuntu/BlocKuntu.xpi
sudo install -Dm644 packaging/native-messaging/blockuntu_native.json /usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json
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

The daemon repairs the Firefox policy on startup and periodically afterwards.
The expected policy force-installs the extension ID `blockuntu@example.local`
from:

```text
/usr/local/share/blockuntu/BlocKuntu.xpi
```

Restart Firefox after installing or changing the production policy or native
host manifest.

## Production Gaps Before Lockdown

Before calling production complete, the following items should be finished and
tested end to end:

- Add a real installer that performs all privileged file installation steps
  consistently.
- Add a deliberate uninstall/disable path that removes systemd units, Firefox
  policy, Native Messaging manifest, and managed hosts entries in a controlled
  way.
- Re-run `systemd-analyze verify` on installed units and test the mutual
  watchdog behavior on a disposable machine.
- Confirm the target Firefox channel accepts the force-installed local XPI or
  add a signing/distribution step.
- Verify `/etc/hosts` repair behavior on a disposable machine, including
  preservation of unrelated hosts entries.
- Add production packaging for the GUI, not just `npm run tauri dev`.
- Implement `focus-cli` for admin/debug commands outside the GUI.
- Expand root-path integration tests or scripted dry-runs for `/etc`,
  `/var/lib/blockuntu`, `/run/blockuntu`, and Firefox policy handling.
- Decide how strict production should be about stopping or uninstalling the
  watchdog units during maintenance.

Until those are complete, prefer the `/tmp/blockuntu` development workflow for
iteration and browser testing.
