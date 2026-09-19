# Allowlist Mode

Status: Tier 2 and Tier 3 website and application allowlists are implemented in source as of 2026-08-31. The exact packaged build still needs clean-VM acceptance testing.

BlocKuntu uses **allowlist** for “allow only listed targets” and **blocklist** for the existing “block listed targets” behavior. Tier 1 allowlists are intentionally unsupported. Saving an allowlist does not activate it; an attached schedule or selected Detox session does.

## Policy semantics

Every website and application list has a mode:

```text
blocklist: block a target when any entry in the list matches
allowlist: block a target when no entry in the list matches
```

Entries in one allowlist are alternatives. Multiple active allowlists are intersected, so a target must match every active allowlist. A matching blocklist always wins.

| Tier   | Blocklist                                                          | Allowlist                                                              |
| ------ | ------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| Tier 1 | Matching entries are always blocked while the list is enabled      | Not supported                                                          |
| Tier 2 | Matching entries are blocked during an attached schedule or Detox  | Non-matching targets are blocked during an attached schedule or Detox  |
| Tier 3 | Matching entries consume allowance and block after it is exhausted | Non-matching targets consume allowance and block after it is exhausted |

Tier 3 keeps the existing two-minute manual unlock. The rejecting list can be unlocked by entering its list ID or a target that it rejects in **Overview → Manual Unlock**. Unlocking one allowlist does not override another active allowlist or a matching blocklist.

## Website allowlists

Website allowlists evaluate top-level HTTP and HTTPS navigation. Browser-internal pages, extension pages, `file://` URLs, and subresources loaded by a page are outside this boundary. Redirect and sign-in pages are top-level navigations and may need separate entries.

The browser extension provides enforcement for both new navigations and already-open tabs. The hosts file is not used for allowlists because it cannot express “block every domain except these entries.”

Allowlist activation is independent of the Chromium private-browsing choice. **Allow with manual extension consent** is the default. Regular windows remain protected. Full private-window protection requires either manually granting the extension incognito access or selecting **Disable private browsing**, which is the recommended setting. Chromium private-browsing controls are locked while a website allowlist is active.

## Application allowlists

Application allowlists evaluate regular-user processes individually. They do not try to infer which launchers, helpers, children, or reparented processes belong to one logical application.

### Process evaluation

During each scan, BlocKuntu evaluates every process that belongs to a regular desktop-user account. Each process is matched using only its own available identity:

- executable path;
- executable basename;
- command name;
- desktop ID inherited by that process;
- detected X11 window titles owned by that process.

A match never transfers to a parent, child, sibling, helper, renderer, or another process with a different identity. For example, allowing a browser's main executable does not implicitly allow a separate crash handler. Every related process must match an allowed entry itself.

This behavior does not depend on Wayland window discovery. Desktop IDs and X11 titles can improve a particular process identity when available, but neither is required for a process to be evaluated.

### Recovery exemptions

The normal GUI cannot remove the built-in safety boundary. It excludes:

- the BlocKuntu daemon, GUI, Native Messaging components, and descendants required by those processes;
- PID 1 and processes owned by root or non-desktop system accounts;
- processes whose regular-user identity cannot be established.

Regular-user session infrastructure is deliberately not exempt. This keeps process-level semantics predictable but can destabilize or terminate the desktop session.

### Tier 2 enforcement

While its schedule or Detox is active, a Tier 2 application allowlist sends `SIGTERM` to every eligible process that does not match the list. The event log records every terminated process and its rejecting rule. Desktop notifications are limited to one process notification per rejecting rule during a scan.

Enforcement runs on the daemon's periodic process scan, which defaults to ten seconds. BlocKuntu therefore promptly closes a rejected application after it is detected; it does not prevent the initial process execution at the kernel boundary.

### Tier 3 allowance

Tier 3 counts wall-clock time while at least one non-allowlisted eligible process is running during an attached schedule or Detox. Background and minimized processes count because metering is based on the process snapshot rather than keyboard focus or the active window.

Usage is recorded once per rule even when several rejected processes are running. Matching processes do not consume that allowlist's allowance. After exhaustion, rejected processes are terminated unless a manual unlock for that rule is active.

### Process-level risks

An incomplete allowlist may terminate essential regular-user processes, including:

- `systemd --user` and D-Bus services;
- desktop portals and authentication agents;
- audio and clipboard infrastructure;
- browser renderers, GPU processes, and crash handlers;
- IDE language servers and terminal child processes.

The save warning lists these risks. **Add all detected** is a starting snapshot, not a guarantee that future helpers or services will already have a matching identity.

## GUI workflow

The application editor offers **Block listed applications** and **Allow only listed applications**. Selecting allow-only mode moves a new Tier 1 draft to Tier 2 and keeps Tier 1 unavailable.

For application allowlists:

- allowed identities are collapsed by default so large lists do not dominate the editor;
- detected processes are also collapsed and show the current eligible process count;
- **Add all detected** adds one preferred identity for each currently detected process without expanding the detected list and deduplicates identical matchers;
- expanding the section provides search, refresh, and individual **Add** actions;
- automatic identity selection prefers desktop ID, then exact executable path, executable basename, and command name;
- saving always opens the same warning-dialog style used for website allowlists;
- no preview step is required. VM testing is the acceptance path for the first implementation.

An application allowlist must contain at least one identity. It remains inactive until it is attached to a schedule or selected for Detox.

## Active-edit safety

Only changes that make an active restriction stricter are accepted:

| Active list | Allowed edit        | Rejected edit                                      |
| ----------- | ------------------- | -------------------------------------------------- |
| Blocklist   | Append blocked rows | Remove/edit rows or change protected list settings |
| Allowlist   | Remove allowed rows | Add/edit rows or change protected list settings    |

Name, tier, schedules, allowance, enabled state, and mode remain locked while the list is active. Application and website rules use the same server-side check, so bypassing the GUI does not weaken an active list.

## Data and enforcement paths

| Area                         | Allowlist responsibility                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------ |
| `focus-core/src/config.rs`   | Shared `ListMode`, Tier 1 rejection, and enabled-state validation                          |
| `focus-core/src/policy.rs`   | Mode-aware matching, schedule/Detox activation, Tier 3 metering, and unlock resolution     |
| `focus-core/src/db.rs`       | SQLite mode and allowance persistence                                                      |
| `focusd/src/process_scan.rs` | `/proc` identity collection, recovery exemptions, and process termination                  |
| `focusd/src/app.rs`          | Per-process allowlist enforcement and per-rule Tier 3 usage sessions                       |
| `focusd/src/rpc.rs`          | Mode-aware active edits, detected-process snapshots, block reasons, and policy persistence |
| `focusd/src/hosts.rs`        | Excludes website allowlists from the block-only hosts fallback                             |
| `focus-gui`                  | Mode controls, warnings, collapsed identity selection, and Add-all workflow                 |
| Browser extensions           | Top-level website enforcement and allowlist-specific blocked-page text                      |

The mandatory unsupported-browser rule remains an enabled Tier 1 blocklist and cannot be converted to an allowlist.

## Known limitations

- Process identities vary between package formats, launch methods, and application versions. A saved matcher may therefore not cover a later helper process.
- Window-title matching remains X11-only and may be unavailable to the privileged daemon when it cannot access the graphical session. Executable, command, and desktop-ID matching do not depend on it.
- Termination currently sends `SIGTERM`; there is no later `SIGKILL` escalation.
- Short-lived processes that start and exit between process scans may not be observed.
- Terminating regular-user session infrastructure may log the user out, remove audio or clipboard functionality, or otherwise destabilize the desktop.
- BlocKuntu cannot protect against unrestricted root or sudo access.
- Compatibility with policies or databases written by older BlocKuntu versions is not an implementation goal for this feature.

## Source validation and VM acceptance

Automated source tests cover:

- Tier 2 schedule and Detox inversion;
- Tier 3 allowance metering, exhaustion, and manual unlock;
- SQLite mode round-tripping;
- active allowlist edit restrictions;
- independent evaluation of allowed and rejected regular-user processes;
- inclusion of regular-user D-Bus and session processes in allowlist enforcement;
- recovery exemptions for BlocKuntu and non-desktop system accounts;
- metering several rejected processes only once per rule.

The installed package still needs VM acceptance. At minimum:

1. Create Tier 2 and Tier 3 application allowlists and use **Add all detected**.
2. Attach each list to a schedule and activate each through Detox.
3. Confirm every required process identity for a multi-process application is present and the application remains usable.
4. Confirm a background or minimized rejected Tier 3 process consumes allowance, then closes after exhaustion.
5. Request a manual unlock using the application list ID and relaunch the rejected application.
6. Test a terminal, an IDE, and a browser including their renderers, GPU processes, language servers, crash handlers, and child processes.
7. Confirm required D-Bus, portal, authentication, audio, and clipboard processes were explicitly allowed and remain healthy.
8. Confirm the GUI, daemon, Native Messaging, and protected uninstall remain available through the BlocKuntu recovery exemption.

Source validation is not evidence that the packaged VM behavior passed; record those results separately after testing the exact built package.
