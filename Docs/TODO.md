# BlocKuntu Roadmap And Open Work

`Docs/FEATURES.md` is the inventory of implemented behavior. This file is the
single roadmap for missing features, hardening work, bugs, and product ideas.

## Priority 1: Correctness And Hardening

### Stronger Network Fallback

Current strict mode is process-based:

- force-install supported browser extensions through managed policy
- require recent Firefox/Chrome extension heartbeats
- close Firefox/Chrome when their required extension heartbeat is stale
- hard-block unsupported browsers as applications

Future hardening option:

- add a root-only network fallback that blocks browser web traffic when
  extension health is stale
- evaluate `nftables` first because it can target user/process/network traffic
  more honestly than `/etc/hosts`
- consider a local DNS or proxy layer only if it can preserve exact policy
  semantics and fail safely
- expose this as an explicit opt-in strict-mode setting, not as the default dev
  behavior
- add install, uninstall, status, and repair commands before enabling it in
  production

Acceptance criteria:

- stale extension heartbeat blocks supported browser network access without
  killing unrelated apps
- recovery is automatic after a fresh extension heartbeat
- uninstall removes every firewall/DNS/proxy rule created by BlocKuntu
- docs include verification commands for active rules and cleanup

### SQLite And Policy Tamper Resistance

Open question: deleting or replacing the SQLite database as root can destroy
runtime and policy state.

Potential work:

- define which state is canonical during production: TOML, SQLite, or both
- keep recoverable policy backups before database replacement
- detect missing/corrupt database on daemon startup
- restore policy from config or last-known-good snapshot where possible
- log a high-severity event when policy state was recreated
- document what root-level tampering can and cannot be defended against

Acceptance criteria:

- deleting the database does not silently remove configured blocking rules
- daemon startup reports the recovery path clearly
- GUI health shows policy recovery or corruption warnings

### Time Tampering Behavior

Open question: changing the system clock can affect schedules, allowances,
cooldowns, unlocks, Detox, and operator windows.

Potential work:

- document current behavior when system time moves backward or forward
- detect large wall-clock jumps in daemon service state
- decide whether active Detox and Tier 1 guardrails should use monotonic timing
  where possible
- add warning events for suspicious time jumps
- make GUI health surface recent time-jump warnings

Acceptance criteria:

- expected behavior is documented for manual clock changes
- large time jumps create a visible event
- Detox/unlock/operator-window behavior is not silently weakened

### Single GUI Instance

Prevent multiple BlocKuntu GUI processes from running at the same time.

Potential work:

- add a single-instance lock or Tauri single-instance plugin
- focus/show the existing window when a second launch is attempted
- keep tray behavior compatible with the single-instance flow

Acceptance criteria:

- launching the app twice shows the existing window instead of opening a second
  independent GUI
- tray state and daemon polling remain stable

## Priority 1: Current Bugs

### Firefox Snap Native Host Warning

Current issue:

- GUI can warn that
  `/home/akhi/snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json`
  is missing even when Firefox policy/enforcement actually works.

Potential work:

- distinguish "not installed" from "not needed for this Firefox packaging"
- check system Firefox, Snap Firefox, and Flatpak Firefox separately
- make the health message explain which path is active

Acceptance criteria:

- no warning is shown for an unused Snap path when the active Firefox
  integration is healthy
- setup guidance is specific to the detected Firefox package type

### GUI Error Messages

Current issue:

- some GUI errors expose raw JSON-RPC internals instead of user-facing messages.

Potential work:

- normalize daemon error responses in `focus-gui/src/lib/errors.ts`
- map common daemon validation failures to concise GUI copy
- keep technical details available in logs or expandable debug UI

Acceptance criteria:

- common validation, socket, policy, and unlock errors are readable in the GUI
- raw JSON-RPC structures are not shown as primary user-facing text

### Chrome Extension Package Refresh

Current issue:

- Chrome extension package may be stale or not updated correctly.

Potential work:

- rebuild the Chrome extension package from current source
- verify manifest key, extension ID, native messaging origin, update manifest,
  and hosted CRX URL
- update packaging artifacts and docs if the CRX changed

Acceptance criteria:

- installed Chrome extension matches the current source version
- Chrome managed policy force-installs the expected extension ID
- native messaging works after package install and browser restart

## Priority 1: Verification Tasks

- Test Tier 1 URL contains behavior in production-like browser installs.
- Test adding patterns while a Tier 1 list is active.
- Test uninstall flow during the Sunday 20:00-23:59 operator window.
- Test new Detox, tray, and desktop app allowance behavior on a clean install.
- Run clean-VM acceptance testing for the Debian package.

## Priority 2: Packaging And Platform Support

### Full Debian Package Hardening

Initial `.deb` packaging exists in `scripts/package-deb.sh` and
`packaging/deb/`. It packages the daemon, native host, Tauri GUI, systemd
units, Native Messaging manifests, a minimal strict-browser config, and local
extension artifacts used as later policy install sources. Browser policy repair
is deferred until the first extension heartbeat.

Remaining packaging hardening:

- verify the package on a clean Ubuntu/Debian VM
- move from raw `dpkg-deb` metadata to a fuller Debian packaging workflow if
  external distribution becomes a goal
- audit runtime dependencies across Ubuntu LTS versions; `wmctrl` is currently
  only recommended because title matching can degrade without blocking install
- preserve `/etc/blockuntu/config.toml` as a conffile across upgrades
- document that package installation cannot safely infer which desktop user
  should be added to the `blockuntu` group without an explicit installer prompt
  or post-install command

Acceptance criteria:

- `apt install ./blockuntu_VERSION_amd64.deb` installs runtime dependencies and
  starts the daemon on a clean Ubuntu/Debian VM
- the GUI launches from the app launcher without build tools installed
- `node`, `npm`, `rustc`, and `cargo` are only needed on the build machine
- upgrades preserve user config and restart systemd units cleanly
- removal stops units and removes installed BlocKuntu-owned files without
  deleting user policy/config state unexpectedly

### RPM Package Support

Potential work:

- support Fedora/RHEL-style package paths and dependencies
- support Firefox RPM policy/native-host paths
- support Chrome RPM policy/native-host paths where they differ from Debian
- add package verification docs for RPM-based distros

Acceptance criteria:

- daemon, GUI, native host, systemd units, policies, and manifests install on a
  clean RPM-based desktop VM
- uninstall removes BlocKuntu-owned state without touching user-owned policy
  unexpectedly

## Priority 2: Product Features

### Import And Export Rules

Potential work:

- export site rules, app rules, schedules, allowances, strict-mode settings,
  and Detox-independent policy data
- import with validation and conflict handling
- support dry-run preview before replacing policy
- include version metadata in exported files

Acceptance criteria:

- user can export policy from one machine and import it on another
- invalid imports do not partially mutate current policy
- conflicts are explained before applying changes

### Reverse Block / Allowlist Mode

Feature idea:

- block everything except selected websites and applications.

Design questions:

- should this be global, schedule-based, Detox-based, or rule-based?
- how should operating-system update domains and login/captive portal pages be
  handled?
- should unsupported browsers still be killed, or should all browser traffic be
  forced through supported browsers?

Acceptance criteria:

- allowlisted sites and apps remain available
- non-allowlisted web navigation blocks fail-closed
- bypass-sensitive browser and app behavior remains compatible with strict mode

### Admin Settings

Feature idea:

- add an Admin settings area for guardrail and operator-window behavior.

Candidate settings:

- uninstall phrase window
- Tier 1 edit operator window
- strict-mode grace period
- browser policy management toggles
- whether network fallback is enabled once implemented

Important product decision:

- Tier 2 unlock policy should not become configurable. It should remain fixed
  and predictable.

Acceptance criteria:

- settings that affect enforcement are daemon-owned and validated
- GUI changes cannot weaken active Tier 1 protections without the proper unlock

### GUI Keyboard Workflow

Potential work:

- pressing Enter creates a new pattern/matcher field in website and app editors
- Shift+Arrow navigates between pattern/matcher fields
- preserve normal text editing behavior inside inputs

Acceptance criteria:

- repeated rule entry is faster from the keyboard
- shortcuts do not interfere with typing URLs, paths, or titles

### Humanized Policy Labels Cleanup

Current issue:

- the rename from "Lists" to "Websites" is implemented, but the
  `humanizePolicyNouns` approach is brittle.

Potential work:

- replace string post-processing with explicit display labels
- keep daemon/API nouns stable while GUI copy uses product language

Acceptance criteria:

- GUI consistently says "Websites" where intended
- no broad string replacement is needed for display copy

### Detox Remaining Time In More Places

Implemented:

- the GUI active Detox list reports remaining time.

Potential follow-up:

- ensure blocked pages, tray status, and overview cards consistently show when
  a Detox block ends.

Acceptance criteria:

- user can see remaining Detox time from the main surfaces they encounter while
  blocked

## Priority 3: Exploratory Ideas

### Anti-Infinite-Scroll Mode

Feature idea:

- limit how far a user can scroll on selected sites, then obscure or block the
  page after a threshold.

Research first:

- compare how other focus products approach feed limiting
- evaluate whether this belongs in the extension or in rule metadata
- test whether it creates a better behavior than simple blocking

### MCP Support

Feature idea:

- expose BlocKuntu status and controlled actions through an MCP server.

Potential capabilities:

- read current enforcement status
- list rules and recent events
- request manual unlocks
- start predefined Detox sessions

Guardrail:

- any action that weakens enforcement must go through the same daemon
  validation and operator controls as the GUI.

### Brave Browser Support

Current strict-mode behavior blocks Brave as an unsupported browser.

Potential work:

- evaluate whether Brave extension install, managed policy, and native
  messaging can be made as reliable as Firefox/Chrome
- only move Brave from blocked to supported if heartbeat and managed policy
  guarantees are comparable

### Smartphone Detox Integration

Feature idea:

- integrate with Android via Tasker or a companion app.

Design questions:

- is BlocKuntu the source of truth, or does the phone report state back?
- what trust model is acceptable for mobile enforcement?
- should phone integration only start Detox sessions, or also mirror rules?

## Done / Moved To Features

These items were present in the older `Docs/ToDo.md` notes and are now treated
as implemented or covered by `Docs/FEATURES.md`:

- uninstall heartbeat / uninstalling browser-extension mode
- initial browser heartbeat startup behavior improvements
- open-tab revalidation when a block becomes active
- strictest-rule selection for duplicate/multiple matching Tier 2 rules
- Firefox Snap and Flatpak support
- manual unlock support through native messaging
- 24-hour schedule display and schedule validation
- Ubuntu taskbar/app icon work
- first-run refresh improvements
- Admin page redesign
- enabled/disabled distinction cleanup in the GUI workflow
- URL contains pattern support
- Tier 1 edit guardrails for active lists
- operator-window restricted uninstall/Tier 1 edits
- tray icon behavior
- Detox mode
- daily allowances for desktop applications
- app-rule case-insensitive matching for basename/command/desktop ID
- detected-app search workflow
