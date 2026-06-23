# BlocKuntu Hardening Tracker

This document tracks hardening tactics that intentionally make bypasses harder
in the installed production flow. The daemon remains the enforcement authority:
GUI, browser extensions, and the Native Messaging bridge should surface daemon
state, not make durable trust decisions locally.

## Implemented

### Clock Tamper Detection

Status: implemented in `focusd` and `focus-core`.

Risk addressed:

- A user can move the system wall clock to bypass schedules, allowances, unlock
  cooldowns, detox end times, or Tier 1 operator windows.

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
