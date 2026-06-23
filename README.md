# BlocKuntu

BlocKuntu is a Linux focus blocker. It combines a privileged daemon, browser
extensions, a Native Messaging bridge, and a Tauri/Svelte GUI to enforce website
and application blocking rules from one local policy store.

The normal production path is the Debian package built by
`./scripts/package-deb.sh`. Browser extensions are installed and enabled by the
user; the daemon repairs managed browser policy only after the matching
extension has sent its first heartbeat through Native Messaging.

The older proof of concept still exists under `PoC/`, but the active code lives
in the `focus-*`, `native-host`, browser-extension, packaging, and script
directories described below.

## Documentation

- [Production installation](Docs/INSTALLATION.md): build a current `.deb`,
  install it on Ubuntu/Debian, configure Firefox Snap/Flatpak support, and
  understand production runtime paths.
- [Implemented features](Docs/FEATURES.md): inventory of current website,
  application, GUI, daemon, browser, packaging, and enforcement capabilities.
- [Uninstall](Docs/UNINSTALL.md): GUI uninstall, recovery phrases, Debian purge
  behavior, and standalone script cleanup.
- [Hardening tracker](Docs/HARDENING.md): implemented bypass-resistance tactics,
  fail-closed behavior, and open hardening follow-ups.
- [Dev and production runbook](Docs/DEV_AND_PROD_RUNBOOK.md): local daemon
  runtime, dev Native Messaging manifests, browser-extension testing, and manual
  commands.
- [Component architecture](Docs/COMPONENT_ARCHITECTURE.md): daemon, policy
  engine, GUI, browser, database, systemd, and packaging boundaries.
- [TODO](Docs/TODO.md): open design and implementation work.

## Repository Layout

```text
.
├── focus-core/                  # Rust policy, config, database, evaluation
├── focusd/                      # Privileged daemon and local RPC surface
├── focus-gui/                   # Tauri v2 + Svelte desktop GUI
├── native-host/                 # Firefox/Chrome Native Messaging bridge
├── browser-extension-firefox/   # Firefox WebExtension TypeScript source
├── browser-extension-chrome/    # Chrome/Chromium MV3 TypeScript source
├── packaging/                   # systemd units and Native Messaging manifests
├── scripts/                     # dev, verification, package, install helpers
├── examples/                    # seed policy examples
├── Docs/                        # detailed operational and design docs
└── PoC/                         # historical proof of concept
```

## Architecture

| Component | Role |
| --- | --- |
| `focus-core` | Shared Rust library for policy validation, URL/app evaluation, schedules, allowances, unlocks, events, and SQLite migrations. |
| `focusd` | Root-oriented daemon. Owns the runtime database, enforcement state, `/etc/hosts` repair, browser policy repair, process checks, and the local daemon socket. |
| `focus-gui` | Unprivileged Tauri/Svelte GUI. It does not decide policy locally; it calls daemon-backed Tauri commands. |
| `native-host` | Unprivileged Native Messaging bridge used by Firefox and Chrome/Chromium extensions. |
| Browser extensions | Observe top-level navigation, maintain heartbeat state, ask the daemon for decisions, record visit usage, and show blocked pages. |
| `packaging/` and `scripts/` | systemd units, Native Messaging manifests, confined Firefox helper, Debian packaging, install, uninstall, and verification workflows. |

The main browser path is:

```text
browser extension -> blockuntu-native -> blockuntud -> focus-core
```

The main GUI path is:

```text
focus-gui frontend -> Tauri command -> blockuntud -> focus-core
```

## Current Policy Model

BlocKuntu supports:

- Tier 1 hard blocks.
- Tier 2 controlled access rules.
- Domain and URL pattern matching.
- Weekly schedules, including grouped days such as workdays and weekends.
- Daily allowances, including zero-minute allowances.
- Rule-scoped temporary unlocks.
- Cooldowns, maximum session length, and hourly unlock quotas.
- App rules based on process identity such as executable path, basename,
  command name, desktop id, and fallback window title matching.
- Browser heartbeat fail-closed behavior when the extension, native host, or
  daemon chain is unhealthy.

If multiple Tier 2 site rules match, the policy engine evaluates the stricter
applicable rule rather than relying on the first matching rule.

## Production Package

Build the current Debian package from the repository root:

```bash
./scripts/package-deb.sh
```

The default package version is currently `0.1.0-10`, and the artifact path is:

```bash
target/debian/blockuntu_0.1.0-10_$(dpkg --print-architecture).deb
```

Install it on the target Ubuntu/Debian machine with `apt`, not raw `dpkg -i`:

```bash
sudo apt install ./target/debian/blockuntu_0.1.0-10_$(dpkg --print-architecture).deb
sudo usermod -aG blockuntu "$USER"
```

Log out and back in after adding the desktop user to the `blockuntu` group.

For Firefox Snap or Firefox Flatpak, run the confined-browser helper as the
desktop user after installing the package:

```bash
blockuntu-setup-confined-firefox
```

See [Docs/INSTALLATION.md](Docs/INSTALLATION.md) for package inspection,
browser-specific setup, Chrome policy behavior, recovery files, and production
runtime paths.

## Local Development

Start the non-root dev daemon from the repository root:

```bash
./scripts/start-dev-daemon.sh
```

It uses temporary paths under `/tmp/blockuntu`, including:

```text
/tmp/blockuntu/config.toml
/tmp/blockuntu/blockuntu.sqlite3
/tmp/blockuntu/blockuntud.sock
/tmp/blockuntu/firefox/policies.json
/tmp/blockuntu/chrome/policies/managed/blockuntu.json
/tmp/blockuntu/hosts
```

Install dev Native Messaging manifests:

```bash
./scripts/install-dev-native-host.sh
```

Then restart Firefox or Chrome so the browser reloads the manifests.

Run the GUI in development mode:

```bash
cd focus-gui
npm install
npm run tauri dev
```

The GUI auto-detects `/run/blockuntu/blockuntud.sock` first and then
`/tmp/blockuntu/blockuntud.sock`.

## Browser Extension Development

Firefox:

```bash
cd browser-extension-firefox
npm install
npm run build
```

Load `browser-extension-firefox/manifest.json` from
`about:debugging#/runtime/this-firefox`.

Chrome/Chromium:

```bash
cd browser-extension-chrome
npm install
npm run build
```

Load `browser-extension-chrome/` from `chrome://extensions` with Developer mode
enabled.

The Chrome extension ID is kept stable by the manifest key:

```text
odedgejjcdilkoibeljkeohekonmdfea
```

## Verification

From the repository root:

```bash
./scripts/verify-focus-core.sh
./scripts/verify-focusd.sh
./scripts/verify-focus-gui.sh
./scripts/verify-native-host.sh
./scripts/verify-firefox-extension.sh
./scripts/verify-chrome-extension.sh
```

Targeted checks are also available from each component:

```bash
cargo test --manifest-path focus-core/Cargo.toml --all-targets
cargo test --manifest-path focusd/Cargo.toml --all-targets
cargo test --manifest-path native-host/Cargo.toml --all-targets
```

```bash
cd focus-gui
npm run verify
```

```bash
cd browser-extension-firefox
npm run verify
```

```bash
cd browser-extension-chrome
npm run verify
```

## Uninstall

For Debian package installs, use the GUI Admin tab when possible. The GUI
validates the uninstall phrase and then runs the package purge through `pkexec`.

The terminal equivalent is:

```bash
sudo dpkg --purge blockuntu
```

Package purge removes the systemd units, runtime socket directory, package
binaries, package GUI launcher/icons, BlocKuntu-owned browser policies,
BlocKuntu-owned `/etc/hosts` state, `/etc/blockuntu`, and `/var/lib/blockuntu`.

See [Docs/UNINSTALL.md](Docs/UNINSTALL.md) before manual cleanup, especially if
you used dev manifests, Firefox Snap, or Firefox Flatpak.
