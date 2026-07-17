# BlocKuntu Hardening Tracker

This document tracks hardening tactics that intentionally make bypasses harder
in the installed production flow. The daemon remains the enforcement authority:
GUI, browser extensions, and the Native Messaging bridge should surface daemon
state, not make durable trust decisions locally.

## Implemented

### No Runtime Enforcement Stop

Status: implemented across `focusd` and the Tauri tray.

Risk addressed:

- The previous tray actions and daemon RPC methods could persistently stop all
  URL, application, browser-policy, and hosts-file enforcement.

Implementation:

- The `start_enforcement` and `stop_enforcement` RPC methods no longer exist.
- The tray no longer exposes start or stop actions.
- Legacy `enforcement_state = stopped` and
  `browser_extension_mode = disabled` values are ignored, so upgrading
  automatically returns an old stopped installation to active enforcement.
- The GUI can still quit without stopping the daemon or enforcement.

Uninstall handoff:

- GUI uninstall retains the internal `prepare_uninstall` RPC so browser
  extensions can stand down before package purge removes the native host.
- This is not a general stop mechanism. It creates a 30-second
  `uninstalling` lease and the daemon itself rejects calls outside Sunday
  20:00-23:59 or while clock integrity is tampered.
- If package purge fails, the lease expires and daemon repair loops restore
  browser policies, the managed hosts block, and process enforcement.

### Policy Database Recovery Snapshot

Status: implemented in `focusd`.

Risk addressed:

- Deleting `/var/lib/blockuntu/blockuntu.sqlite3` previously caused the daemon
  to recreate the database from the minimal packaged
  `/etc/blockuntu/config.toml`, losing rules configured through the GUI.

Implementation:

- The daemon maintains a complete validated TOML snapshot at
  `/etc/blockuntu/policy-recovery.toml`.
- Every successful policy mutation updates this snapshot, including site-list,
  application-rule, allowance, schedule, and TOML-import changes.
- Snapshot writes use a temporary file, `fsync`, and an atomic rename. The
  resulting file is root-owned in production and has mode `0600`.
- Production applies `chattr +i` to the snapshot. The daemon temporarily clears
  the immutable flag, updates the snapshot, and reapplies the flag.
- Development uses `/tmp/blockuntu/policy-recovery.toml` without immutability.

Startup behavior:

- A valid policy in SQLite remains authoritative and refreshes the snapshot.
- If SQLite is missing or contains no persisted policy, the daemon restores the
  complete policy from the recovery snapshot and records a `policy_recovered`
  event.
- An existing empty database without a recovery snapshot is treated as
  suspicious. The daemon fails closed instead of silently replacing prior
  state with the packaged baseline.
- A genuinely new installation with neither a database nor a recovery snapshot
  initializes from `/etc/blockuntu/config.toml` and immediately creates the
  recovery snapshot.

Uninstall behavior:

- Debian package removal and `scripts/uninstall-production.sh --purge-data`
  clear the immutable flag before removing `/etc/blockuntu`.
- Normal scripted uninstall preserves both `/etc/blockuntu` and
  `/var/lib/blockuntu`.

Known limits:

- This protects against accidental deletion and simple local tampering. A user
  with unrestricted root access can clear immutable flags and remove the
  database, snapshot, daemon, and service units.
- The recovery snapshot contains policy configuration only. Runtime history,
  usage accounting, detox sessions, events, and clock-integrity state remain
  SQLite-only.
- A corrupt SQLite file causes daemon startup to fail rather than being
  overwritten automatically. The recovery snapshot remains available for an
  explicit repair flow.

### Detox Tier Isolation

Status: implemented in `focus-core`.

- Tier 2 rules activated by Detox remain strict, enter the managed hosts block
  for domain patterns, and reject manual unlock requests before a reason or the
  global hourly quota is consumed.
- Tier 3 rules activated by Detox intentionally retain their daily allowance
  and manual unlock behavior and therefore never enter the hosts file.
- Ending Detox early still requires the privileged Tier 1 cancellation path.

### Clock Tamper Detection

Status: implemented in `focusd` and `focus-core`.

Risk addressed:

- A user can move the system wall clock to bypass schedules, allowances, hourly
  unlock limits, detox end times, or Tier 1 operator windows.

Implementation:

- `focusd/src/clock_guard.rs` records a clock baseline in SQLite
  `service_state`.
- Each guarded daemon time read compares current UTC wall time with Linux boot
  id and `/proc/uptime`.
- If wall time drifts from uptime by more than five minutes, or moves backwards
  across reboot by more than five minutes, the daemon persists
  `clock_guard.status = tampered`.
- A detected tamper records a `clock_tamper_detected` event.
- The tampered state is sticky. It is not automatically cleared just because
  the current wall clock later looks reasonable.

Fail-closed behavior:

- Runtime RPC paths ignore normal client-supplied `now` values in production.
  Tests explicitly opt into trusted client time.
- Scheduled controlled rules are treated as active while the clock is tampered.
- Controlled-access allowances and temporary unlocks do not grant access while
  the clock is tampered.
- Non-cancelled detox sessions continue to block while the clock is tampered.
- Privileged policy mutations, detox start/cancel, temporary unlock requests,
  and Tier 1 edit unlocks are rejected while the clock is tampered.
- The Tier 1 operator window is reported closed while the clock is tampered.
- App usage accounting is paused while the clock is tampered to avoid writing
  misleading allowance state.

Diagnostic surface:

- `status` includes `clock_integrity`.
- `enforcement_status` includes `clock_integrity`.
- `clock_integrity_status` returns the current daemon clock-integrity payload.
- `tier1_edit_status` includes `clock_integrity`.

Known limits:

- This is local tamper detection, not a secure time source. It detects wall
  clock movement relative to Linux uptime and reboot identity.
- A privileged user with root access can still modify daemon state or database
  files. The target is practical local-user bypass resistance, not protection
  against a fully privileged administrator.
- Automatic recovery is intentionally not implemented yet. Clearing a tampered
  state should be a deliberate recovery flow.

## Planned Or Open

### Recovery Flow For Clock Tamper

Status: open.

Decision needed:

- Define the operator-approved way to clear `clock_guard.status = tampered`
  after the system time has been corrected.

Preferred shape:

- Require an explicit privileged recovery action.
- Record a recovery event with timestamp and reason.
- Avoid silently clearing the state from normal health/status polling.

### Packaging And Health UI Surfacing

Status: open.

Desired behavior:

- The GUI health/admin surface should show clock-integrity state with concise
  wording.
- Production docs should explain how an operator confirms and recovers from
  clock tamper.

### Browser Policy And Native Messaging Hardening

Status: existing hardening, tracking still incomplete.

Current behavior:

- Browser extension heartbeat is used by strict browser enforcement.
- Browser policy repair can be deferred until first extension heartbeat.
- Strict mode can fail closed by killing unsupported or stale-extension browser
  processes.

Tracking gap:

- Document the exact production health states and recovery actions in this
  tracker after the GUI status surface is finalized.
