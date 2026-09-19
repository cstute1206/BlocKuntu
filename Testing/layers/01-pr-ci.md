# Layer 1: Pull-request CI

## Purpose

This layer gives fast, deterministic feedback without requiring root access,
systemd, a graphical desktop, or installed browser packages. It validates
policy semantics and component contracts, but it does not claim that a package
works in a clean VM.

## Required jobs

Run independent jobs for:

- `focus-core`: format, Clippy, and all tests;
- `focusd`: format, Clippy, and all tests;
- `native-host`: format, Clippy, and all tests;
- Tauri Rust backend: format, Clippy, and tests;
- Svelte GUI: type checking, unit tests, and production build;
- Firefox extension: unit tests, TypeScript build, and manifest check;
- Chrome extension: unit tests, TypeScript build, and manifest check.

Cargo dependency resolution must use each crate's committed lockfile.

## General policy cases

| ID | Test case | Expected result |
| --- | --- | --- |
| PR-POL-001 | Evaluate domain matching with and without subdomains. | Only the configured domain scope is blocked. |
| PR-POL-002 | Evaluate exact URL, URL prefix, URL contains, and path prefix with positive and near-miss inputs. | Every matcher blocks its intended input and allows similar non-matching inputs. |
| PR-POL-003 | Evaluate a disabled list and a Tier 2 or Tier 3 list without an active schedule or Detox. | The rule is inactive. |
| PR-POL-004 | Evaluate schedule start, end, overnight, weekday, and local-time boundaries. | Blocking starts and stops exactly at the configured boundaries. |
| PR-POL-005 | Start and finish Detox sessions for website and application targets. | Selected rules activate during Detox and become inactive after its absolute end. |
| PR-POL-006 | Evaluate overlapping Hard, active Scheduled Block, and Controlled Access rules. | Hard wins, then active Scheduled Block, then Controlled Access. |
| PR-POL-007 | Evaluate overlapping Controlled Access allowances. | The strictest applicable remaining allowance is reported and charged. |
| PR-POL-008 | Evaluate website allowance start, heartbeat, stale heartbeat, end, and local-midnight reset. | Only confirmed eligible time is charged and the allowance resets on the local day boundary. |
| PR-POL-009 | Evaluate application blocking by executable path, basename, command name, desktop ID, and available X11 title. | Each matcher uses only the identity fields defined for the evaluated process. |
| PR-POL-010 | Evaluate clock-integrity failure for time-sensitive policies. | Time-sensitive restrictions fail closed. |

## Website allowlist cases

| ID | Test case | Expected result |
| --- | --- | --- |
| PR-WAL-001 | Parse a Tier 1 website allowlist. | Validation rejects it because Tier 1 allowlists are unsupported. |
| PR-WAL-002 | Parse an empty website allowlist. | Validation rejects a configuration that would unintentionally allow nothing. |
| PR-WAL-003 | Evaluate an inactive Tier 2 website allowlist. | Matching and non-matching HTTP(S) URLs are allowed by that list. |
| PR-WAL-004 | Activate a Tier 2 website allowlist through a schedule. | Matching URLs are allowed and non-matching top-level HTTP(S) URLs are blocked. |
| PR-WAL-005 | Activate the same Tier 2 allowlist through Detox. | The schedule-independent Detox activation produces the same inversion. |
| PR-WAL-006 | Evaluate browser-internal, extension, `file://`, and subresource URLs. | They remain outside the website-allowlist top-level navigation boundary. |
| PR-WAL-007 | Activate two website allowlists with different allowed domains. | A URL must satisfy both lists; the effective allowed set is their intersection. |
| PR-WAL-008 | Match an active allowlist and a matching blocklist simultaneously. | The blocklist wins. |
| PR-WAL-009 | Render the hosts file for active website allowlists. | No catch-all allowlist representation is written to `/etc/hosts`. |
| PR-WAL-010 | Meter a non-matching URL under a Tier 3 allowlist. | It consumes that list's allowance; a matching URL does not. |
| PR-WAL-011 | Exhaust a Tier 3 website-allowlist allowance and request its manual unlock. | The rejected URL is allowed only while the valid rule unlock is active. |
| PR-WAL-012 | Combine an unlocked Tier 3 allowlist with another rejecting allowlist or blocklist. | The other active restriction still blocks the URL. |

## Application allowlist cases

Application allowlists operate on individual processes. Tests must not create
implicit application, cgroup, parent-child, or helper-process permission.

| ID | Test case | Expected result |
| --- | --- | --- |
| PR-AAL-001 | Parse a Tier 1 application allowlist. | Validation rejects it because Tier 1 allowlists are unsupported. |
| PR-AAL-002 | Parse an application allowlist without any allowed identity. | Validation rejects it. |
| PR-AAL-003 | Evaluate an inactive Tier 2 application allowlist. | Matching and non-matching eligible user processes are allowed by that list. |
| PR-AAL-004 | Activate a Tier 2 application allowlist through a schedule. | Each matching process remains; each eligible non-matching process receives `SIGTERM`. |
| PR-AAL-005 | Activate the same Tier 2 allowlist through Detox. | The same per-process inversion applies. |
| PR-AAL-006 | Allow a parent process but evaluate a differently identified child, sibling, helper, renderer, or crash handler. | Permission does not transfer; the other process is independently rejected. |
| PR-AAL-007 | Allow a process by one identity while a different identity field belongs to another process. | Only the process whose own matcher resolves is allowed. |
| PR-AAL-008 | Evaluate PID 1, root and non-desktop system accounts, unknown user identities, BlocKuntu components, and required BlocKuntu descendants. | Recovery-exempt processes are never terminated by an application allowlist. |
| PR-AAL-009 | Activate two application allowlists with different permitted process identities. | A process must satisfy both active lists. |
| PR-AAL-010 | Match an application allowlist and an application blocklist simultaneously. | The blocklist wins. |
| PR-AAL-011 | Run one rejected process under a Tier 3 application allowlist. | Runtime is charged once to the rejecting rule and the process is terminated after exhaustion. |
| PR-AAL-012 | Run several rejected processes for the same Tier 3 allowlist. | The rule is metered once for elapsed wall-clock time, not once per process. |
| PR-AAL-013 | Run only matching processes under a Tier 3 allowlist. | No allowlist allowance is consumed. |
| PR-AAL-014 | Minimize or background a rejected Tier 3 process. | It continues consuming allowance because metering is process-based, not focus-based. |
| PR-AAL-015 | Request a Tier 3 unlock using the list ID and then a rejected process identity. | Both resolve to the rejecting rule when all unlock preconditions are satisfied. |
| PR-AAL-016 | Request a Tier 1 or Tier 2 unlock, reuse a reason, provide fewer than 20 alphabetic letters, or exceed the rolling-hour quota. | The request is rejected. Tier 3 manual unlocks remain available during targeted Detox under the normal preconditions. |
| PR-AAL-017 | Unlock one Tier 3 allowlist while another allowlist or blocklist rejects the process. | The other restriction still wins. |
| PR-AAL-018 | Let the fixed two-minute unlock expire. | Rejected processes are terminated on a subsequent process scan and exhausted allowance remains exhausted. |

## Active-edit and persistence cases

| ID | Test case | Expected result |
| --- | --- | --- |
| PR-EDIT-001 | Append entries to an active blocklist. | The stricter edit succeeds. |
| PR-EDIT-002 | Remove or modify entries in an active blocklist. | The weakening edit is rejected. |
| PR-EDIT-003 | Remove allowed entries from an active allowlist. | The stricter edit succeeds. |
| PR-EDIT-004 | Add or modify allowed entries in an active allowlist. | The weakening edit is rejected. |
| PR-EDIT-005 | Change active name, mode, tier, schedules, allowance, or enabled state. | Protected-field changes are rejected server-side. |
| PR-DATA-001 | Export policy to TOML and parse it again. | Supported policy fields round-trip without protected service state. |
| PR-DATA-002 | Import a policy with new IDs. | New objects append and existing objects remain. |
| PR-DATA-003 | Import identical or conflicting IDs. | Identical objects are skipped and conflicts are rejected. |
| PR-DATA-004 | Persist blocklist and allowlist modes through SQLite and recovery TOML. | Modes and supported configuration survive round-trips. |

## Component-state cases

| ID | Test case | Expected result |
| --- | --- | --- |
| PR-HB-001 | Classify browser closed, starting, active, stale, and missing-heartbeat states. | Each state is reported without conflating normal startup with failure. |
| PR-HB-002 | Close and reopen a browser after a heartbeat from the previous session. | A new startup grace is used and the old heartbeat is not accepted for the new session. |
| PR-HB-003 | Receive the first verified heartbeat with deferred policy repair enabled. | Managed browser policy is written only after the verified heartbeat. |
| PR-EXT-001 | Unit-test navigation, existing-tab revalidation, SPA navigation, fail-closed transitions, and recovery. | Firefox and Chrome implementations produce equivalent policy decisions. |
| PR-EXT-002 | Render blocked-page reasons for blocklists and allowlists. | The page clearly distinguishes a matched block from a not-allowed target. |
| PR-GUI-001 | Unit-test mode controls and Tier selection. | Selecting allowlist mode keeps Tier 1 unavailable. |
| PR-GUI-002 | Unit-test the application-allowlist save warning. | It names session services, portals/authentication, audio/clipboard, browser helpers, and IDE/terminal child risks. |
| PR-GUI-003 | Unit-test Add all detected deduplication and preferred identity selection. | One preferred identity per current process is added without implying future helper coverage. |

Application termination in this layer means asserting the enforcement decision through a
recording process killer; real signal delivery and desktop effects belong to Layer 3.

## Completion criteria

- Restriction precedence, process exemptions, allowance accounting, and unlock
  boundaries must have automated tests.
- Every other case has an automated test or an explicit follow-up issue.
- Case-to-test references are maintained in [../pr-ci-coverage.json](../pr-ci-coverage.json).
- No test depends on the public internet or wall-clock waits measured in minutes.
- The workflow clearly labels this layer as source validation, not VM acceptance.
