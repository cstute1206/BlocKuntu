# focus-hosts Implementation Details

This document explains how the tool works internally.

## Commands

- `status`: prints config paths, tier counts, and remaining `open-for` uses in the rolling hour.
- `explain URL`: classifies a URL as Tier 1, Tier 2, or unknown.
- `examples`: prints safe quoted `open-for` examples for configured Tier 2 sites.
- `rebuild`: regenerates the managed `/etc/hosts` block with all Tier 1 and Tier 2 domains blocked.
- `lock`: runs `chattr +i` on the configured hosts file.
- `unlock`: runs `chattr -i` on the configured hosts file.
- `open-for URL`: temporarily removes one Tier 2 site's hosts entries, opens a disposable Firefox session, and schedules a restore.
- `restore-site SITE`: restores all configured hosts blocks. This is used by `systemd-run`.
- `watch-repair`: repairs `/etc/hosts` after manual edits unless an intentional `open-for` window is active.
- `install-watchdog`: writes and enables the systemd path/service units.
- `uninstall-watchdog`: disables and removes the systemd path/service units.
- `install-cli`: installs the current binary to `/usr/local/bin/focus-hosts`.
- `uninstall-cli`: removes the installed global binary.
- `summary`: summarizes local JSONL usage metrics for today, week, or month.
- `schedule-status`: shows active recurring schedules.
- `schedule-apply`: rebuilds the hosts file using active schedule policy.
- `install-schedules` / `uninstall-schedules`: manage the systemd schedule timer/service.
- `gui`: serves the local web dashboard.

## Config

The default config path is:

```text
/etc/focus-hosts/config.yml
```

The config has three main areas:

- `tier1`: domains that are always blocked.
- `tier2`: named sites that can be opened briefly.
- `tier2.NAME.example_url`: optional safe example URL shown by `status` and `examples`.
- `allowances.NAME.daily_minutes`: optional daily Tier 2 minute allowance for a site.
- `allowances.NAME.max_session_minutes`: optional per-opening cap for a site.
- `schedules.NAME`: optional recurring schedule with `days`, `start`, `end`, `tier1_extra`, `tier2_enabled`, and `mode`.
- `settings`: paths and global policy such as `open_limit_per_hour`.

## Hosts File Management

The tool only owns the block between these markers:

```text
# BEGIN focus-hosts
# END focus-hosts
```

Anything outside that block is preserved.

Rebuild flow:

1. Read the current hosts file.
2. Strip the previous managed block.
3. Evaluate active schedules.
4. Render a fresh managed block.
5. Run `chattr -i` on the hosts file.
6. Write the new file.
7. Run `chattr +i` on the hosts file.

Active schedules can add `tier1_extra` domains or Tier 2 site names. They can also set `tier2_enabled: false`, which omits Tier 2 blocks while the schedule is active.

If the generated content is already identical to the current file, the tool skips the write and only reapplies `chattr +i`.

## Allowances

Allowances are derived from the existing JSONL log instead of a separate quota file.

`open-for` applies caps in this order:

1. Requested/default site minutes.
2. Site `max_minutes`.
3. Optional allowance `max_session_minutes`.
4. Remaining `daily_minutes` since local midnight.

If no allowance remains, the request is denied and logged.

## Schedules

Schedules use local time and support windows that cross midnight.

`install-schedules` creates:

```text
/etc/systemd/system/focus-hosts-schedule.service
/etc/systemd/system/focus-hosts-schedule.timer
```

The timer runs once per minute and calls:

```bash
focus-hosts --config /etc/focus-hosts/config.yml schedule-apply
```

`schedule-apply` rebuilds the hosts file using the currently active schedules.

## Statistics

`summary --today`, `summary --week`, and `summary --month` read the local JSONL log and report temporary openings, total opened minutes, denied attempts, restores, watchdog repairs, top opened sites, and common reasons.

## GUI

`gui` starts a local HTTP server on `127.0.0.1:9876` by default.

The server exposes:

- `/`: embedded dashboard HTML.
- `/style.css`: embedded dashboard CSS.
- `/app.js`: embedded dashboard JavaScript.
- `/api/dashboard`: JSON state derived from config, runtime state, schedules, allowances, and logs.
- `POST /api/rebuild`: runs the existing hosts rebuild flow.
- `POST /api/close-current`: closes the current temporary opening through the same restore flow as `restore-site`.

The first GUI pass is dashboard-first. Other navigation areas are scaffolded, but editing tiers, schedules, settings, and locks is intentionally left for later so the GUI does not bypass CLI policy.

## Tauri Desktop Shell

The Tauri app lives in:

```text
app/
src-tauri/
```

The frontend is a Vite app that uses `@tauri-apps/api` to invoke Rust commands:

- `dashboard_json`
- `rebuild_hosts`
- `close_current`

The Tauri backend imports the current CLI source as a Rust module and calls public GUI wrapper functions. This keeps the desktop shell on the same config, log, schedule, allowance, rebuild, and restore behavior as the CLI.

KDE Breeze SVG icons are vendored in `app/assets/icons/breeze/` with attribution and LGPL license files.

## Tier 2 Opening Flow

`open-for URL` does this:

1. Parse and classify the URL.
2. Deny Tier 1 and unknown domains.
3. Count successful `allow` log entries in the last hour.
4. Deny if the 2-per-hour limit is reached.
5. Ask for a reason unless `--reason` is provided.
6. Write runtime state to `/run/focus-hosts/open.json`.
7. Rebuild hosts with that Tier 2 site temporarily omitted.
8. Create a temporary Firefox profile under `/tmp`.
9. Spawn Firefox with `--no-remote --profile TEMP_PROFILE --new-window URL`.
10. Store the Firefox process group ID and profile path in runtime state.
11. Schedule `restore-site` using `sudo systemd-run`.
12. Append an `allow` log entry.
13. Print a remaining-time countdown unless `--no-countdown` is used.

The restore job kills the stored Firefox process group, deletes the temporary profile, removes runtime state, rebuilds all blocks, logs the restore, and reapplies `chattr +i`.

## Watchdog

`install-watchdog` creates:

```text
/etc/systemd/system/focus-hosts-watchdog.path
/etc/systemd/system/focus-hosts-watchdog.service
```

The path unit watches the configured hosts file with `PathChanged=`.

The service runs:

```bash
focus-hosts --config /etc/focus-hosts/config.yml watch-repair
```

The path unit is enabled with:

```bash
systemctl enable --now focus-hosts-watchdog.path
```

That means it starts immediately and starts automatically on boot.

## Runtime State

The runtime state file is:

```text
/run/focus-hosts/open.json
```

It exists only while an intentional `open-for` window is active. The watchdog checks this file before repairing `/etc/hosts`; if the window has not expired yet, the watchdog logs `watchdog-skip` and only reapplies `chattr +i`.

The state also stores the temporary Firefox process group ID and profile path so `restore-site` can close the browser session even if the tab is still open.

## Logging

The default log path is:

```text
~/.local/state/focus-hosts/access.jsonl
```

The log is JSONL. Each line is one event. Appends use a file lock so restore and watchdog processes do not interleave their writes.

Malformed historical log lines are skipped with a warning instead of blocking commands like `open-for`. This keeps one damaged line from breaking future access-window checks.

The code tries to repair log ownership if a root-run command accidentally created the log or its directory as root.

## Firefox

Normal case:

```bash
firefox --no-remote --profile /tmp/focus-hosts-firefox-SITE-PID-TS --new-window URL
```

If `open-for` is accidentally run via `sudo`, the tool checks `SUDO_USER` and tries to run Firefox as that original user while preserving desktop session environment variables like `DISPLAY`, `XAUTHORITY`, and `DBUS_SESSION_BUS_ADDRESS`.

The Firefox process is spawned in its own process group. `restore-site` sends `SIGTERM`, waits briefly, then sends `SIGKILL` to that process group. This is what prevents the site from continuing to work after refresh once the time window is over.

## Known Boundary

The tool adds friction and self-healing, not root-proof security. With unrestricted sudo, a user can still stop systemd units, edit configs, remove binaries, or clear immutable flags manually. The watchdog exists to make manual bypasses short-lived and annoying.
