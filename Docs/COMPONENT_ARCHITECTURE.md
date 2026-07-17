# BlocKuntu Component Architecture

BlocKuntu is split into a policy library, a privileged daemon, unprivileged
desktop/browser clients, and Linux packaging glue. The daemon is the single
source of truth. The GUI, browser extensions, and Native Messaging bridge do
not make durable policy decisions.

## High-Level Flow

```text
Firefox tab navigation
  -> browser-extension-firefox
  -> Firefox Native Messaging
  -> native-host / blockuntu-native
  -> Unix domain socket
  -> focusd / blockuntud
  -> focus-core policy engine
  -> SQLite runtime state + TOML config

Chrome tab navigation
  -> browser-extension-chrome
  -> Chrome Native Messaging
  -> native-host / blockuntu-native
  -> Unix domain socket
  -> focusd / blockuntud
  -> focus-core policy engine
  -> SQLite runtime state + TOML config

focus-gui / Tauri
  -> Tauri Rust commands
  -> Unix domain socket
  -> focusd / blockuntud
  -> focus-core policy engine

focusd / blockuntud
  -> Firefox enterprise policy repair
  -> managed hosts block repair
  -> process scanner and kill logging
  -> systemd socket/watchdog/path integration
```

## Component Responsibilities

| Component | Responsibility | Privilege level |
| --- | --- | --- |
| `focus-core` | Config parsing, validation, SQLite migrations, URL/app policy evaluation, unlocks, visits, events, heartbeats | Library only |
| `focusd` | Root daemon, JSON-RPC API, system enforcement, config writes, policy repair, hosts repair, process scanning | Privileged |
| `native-host` | Browser Native Messaging framing and Unix socket forwarding | Unprivileged |
| `browser-extension-firefox` | Browser navigation interception, heartbeat, fail-closed redirect to `blocked.html` | Unprivileged browser extension |
| `browser-extension-chrome` | Chrome/Chromium MV3 navigation interception, heartbeat, fail-closed redirect to `blocked.html` | Unprivileged browser extension |
| `focus-gui` | Tauri/Svelte presentation, admin/debug views, daemon-mediated TOML editing | Unprivileged desktop app |
| `packaging/systemd` | Socket activation, watchdog companion, hosts path watcher | systemd/root |
| `packaging/native-messaging` | System Firefox and Chrome native-host manifests | Browser/system install |

## Trust Boundary

Only `focusd` is trusted to mutate system state.

Allowed privileged side effects:

- Write Firefox enterprise policy at `/etc/firefox/policies/policies.json`.
  Firefox Flatpak uses a per-user Flatpak `org.mozilla.firefox.systemconfig`
  extension policy instead.
- Write Chrome managed policy at
  `/etc/opt/chrome/policies/managed/blockuntu.json` and the local Chrome
  update manifest at `/usr/local/share/blockuntu/chrome-extension-updates.xml`.
- Repair the managed BlocKuntu section in `/etc/hosts`.
- Maintain `/run/blockuntu/blockuntud.sock` through systemd socket activation.
- Kill forbidden processes found through `/proc`.
- Write service state, events, visits, unlocks, and heartbeats to SQLite.
- Atomically write validated TOML config changes.

Not allowed in unprivileged components:

- The GUI must not evaluate allow/block decisions locally.
- The extension must not maintain its own durable rules.
- The Native Messaging host must not bypass the daemon or invent decisions.
- Temporary unlocks must go through daemon RPC and `focus-core` validation.

## Data Model

BlocKuntu has two state layers.

Durable configuration:

```text
TOML config file
  -> rules
  -> URL patterns
  -> schedules
  -> allowances
  -> app definitions
  -> unlock constraints
```

Runtime state:

```text
SQLite database
  -> rules
  -> rule_patterns
  -> apps
  -> schedules
  -> allowances
  -> unlocks
  -> visits
  -> events
  -> heartbeats
  -> service_state
```

`focus-core` owns the schema migration and policy behavior. `focusd` owns where
the files live and when system repairs run.

## Policy Evaluation

URL evaluation starts in the browser extension for timing reasons, but the
decision is made by `focus-core` through `focusd`.

```text
webNavigation.onBeforeNavigate
  -> extension sends evaluate_url
  -> native-host forwards JSON-RPC
  -> focusd builds EvaluationContext
  -> focus-core evaluates:
       Tier 1 hard block
       Tier 2 controlled access
       active unlocks
       weekly schedule windows
       daily allowance usage
  -> focusd returns allow or block
  -> extension allows navigation or redirects to blocked.html
```

Tier 1 hard blocks are strict. Once active, the unprivileged GUI editor is not
allowed to remove or weaken them.

Tier 2 controlled access can be unlocked only through `request_unlock`, which
enforces:

- Fixed two-minute duration.
- One global unlock per rolling hour.
- A unique reason containing at least 20 letters.
- Target matching against controlled-access rules.
- Rejection before unlock accounting when the target is covered by Detox.

## Heartbeat Flow

The extension sends periodic heartbeat messages through the same Native
Messaging route used for URL checks:

```text
browser-extension-firefox
  -> {"method":"extension_heartbeat","params":{"component":"firefox_extension"}}
  -> native-host
  -> focusd
  -> SQLite heartbeats/events
  -> acknowledgement back to extension

browser-extension-chrome
  -> {"method":"extension_heartbeat","params":{"component":"chrome_extension"}}
  -> native-host
  -> focusd
  -> SQLite heartbeats/events
  -> acknowledgement back to extension
```

The extension starts unhealthy and fail-closed. If it does not receive heartbeat
acknowledgements, it blocks all top-level HTTP/HTTPS navigations. This is
intentional because a missing daemon/native-host chain means the browser cannot
prove policy enforcement is alive.

## JSON-RPC API

The daemon accepts one JSON request per Unix socket connection. Current methods:

| Method | Caller | Purpose |
| --- | --- | --- |
| `status` | GUI, debug tools | Basic daemon/config counts |
| `config_snapshot` | GUI | Structured config for dashboards |
| `config_file` | GUI | Load raw TOML and path |
| `write_config_file` | GUI | Validate, atomically write, and reload TOML |
| `log_summary` | GUI | Parse the plain daemon event log into total and event-kind counts |
| `evaluate_url` | Extension, GUI probe | Return allow/block decision for a URL |
| `request_unlock` | GUI | Request a Tier 2 controlled-access unlock |
| `record_visit_start` | Extension | Start allowed visit tracking |
| `record_visit_heartbeat` | Extension | Keep visit tracking alive |
| `record_visit_end` | Extension | End visit tracking |
| `extension_heartbeat` | Extension | Prove extension/native-host path is alive |
| `extension_status` | GUI | Report Firefox or Chrome extension heartbeat freshness |

The native host also accepts legacy extension messages for compatibility and
translates them into daemon JSON-RPC.

## System Enforcement

### Firefox Enterprise Policy

`focusd` computes and repairs the Firefox policy JSON. The current expected
policy:

- Force-installs the signed extension ID `{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}`.
- Uses the configured XPI path, defaulting to
  `/home/christian/Desktop/HostFileModifier/browser-extension-firefox/BlocKuntu-Signed.xpi`.
- Keeps private browsing available and enables the extension there.
- Blocks `about:config`, `about:profiles`, and `about:support`.
- Disables developer tools and safe mode workarounds.
- Keeps `about:addons` available.

The dev daemon writes this policy to the sandbox path:

```text
/tmp/blockuntu/firefox/policies.json
```

Production uses:

```text
/etc/firefox/policies/policies.json
```

Firefox Flatpak uses:

```text
$XDG_DATA_HOME/flatpak/extension/org.mozilla.firefox.systemconfig/<arch>/stable/policies/policies.json
```

which is mounted inside the sandbox at:

```text
/app/etc/firefox/policies/policies.json
```

Production installers can defer Firefox and Chrome policy repair until the
matching extension sends its first Native Messaging heartbeat. This lets the
user install and enable the extension manually first, then makes the daemon
write managed policy once the integration is proven alive.

### Hosts Fallback

`focusd` renders a managed block in the hosts file for Tier 1 domain
patterns:

```text
# BEGIN BLOCKUNTU MANAGED
...
# END BLOCKUNTU MANAGED
```

Only domain rules can be represented in `/etc/hosts`. Path-level URL patterns
and exact URL rules are browser-level enforcement only.

For the production `/etc/hosts` path, `focusd` clears the immutable flag before
repairing the managed block and reapplies `chattr +i` afterwards. Development
hosts sandboxes skip immutability unless `--hosts-immutable` is passed.

Development uses:

```text
/tmp/blockuntu/hosts
```

Production uses:

```text
/etc/hosts
```

### Chrome Policy And Native Messaging

The Chrome/Chromium extension uses the same `blockuntu_native` host name as
Firefox, but Chrome requires `allowed_origins` in the manifest. The fixed
extension ID for the hosted CRX is:

```text
odedgejjcdilkoibeljkeohekonmdfea
```

Development manifest paths:

```text
~/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json
~/.config/chromium/NativeMessagingHosts/blockuntu_native.json
```

Production manifest paths:

```text
/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json
/etc/chromium/native-messaging-hosts/blockuntu_native.json
```

`focusd` also writes Chrome managed policy for Google Chrome:

```text
/etc/opt/chrome/policies/managed/blockuntu.json
```

The policy sets `ExtensionInstallForcelist` and `ExtensionSettings` for
`odedgejjcdilkoibeljkeohekonmdfea`, with `override_update_url` enabled. The
policy update URL points at a local update manifest:

```text
/usr/local/share/blockuntu/chrome-extension-updates.xml
```

That XML contains the hosted CRX codebase:

```text
https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download
```

The Chrome CRX signing key determines the extension ID. Repackaging with a new
key changes the ID and requires updating Chrome policy and Native Messaging
origins.

### Process Scanner

The daemon scans `/proc` for configured forbidden applications. Matching can use:

- `/proc/[pid]/exe` for executable path checks.
- the executable basename from `/proc/[pid]/exe`.
- `/proc/[pid]/comm` for command-name checks.
- desktop ids discovered from launch environment/cmdline hints.
- window titles discovered through `wmctrl -lp` when an X11-compatible window
  list is available.

When a forbidden process is found, the daemon kills it and writes an
`app_killed` event to SQLite. Window-title matching is a fallback signal; it is
not expected to work generically on Wayland compositors.

The daemon injects a mandatory hard app rule for unsupported browsers so users
cannot bypass browser-extension enforcement by switching engines. Firefox and
Google Chrome are the supported browser paths. Chromium, Brave, Edge, Opera,
Vivaldi, LibreWolf, Waterfox, Epiphany, Falkon, qutebrowser, Midori, Min, Nyxt,
and Tor Browser are treated as Tier 1 application blocks.

Strict mode also protects supported browsers. When Firefox or Chrome is newly
observed, `focusd` records a browser-session start and allows at least 60
seconds for a heartbeat produced by that launch. A heartbeat from a previous
browser session does not satisfy this requirement. Once a current-session
heartbeat has arrived, a missing or stale heartbeat beyond the configured
grace period causes `focusd` to terminate that browser and record a
`browser_killed_extension_stale` event. The default ongoing-heartbeat grace
period is 30 seconds. The stronger network fallback remains future work; see
`Docs/TODO.md`.

The GUI reports this lifecycle as browser closed, starting, connected, or
needs attention. A closed browser is neutral: it does not need a heartbeat.

## Development Connections

Development uses explicitly bound sockets and user-local browser integration:

```text
/tmp/blockuntu/blockuntud.sock
~/.mozilla/native-messaging-hosts/blockuntu_native.json
~/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json
~/.config/chromium/NativeMessagingHosts/blockuntu_native.json
~/.local/share/blockuntu/blockuntu-native-dev
```

The dev native-host wrapper runs:

```text
blockuntu-native \
  --socket /tmp/blockuntu/blockuntud.sock \
  --revive-command ./scripts/start-dev-daemon.sh
```

This avoids root-only paths while testing extension behavior. The revival
command is development-only; production survival is handled by systemd units.

## Production Connections

Production is designed around systemd socket activation:

```text
/run/blockuntu/blockuntud.sock
```

The socket unit owns permissions:

```text
SocketMode=0660
SocketUser=root
SocketGroup=blockuntu
```

The production native host manifests point Firefox and Chrome at:

```text
/usr/local/bin/blockuntu-native
```

The native host defaults to:

```text
/run/blockuntu/blockuntud.sock
```

The GUI also auto-detects the production socket first, then falls back to the
development socket if it exists.

## systemd Units

The packaging directory contains:

| Unit | Purpose |
| --- | --- |
| `blockuntu.socket` | Owns `/run/blockuntu/blockuntud.sock` and socket permissions |
| `blockuntu.service` | Runs `/usr/local/bin/blockuntud serve` |
| `blockuntu-watchdog.service` | Companion service that repeatedly restarts the socket/service pair |
| `blockuntu-hosts.path` | Watches `/etc/hosts` for changes |
| `blockuntu-hosts.service` | Runs `/usr/local/bin/blockuntud repair-hosts` |

`blockuntu.service` and `blockuntu-watchdog.service` are intentionally linked
with `BindsTo=` and `Restart=always`.

## Operational Flows

### Normal Browser Navigation

```text
Firefox/Chrome navigation
  -> extension checks heartbeat freshness
  -> extension asks daemon to evaluate URL
  -> focus-core returns Decision
  -> extension redirects blocked navigations to blocked.html
```

### Manual Unlock

```text
GUI request
  -> request_unlock RPC
  -> focus-core validates target and constraints
  -> active Detox rejects the request without consuming reason/quota
  -> SQLite unlock row
  -> future evaluate_url calls may allow the matched Tier 2 rule until expiry
```

### Config Edit

```text
GUI Config page
  -> config_file RPC
  -> user edits TOML
  -> write_config_file RPC
  -> focus-core parses and validates TOML
  -> focusd checks active hard-block preservation
  -> focusd writes file atomically
  -> focusd reloads core config
  -> event logged as config_updated
```

### System Repair

```text
focusd startup and repair loop
  -> verify Firefox policy, or defer until first heartbeat
  -> repair policy if missing or changed after policy repair is active
  -> verify Chrome policy and local update manifest, or defer until first heartbeat
  -> repair Chrome policy/update manifest if missing or changed after policy repair is active
  -> verify managed hosts block
  -> repair hosts block if missing or changed
```

## Current Production Gaps

The architecture is in place, but a few pieces should be completed before a real
locked-down deployment:

- `focus-cli` is not implemented yet.
- Production install, uninstall, and initial `.deb` packaging exist, but the
  `.deb` still needs clean-VM acceptance testing.
- Production Firefox XPI signing/installability needs final verification for
  the target Firefox channel.
- The watchdog behavior needs disposable-machine validation after unit install.
- Full root-path integration tests are still missing.
- GUI update flow needs final packaging decisions.
