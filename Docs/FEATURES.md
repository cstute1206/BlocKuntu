# BlocKuntu Implemented Features

This document lists the implemented BlocKuntu features found in the active  
codebase: `focus-core`, `focusd`, `focus-gui`, `native-host`,  
`browser-extension-firefox`, `browser-extension-chrome`, `packaging`, and  
`scripts`.

## Policy Model

- Central local policy store owned by the daemon and shared through  
`focus-core`.
- Structured policy configuration for website rules, application rules,  
schedules, allowances, defaults, and strict-mode settings.
- SQLite-backed runtime state for policy rows, unlocks, visits, events,  
Detox sessions, service state, and extension heartbeats.
- TOML config parsing and validation.
- Database-backed policy loading and replacement.
- Unique ID validation for site rules, app rules, schedules, and allowances.
- Validation that referenced schedules and allowances exist.
- Validation that one allowance is linked to only one site or app rule.
- Validation that hard-block rules cannot define allowances.
- Fixed Tier 2 unlock policy enforced by the policy engine.
- Strict-mode settings:
  - require Firefox extension heartbeat
  - require Chrome extension heartbeat
  - kill supported browsers when the extension heartbeat is stale
  - block unsupported browsers
  - configurable grace period, defaulting to 30 seconds

## Website Blocking

- Tier 1 hard-block website rules.
- Tier 2 controlled-access website rules.
- Per-rule enabled/disabled state.
- Multiple patterns per website rule.
- Website pattern kinds:
  - domain
  - exact URL
  - URL prefix
  - URL contains
  - path prefix
- Optional subdomain matching for domain patterns.
- URL normalization before policy evaluation.
- Invalid URL blocking.
- Tier 1 rules are always active when enabled.
- Tier 2 rules are active only during linked schedule windows.
- Multiple matching Tier 2 site rules are resolved by selecting the stricter  
applicable rule.
- Tier 2 strictness accounts for allowance size and pattern specificity.
- Block reason metadata includes rule ID, rule name, tier, controlled-access  
reason, schedule details, expected release time, and allowance reset time.
- URL probe support through the GUI and daemon RPC.

## Application Blocking

- Tier 1 hard-block application rules.
- Tier 2 controlled-access application rules.
- Per-rule enabled/disabled state.
- Multiple matchers per application rule.
- Application matcher kinds:
  - executable path
  - executable basename
  - command name
  - desktop ID
  - exact window title
  - window title contains
- Case-insensitive matching for executable basename, command name, and desktop  
ID.
- `/proc` scanning for running processes.
- Executable path and basename detection from `/proc/[pid]/exe`.
- Command-name detection from `/proc/[pid]/comm`.
- Desktop ID detection from process environment and command-line hints.
- Window-title detection with `wmctrl -lp` when available.
- Window-title support status reporting, including provider, session type, and  
availability details.
- Forbidden process termination with `SIGTERM`.
- `app_killed` style event logging for terminated processes.
- Running-app snapshot API for the GUI, including decision, blocking rule, and  
detected identity fields.
- GUI-assisted creation of app rules from detected running applications.
- Mandatory hard app rule injection for unsupported browsers to reduce bypass  
paths.
- Unsupported browser blocking includes Chromium, Brave, Edge, Opera, Vivaldi,  
LibreWolf, Waterfox, Epiphany, Falkon, qutebrowser, Midori, Min, Nyxt, and  
Tor Browser.
- Supported browser protection can terminate Firefox or Chrome when the  
required extension heartbeat is stale beyond the configured grace period.

## Schedules

- Reusable schedules referenced by site and app rules.
- Multiple windows per schedule.
- Schedule day selectors:
  - everyday
  - workdays
  - weekend
  - individual weekdays
- `HH:MM` time validation.
- Overnight schedule windows where the end time is earlier than the start time.
- Active schedule evaluation using local time.
- GUI weekly schedule editor.
- GUI weekly grid visualization.
- Guardrails that prevent editing or deleting currently active schedules.

## Allowances And Usage Tracking

- Reusable daily allowances.
- Daily allowance values in minutes, including zero-minute allowances.
- Site visit tracking through browser extensions:
  - visit start
  - visit heartbeat
  - visit end
- App usage accounting for metered app rules.
- Daily usage calculation with current-day clamping.
- Tier 2 allow while daily allowance remains.
- Tier 2 block when allowance is exhausted.
- Allowance exhaustion block reasons include next reset time.
- Guardrails that prevent editing or deleting currently active allowances.
- Automatic cleanup of unreferenced allowances after rule edits.

## Temporary Unlocks

- Daemon-mediated `request_unlock` flow for Tier 2 targets.
- Unlock target resolution for URLs and app rules.
- Hard-blocked targets cannot be unlocked.
- Unlock reasons must contain at least 20 letters.
- Reasons cannot be reused, ignoring case and repeated whitespace.
- Empty target validation.
- Active unlock detection to prevent duplicate unlocks for a rule.
- One global unlock per rolling hour across all website and application rules.
- Active Detox targets reject manual unlock requests before the reason or
  hourly quota is consumed.
- Unlock rows persisted in SQLite.
- `unlock_granted` event logging.
- GUI manual unlock form.
- Every Tier 2 unlock lasts exactly 2 minutes.

## Detox Sessions

- Detox sessions that temporarily block selected site rules and app rules.
- Optional Detox session name.
- Duration validation from 1 minute to 12 weeks.
- Session target validation against configured site and app rules.
- Active, scheduled, expired, and cancelled session status reporting.
- Remaining time reporting for active sessions.
- Detox blocks take precedence over normal hard/controlled evaluation.
- Detox block reasons include session ID, session name, target kind, rule ID,  
rule name, end time, and expected release time.
- GUI Detox start workflow with custom minute, hour, day, and week durations
  plus hour/day/week presets.
- GUI active Detox list and recent Detox history.
- Detox cancellation support.
- Detox cancellation requires the privileged Tier 1 edit unlock.
- `detox_started` and `detox_cancelled` event logging.
- Editing/deleting rules covered by an active Detox session is blocked.

## Browser Extension Enforcement

- Firefox WebExtension implementation.
- Chrome/Chromium MV3 extension implementation.
- Native Messaging integration through `blockuntu_native`.
- Top-level HTTP/HTTPS navigation interception.
- History-state navigation handling.
- Browser-to-daemon JSON-RPC calls through the native host.
- Periodic extension heartbeat:
  - Firefox component: `firefox_extension`
  - Chrome component: `chrome_extension`
- Fail-closed browser behavior when the extension, native host, or daemon  
heartbeat chain is unhealthy.
- Block all top-level HTTP/HTTPS navigations when backend health cannot be  
proven.
- Periodic open-tab revalidation.
- Visit tracking from extensions for allowed navigations.
- Active visit heartbeats.
- Visit cleanup when tabs close or navigations change.
- Local extension setting to disable blocking during the short daemon-managed
  uninstall handoff.
- Extension uninstall/prep mode support through daemon service state.
- Blocked-page redirect with structured reason metadata.
- Blocked page displays:
  - blocked URL
  - reason title
  - summary and detail
  - tier
  - list/rule name
  - active schedules
  - Detox session
  - expected release
  - allowance reset
  - last heartbeat
  - technical reason
- Legacy native-host request compatibility for older extension messages.

## Native Messaging Host

- Native Messaging bridge for Firefox and Chrome/Chromium.
- Browser Native Messaging framing support.
- Unix domain socket forwarding to the daemon.
- JSON-RPC request forwarding.
- Development socket override support.
- Development revive-command support for starting the dev daemon when needed.
- Error handling for daemon connection failures and malformed messages.

## Daemon And RPC

- Privileged daemon as the single source of truth for policy and enforcement.
- One JSON request per Unix socket connection.
- Local Unix domain socket API.
- Production default socket at `/run/blockuntu/blockuntud.sock`.
- Development socket at `/tmp/blockuntu/blockuntud.sock`.
- RPC methods for:
  - status
  - enforcement status
  - prepare uninstall
  - config snapshot
  - create/update/delete site lists
  - create/update/delete allowances
  - create/update/delete app rules
  - create/update/delete schedules
  - start/cancel/list Detox sessions
  - log summary from the plain daemon event log
  - running apps
  - evaluate URL
  - request unlock
  - unlock Tier 1 edits
  - Tier 1 edit status
  - visit start/heartbeat/end
  - extension heartbeat
  - extension status
- Enforcement state persisted as service state.
- Browser extension mode persisted as service state.
- Event log retrieval with clamped limits.
- Structured JSON block decision responses.
- Daemon repair loop for browser policy and hosts enforcement.
- CLI commands for serving and targeted repair operations.

## System Enforcement

- Firefox enterprise policy repair.
- Firefox policy status reporting.
- Firefox policy force-install configuration for the extension.
- Firefox policy hardening for bypass-sensitive browser pages and developer  
tooling.
- Firefox private browsing support with extension enabled there.
- Firefox Flatpak policy support through the Flatpak systemconfig extension  
path.
- Chrome managed policy repair.
- Chrome policy status reporting.
- Chrome extension force-install configuration.
- Chrome extension settings with local update URL override.
- Local Chrome update manifest generation.
- Deferred browser policy repair until the first matching extension heartbeat.
- Optional disabling of Firefox and Chrome policy management through daemon  
flags.
- Hosts-file fallback for Tier 1 domain patterns.
- Managed `/etc/hosts` block between BlocKuntu markers.
- Preservation of user-owned hosts-file content outside the managed block.
- Hosts repair after relevant site-list policy changes.
- Hosts path watcher systemd integration.
- Production hosts immutability handling with `chattr`.
- Development hosts sandbox support.

## GUI

- Tauri v2 desktop application.
- Svelte frontend.
- Auto-detection of production socket first, then development socket.
- Main navigation views:
  - Overview
  - Websites
  - Applications
  - Detox
  - Schedules
  - Statistics
  - Settings
- Overview dashboard for daemon, block tiers, and system status.
- First-run setup panel.
- URL probe form.
- Manual unlock form.
- Website list CRUD.
- Website rule editor with tier, allowance, schedules, and patterns.
- Pattern editing with pattern kind, value, and subdomain toggle.
- App rule CRUD.
- App rule editor with tier, allowance, schedules, and matchers.
- Detected running apps panel with search and blocked-only filter.
- Create app rules from detected running apps.
- Schedule CRUD and weekly grid.
- Detox start/cancel/history UI.
- Statistics view with total and event-kind counts parsed from the plain daemon
  event log.
- Plain daemon event log at `/etc/blockuntu/blockuntu.log`.
- Settings health and browser-integration checks.
- Settings Tier 1 edit key display and unlock form.
- Settings policy TOML import/export and uninstall action.
- Local Settings preferences for refresh interval and restoring the last selected page.
- GUI-level error formatting.
- Tray icon support.
- Closing the window hides it to the tray instead of quitting.
- Tray actions:
  - show window
  - open Detox
  - open Settings
  - refresh status
  - quit GUI
- Tray status items for daemon, enforcement, and active Detox count.
- Periodic tray status refresh.

## Tier 1 And Operator Guardrails

- Active Tier 1 website list edits are protected.
- Additive edits to active hard rules are allowed where they do not weaken the  
rule.
- Removing or weakening active Tier 1 site rules requires a Tier 1 edit unlock.
- Tier 1 edit key loaded from `/etc/blockuntu/tier1-edit-key.txt`.
- Tier 1 edit unlock is available only during the operator window:  
Sunday 20:00-23:59.
- Tier 1 edit unlock lasts 5 minutes.
- Tier 1 edit unlock state is stored in daemon service state.
- Tier 1 edit unlock event logging.
- Active app rules and active site/app rules in Detox are protected from unsafe  
edits or deletion.

## Health, Status, And Observability

- Daemon status reporting with counts for rules, app rules, schedules, and  
allowances.
- Enforcement status reporting for browser policy and hosts-file state.
- Firefox extension status from heartbeat freshness.
- Chrome extension status from heartbeat freshness with a longer Chrome timeout.
- Browser heartbeat metadata includes browser, extension ID, and extension  
version.
- System health checks in the GUI.
- Statistics totals and event-kind counts parsed from the plain daemon event
  log.
- Structured event logging for policy edits, enforcement changes, unlocks,  
Detox, URL blocks, and uninstall preparation.

## Installation, Packaging, And Operations

- Debian package build script.
- Production install script.
- Production uninstall script.
- Package metadata under `packaging/deb`.
- systemd socket unit.
- systemd daemon service.
- systemd watchdog companion service.
- systemd hosts path/service repair units.
- Native Messaging manifests for Firefox, Chrome, and Chromium.
- Confined Firefox helper for Snap and Flatpak support.
- Development daemon starter.
- Development Native Messaging manifest installer.
- Component verification scripts for:
  - `focus-core`
  - `focusd`
  - `focus-gui`
  - `native-host`
  - Firefox extension
  - Chrome extension
- Runtime path documentation for production and development.
- Uninstall workflow through GUI with `pkexec`.
- Package purge cleanup for services, runtime paths, browser policies, hosts  
state, config, and database.
- Recovery uninstall phrase support at `/etc/blockuntu/uninstall-recovery.txt`.

## Tests And Verification Assets

- Rust tests for `focus-core`.
- Rust tests for `focusd`.
- Rust tests for `native-host`.
- Browser extension build and verification scripts.
- GUI verification script.
- Packaging and native-host verification scripts.
- Historical proof-of-concept code retained under `PoC/`, separate from the  
active implementation.

## Known Implemented Limitations

- Hosts-file fallback only represents Tier 1 domain patterns. Exact URL,  
prefix, contains, and path-level rules are browser-extension enforcement.
- Window-title app matching depends on `wmctrl` and is mainly useful on  
X11-compatible sessions.
- Firefox and Google Chrome are the supported browser enforcement paths;  
other browsers are handled as blocked applications in strict mode.
- Tier 2 unlock behavior is fixed in the policy engine and is not configurable
  through TOML.
- Network-level fallback beyond browser and hosts enforcement is documented as  
future work.
