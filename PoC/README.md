# BlocKuntu PoC

This folder contains a minimal Linux proof of concept that passes Firefox URL
events through a Native Messaging host into a root-managed Unix Domain Socket
backend.

## Layout

```text
PoC/
  Cargo.toml
  blockuntud/                  # root backend daemon
  blockuntu-native/            # unprivileged Firefox Native Messaging host
  extension/                   # Firefox WebExtension
  native-messaging/            # Firefox native host manifest
  systemd/                     # system service units
```

## Build

```sh
cargo build --release
sudo install -Dm755 target/release/blockuntud /usr/local/bin/blockuntud
sudo install -Dm755 target/release/blockuntu-native /usr/local/bin/blockuntu-native
sudo install -Dm644 extension/BlocKuntu-PoC.xpi \
  /usr/local/share/blockuntu/BlocKuntu-PoC.xpi
```

## Install Systemd Units

```sh
sudo systemctl unmask blockuntu.service blockuntu-watchdog.service
sudo install -Dm644 systemd/blockuntu.service /etc/systemd/system/blockuntu.service
sudo install -Dm644 systemd/blockuntu-watchdog.service /etc/systemd/system/blockuntu-watchdog.service
sudo systemctl daemon-reload
sudo systemctl enable --now blockuntu.service blockuntu-watchdog.service
```

The daemon listens on `/run/blockuntu/blockuntud.sock`. It also enforces
`/etc/firefox/policies/policies.json` once at startup and then every second. If
the Firefox enterprise policy file is missing, malformed, or does not exactly
match the BlocKuntu extension policy, the daemon rewrites it with mode `0644`.
Because `blockuntud` is expected to run as root, the repaired file is root-owned.

The enforced policy also hardens Firefox around the extension: it blocks
`about:config`, disables Troubleshoot Mode, removes Private Browsing with
`PrivateBrowsingModeAvailability: 1`, and locks
`extensions.quarantinedDomains.enabled` to `false`. That last preference disables
Firefox's quarantined-domain add-on protection globally, so keep it only if this
PoC must run the extension on Mozilla-restricted sites.

The Firefox extension sends a Native Messaging heartbeat every 15 seconds. The
daemon logs `Firefox extension active` when heartbeats are flowing and
`Firefox extension inactive` if no heartbeat arrives for 30 seconds.

For this PoC, the daemon sets the socket mode to `0666` so an ordinary
desktop-user Native Messaging host can connect. A production build should
replace that with a dedicated group or a brokered permission model.

## Install Firefox Native Messaging Manifest

For a per-user Firefox install:

```sh
mkdir -p ~/.mozilla/native-messaging-hosts
cp native-messaging/blockuntu_native.json ~/.mozilla/native-messaging-hosts/blockuntu_native.json
```

For a system-wide Firefox install:

```sh
sudo install -Dm644 native-messaging/blockuntu_native.json \
  /usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json
```

The Native Messaging manifest uses:

```json
{
  "name": "blockuntu_native",
  "path": "/usr/local/bin/blockuntu-native",
  "allowed_extensions": ["blockuntu-poc@example.local"]
}
```

The extension manifest uses the matching Firefox ID:
`blockuntu-poc@example.local`.

## Firefox Extension Enforcement

The enforced Firefox policy force-installs:

```text
file:///usr/local/share/blockuntu/BlocKuntu-PoC.xpi
```

After the daemon has started, verify the policy with:

```sh
sudo test -f /etc/firefox/policies/policies.json
sudo stat -c '%U %G %a %n' /etc/firefox/policies/policies.json
sudo jq . /etc/firefox/policies/policies.json
```

Restart Firefox to let enterprise policies install the extension. During
extension development, you can still load `extension/manifest.json` through
`about:debugging#/runtime/this-firefox`, but the enforced XPI is the path used
by the daemon policy loop.

The policy uses Firefox's current `ExtensionSettings.private_browsing` key for
the extension and disables the Private Browsing feature separately. Older
`run_in_private_browsing` policy examples are stale for current Firefox.

When you change files in `extension/`, regenerate and sign the XPI before
installing it to `/usr/local/share/blockuntu/BlocKuntu-PoC.xpi`. The
`extension/Archive.zip` source package is refreshed from the current loose
extension files for that signing step.

## Smoke Test

With the services running and the extension loaded:

```sh
journalctl -u blockuntu.service -f
```

Navigate to `https://instagram.com/` or `https://twitter.com/`. The daemon should
log:

```text
Received evaluation request for: https://instagram.com/
```

The extension then redirects the top-level tab to its packaged
`blocked.html` page. Other URLs receive `{"action":"allow"}` from the daemon.

The daemon should also log extension state transitions:

```text
Firefox extension active (blockuntu-poc@example.local, version 0.1.0)
Firefox extension inactive: last heartbeat was 31 second(s) ago
```

## Extension Inactive Action

By default, the daemon only logs after the extension has been inactive for two
minutes:

```text
Firefox extension has been inactive for 120 second(s); configured action is log-only
```

To make the two-minute inactive action power off the machine, set this
environment variable for `blockuntu.service`:

```ini
[Service]
Environment=BLOCKUNTU_EXTENSION_INACTIVE_ACTION=poweroff
```

Then reload systemd and let the service restart under its own `Restart=always`
policy:

```sh
sudo systemctl daemon-reload
sudo systemctl kill -s SIGTERM blockuntu.service
```

## Administrative Uninstall / Stop

The daemon includes a deliberate privileged cleanup path:

```sh
sudo /usr/local/bin/blockuntud --uninstall
```

The uninstall routine writes temporary runtime systemd drop-ins under
`/run/systemd/system/` to disable the PoC restart and manual-stop guards, reloads
systemd, disables and stops `blockuntu.service` and
`blockuntu-watchdog.service`, removes their installed unit files, masks both unit
names so they cannot come back on reboot, removes
`/etc/firefox/policies/policies.json`, and clears `/run/blockuntu/`. It treats
already-unloaded units as a successful uninstall state.

To reinstall after uninstalling, unmask the units before enabling them:

```sh
sudo systemctl unmask blockuntu.service blockuntu-watchdog.service
```

## Service Relationship Notes

`blockuntu.service` and `blockuntu-watchdog.service` intentionally use mutual
`BindsTo=`, `Wants=`, `Restart=always`, `RestartSec=0`, and
`RefuseManualStop=yes`. A `systemctl stop` request is refused; a process kill or
crash is restarted immediately; and each unit asks systemd to keep the companion
unit active. This is a PoC self-healing arrangement, not a security boundary
against a root user who can edit unit files, mask units, or remove binaries.
