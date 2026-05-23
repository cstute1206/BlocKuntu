# BlocKuntu

BlocKuntu is a Linux focus and productivity blocker. The current productionized
pieces in this repository are `focus-core`, the shared Rust policy engine, and
`focusd`, the privileged daemon that exposes the local JSON-RPC API and owns
root-managed enforcement hooks.

The older proof of concept still lives under `PoC/`.

## Repository Layout

```text
.
├── focus-core/                 # Shared Rust policy/config/database library
├── focusd/                     # Privileged daemon binary crate
├── focus-gui/                  # Tauri v2 + Svelte desktop frontend
├── native-host/                # Firefox Native Messaging bridge
├── browser-extension-firefox/   # Firefox WebExtension TypeScript source
├── packaging/systemd/           # Production systemd units
├── packaging/native-messaging/  # Firefox native-host manifest
├── scripts/
│   ├── verify-focus-core.sh     # Clean build + test harness
│   ├── verify-focusd.sh         # Clean daemon build + test harness
│   ├── verify-focus-gui.sh      # Tauri/Svelte frontend verification
│   ├── verify-native-host.sh    # Clean native-host build + test harness
│   ├── start-dev-daemon.sh      # Non-root development daemon on /tmp/blockuntu
│   ├── install-dev-native-host.sh
│   └── verify-firefox-extension.sh
├── PoC/                         # Earlier Firefox/native-host/root-daemon PoC
└── Docs/                        # Notes and design material
```

## Detailed Documentation

- [Dev and Production Runbook](Docs/DEV_AND_PROD_RUNBOOK.md): how to start the
  dev daemon, install the dev Native Messaging host, load the Firefox extension,
  run the GUI, and what remains before a production install is ready.
- [Component Architecture](Docs/COMPONENT_ARCHITECTURE.md): how `focus-core`,
  `focusd`, `native-host`, the Firefox extension, the GUI, and systemd packaging
  connect to each other.

## What `focus-core` Does

`focus-core` is a library crate. It does not run as a daemon and it does not
edit system files. It provides:

- TOML configuration parsing and validation.
- SQLite runtime database migration.
- URL policy evaluation for Tier 1 hard blocks and Tier 2 controlled access.
- Schedule checks, daily allowance accounting, unlock cooldowns, maximum
  session limits, and hourly unlock quotas.
- Runtime records for unlocks, visits, events, heartbeats, and service state.

Privileged behavior such as `/etc/hosts` repair, Firefox enterprise policy
writing, process killing, socket binding, and systemd watchdogs belongs in
`focusd`.

## What `focusd` Does

`focusd` builds the `blockuntud` daemon. It:

- Loads the TOML policy config and SQLite runtime database through `focus-core`.
- Serves local JSON-RPC requests over a Unix domain socket.
- Supports systemd socket activation for `/run/blockuntu/blockuntud.sock`.
- Repairs Firefox enterprise policy at `/etc/firefox/policies/policies.json`.
- Repairs the BlocKuntu managed block inside `/etc/hosts`.
- Provides process-scanning primitives based on `/proc/[pid]/exe` and
  `/proc/[pid]/comm`.
- Persists URL block events and Firefox extension heartbeats in SQLite.

The daemon is root-oriented. Its tests use temporary files and do not modify
system files.

## What `focus-gui` Does

`focus-gui` is a Tauri v2 desktop frontend built with Svelte and TypeScript. It
does not evaluate policies locally. The Svelte UI calls Tauri commands, and the
Rust side talks to `focusd` over the Unix socket.

It includes:

- Dashboard with daemon, rule, schedule, allowance, and health summaries.
- Blocks view for hard and controlled-access rule inspection.
- Config view for daemon-mediated TOML editing with validation and active
  Tier 1 hard-block protection.
- Weekly schedule grid.
- Allowance overview.
- Statistics from daemon event logs.
- Admin/debug view with systemd/socket/policy/native-host checks and raw
  JSON-RPC execution.

## What `native-host` Does

`native-host` builds the `blockuntu-native` executable used by Firefox Native
Messaging. It is unprivileged and has no policy authority. It:

- Maintains the persistent stdin/stdout Native Messaging session with Firefox.
- Reads and writes Firefox Native Messaging length-prefixed JSON frames.
- Connects to the daemon Unix socket as a member of the `blockuntu` group.
- Translates current PoC messages like `{"url": "..."}` into daemon JSON-RPC.
- Translates extension heartbeats into the daemon `extension_heartbeat` method.
- Passes raw JSON-RPC through unchanged for the future TypeScript extension.
- Fails closed from a privilege perspective: if the daemon is unavailable, it
  returns an error-shaped browser response and never makes local decisions.

## What `browser-extension-firefox` Does

The Firefox extension is written in TypeScript and compiles to
`dist/background.js`. It:

- Maintains daemon health through Native Messaging heartbeat acknowledgements.
- Starts in fail-closed mode until a heartbeat acknowledgement arrives.
- Blocks all top-level HTTP/HTTPS navigation when heartbeat state is stale or
  unavailable.
- Sends URL evaluations to `focusd` through `native-host`.
- Records visit start, visit heartbeat, and visit end events for allowed pages.
- Redirects blocked pages to `blocked.html`.

## Prerequisites

Install a recent Rust toolchain:

```bash
rustup default stable
```

You also need normal native build tooling available on Linux, such as `cc`,
`pkg-config`, and libc headers. SQLite is built through the bundled
`rusqlite` feature, so a system SQLite development package is not required.

## Run Locally

Start a non-root development daemon from the repository root:

```bash
./scripts/start-dev-daemon.sh
```

This uses:

- Config: `/tmp/blockuntu/config.toml`
- Database: `/tmp/blockuntu/blockuntu.sqlite3`
- Socket: `/tmp/blockuntu/blockuntud.sock`
- Sandbox Firefox policy file: `/tmp/blockuntu/firefox/policies.json`
- Sandbox hosts file: `/tmp/blockuntu/hosts`

In a second terminal, start the Tauri GUI:

```bash
cd focus-gui
npm install
npm run tauri dev
```

The GUI socket field defaults to auto-detection. It tries the production socket
`/run/blockuntu/blockuntud.sock` first and then the development socket
`/tmp/blockuntu/blockuntud.sock`.

For the development daemon, you can also set the socket field in the top bar
explicitly to:

```text
/tmp/blockuntu/blockuntud.sock
```

The Config tab loads and saves the daemon TOML through JSON-RPC. Saves are
validated by `focus-core`, written atomically by `focusd`, and reloaded in the
daemon process immediately. Active Tier 1 hard-block rules cannot be removed or
modified from this unprivileged editor.

For Firefox extension testing against the development daemon, install the
per-user Native Messaging manifest:

```bash
./scripts/install-dev-native-host.sh
```

Restart Firefox after installing or updating that manifest.

## Build

From the repository root:

```bash
cd focus-core
cargo build
```

Build the daemon:

```bash
cd focusd
cargo build
```

Build the native host:

```bash
cd native-host
cargo build
```

Build the Tauri frontend web assets:

```bash
cd focus-gui
npm install
npm run build
```

Build the Firefox extension:

```bash
cd browser-extension-firefox
npm install
npm run build
```

For a release build:

```bash
cd focus-core
cargo build --release
```

```bash
cd focusd
cargo build --release
```

```bash
cd native-host
cargo build --release
```

The compiled library artifacts are written under `focus-core/target/`.

## Test

Run the full `focus-core` test suite:

```bash
cd focus-core
cargo test --all-targets
```

Run the full `focusd` test suite:

```bash
cd focusd
cargo test --all-targets
```

Run the full `native-host` test suite:

```bash
cd native-host
cargo test --all-targets
```

Check the Tauri frontend:

```bash
cd focus-gui
npm run verify
```

Type-check and validate the Firefox extension:

```bash
cd browser-extension-firefox
npm run verify
```

Run formatting checks:

```bash
cd focus-core
cargo fmt --check
```

## One-Command Verification

From the repository root:

```bash
./scripts/verify-focus-core.sh
```

For the daemon:

```bash
./scripts/verify-focusd.sh
```

For the Tauri frontend:

```bash
./scripts/verify-focus-gui.sh
```

For the native host:

```bash
./scripts/verify-native-host.sh
```

For the Firefox extension:

```bash
./scripts/verify-firefox-extension.sh
```

This script:

1. Enters `focus-core/`.
2. Runs `cargo clean`.
3. Runs `cargo fmt --check`.
4. Runs `cargo test --all-targets`.

Use these before integrating changes into other crates.

## Running `focusd` for Development

`focus-core` is a library and is not run directly. `focusd` is executable.

For a non-root development run, use the helper script:

```bash
./scripts/start-dev-daemon.sh
```

It uses only temporary paths under `/tmp/blockuntu`, including the daemon socket,
SQLite database, Firefox policy sandbox, and hosts sandbox.

The equivalent manual command is:

```bash
mkdir -p /tmp/blockuntu
cp examples/blockuntu.toml /tmp/blockuntu/config.toml

cd focusd
cargo run -- \
  --config /tmp/blockuntu/config.toml \
  --database /tmp/blockuntu/blockuntu.sqlite3 \
  --socket /tmp/blockuntu/blockuntud.sock \
  --firefox-policy /tmp/blockuntu/firefox/policies.json \
  --hosts /tmp/blockuntu/hosts \
  --dev-bind-socket \
  serve
```

The production service should use systemd socket activation instead of
`--dev-bind-socket`.

If you are testing the Firefox extension against the dev daemon, install the
per-user dev Native Messaging manifest:

```bash
./scripts/install-dev-native-host.sh
```

Then restart Firefox and reload the extension. The dev manifest points
`blockuntu_native` at the local native host binary and forces it to use
`/tmp/blockuntu/blockuntud.sock`.

Check config/database initialization without serving:

```bash
cd focusd
cargo run -- \
  --config /tmp/blockuntu/config.toml \
  --database /tmp/blockuntu/blockuntu.sqlite3 \
  check
```

## Minimal Config Example

`focus-core` expects TOML like this:

```toml
[[allowances]]
id = "social-daily"
name = "Social daily allowance"
daily_minutes = 30

[[schedules]]
id = "work-hours"
name = "Work hours"

[[schedules.windows]]
weekday = "mon"
start = "09:00"
end = "17:00"

[[rules]]
id = "instagram-hard"
name = "Instagram hard block"
tier = "hard"
patterns = [
  { kind = "domain", value = "instagram.com", match_subdomains = true }
]

[[rules]]
id = "reddit-hard"
name = "Reddit hard block"
tier = "hard"
patterns = [
  { kind = "domain", value = "reddit.com", match_subdomains = true },
  { kind = "domain", value = "redd.it", match_subdomains = true }
]

[[rules]]
id = "twitch-hard"
name = "Twitch hard block"
tier = "hard"
patterns = [
  { kind = "domain", value = "twitch.tv", match_subdomains = true }
]

[[rules]]
id = "tiktok-hard"
name = "TikTok hard block"
tier = "hard"
patterns = [
  { kind = "domain", value = "tiktok.com", match_subdomains = true }
]

[[rules]]
id = "youtube-controlled"
name = "YouTube controlled access"
tier = "controlled_access"
schedule_ids = ["work-hours"]
allowance_id = "social-daily"
patterns = [
  { kind = "domain", value = "youtube.com", match_subdomains = true }
]

[rules.unlock_policy]
max_session_minutes = 10
cooldown_minutes = 30
max_unlocks_per_hour = 2
```

## Minimal Rust Usage

```rust
use chrono::Local;
use focus_core::{Database, EvaluationContext, Config, evaluate_url};

let config = Config::from_toml_str(include_str!("blockuntu.toml"))?;
let database = Database::open("blockuntu.sqlite3")?;
let now = Local::now().fixed_offset();
let context = EvaluationContext::new(&config, &database, now);

let decision = evaluate_url("https://youtube.com/watch?v=abc", &context);
println!("{decision:?}");
# Ok::<(), focus_core::Error>(())
```

## Minimal JSON-RPC Usage

With `blockuntud` serving on a Unix socket:

```bash
printf '%s' '{"jsonrpc":"2.0","id":1,"method":"evaluate_url","params":{"url":"https://youtube.com/"}}' \
  | socat - UNIX-CONNECT:/tmp/blockuntu/blockuntud.sock
```

Example methods:

```text
status
evaluate_url
request_unlock
record_visit_start
record_visit_heartbeat
record_visit_end
extension_heartbeat
```

## Native Host Development

The native host talks to `/run/blockuntu/blockuntud.sock` by default. For local
development against a temporary daemon socket:

```bash
cd native-host
cargo run -- --socket /tmp/blockuntu/blockuntud.sock
```

Firefox launches this binary through a native messaging manifest, not manually.
The production manifest is:

```text
packaging/native-messaging/blockuntu_native.json
```

For a per-user Firefox install during development:

```bash
mkdir -p ~/.mozilla/native-messaging-hosts
cp packaging/native-messaging/blockuntu_native.json \
  ~/.mozilla/native-messaging-hosts/blockuntu_native.json
```

For system-wide installation:

```bash
sudo install -Dm755 native-host/target/release/blockuntu-native \
  /usr/local/bin/blockuntu-native
sudo install -Dm644 packaging/native-messaging/blockuntu_native.json \
  /usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json
```

## Tauri GUI Development

```bash
cd focus-gui
npm install
npm run tauri dev
```

The GUI auto-detects `/run/blockuntu/blockuntud.sock` first, then
`/tmp/blockuntu/blockuntud.sock`. For local daemon testing, you can leave the
socket field empty or set it explicitly to `/tmp/blockuntu/blockuntud.sock`.

## Firefox Extension Development

```bash
cd browser-extension-firefox
npm install
npm run build
```

Load `browser-extension-firefox/manifest.json` from
`about:debugging#/runtime/this-firefox`.

Create an unsigned XPI archive when needed:

```bash
cd browser-extension-firefox
npm run build
npm run package:xpi
```

## Current Verification Status

The latest verification run passed:

```text
cargo fmt --check
cargo test --all-targets
focus-core: 8 integration tests passed
focusd: daemon/module tests passed
focus-gui: Svelte check, Vite build, and Tauri cargo check passed
native-host: framing/protocol/socket bridge tests passed
browser-extension-firefox: TypeScript build and manifest checks passed
```

## Next Implementation Step

The next production module should be `focus-cli`, so there is an admin/debug
tool for checking daemon status, repairing installation state, and inspecting
SQLite events.
