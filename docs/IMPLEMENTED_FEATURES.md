# Implemented Features

This document lists what is currently implemented in `focus-hosts`.

## Core Blocking

- Two-tier domain model:
  - Tier 1 domains are always blocked.
  - Tier 2 domains are blocked by default but can be opened temporarily.
- `/etc/hosts` management through a marked block:
  - `# BEGIN focus-hosts`
  - `# END focus-hosts`
- Existing hosts entries outside the managed block are preserved.
- Configurable redirect IP, defaulting to `0.0.0.0`.
- Domain normalization for configured domains such as `https://reddit.com/` to `reddit.com`.

## CLI Commands

- `status`: prints configured paths, tier counts, and remaining `open-for` uses.
- `explain URL`: classifies a URL as Tier 1, Tier 2, or unknown.
- `examples`: prints safe quoted `open-for` commands for configured Tier 2 sites.
- `rebuild`: regenerates the managed hosts block.
- `lock`: applies `chattr +i` to the hosts file.
- `unlock`: applies `chattr -i` to the hosts file.
- `open-for URL`: opens a Tier 2 URL for a short timed window.
- `restore-site SITE`: restores all configured hosts blocks after an opening.
- `watch-repair`: repairs the hosts file after manual edits.
- `install-watchdog`: installs and enables the systemd watchdog units.
- `uninstall-watchdog`: removes the systemd watchdog units.
- `install-cli`: installs the current binary globally.
- `uninstall-cli`: removes the globally installed binary.
- `logs`: prints recent JSONL log entries in a readable format.
- `summary`: prints local usage statistics for today, the last week, or the last month.
- `schedule-status`: prints active recurring schedules and their current Tier 2 policy.
- `schedule-apply`: rebuilds the hosts file using the currently active schedules.
- `install-schedules`: installs and enables the systemd schedule timer.
- `uninstall-schedules`: removes the systemd schedule timer.
- `gui`: serves a local web dashboard for monitoring and safe actions.

## Temporary Tier 2 Access

- Tier 1 and unknown domains are denied.
- Successful openings are limited by `open_limit_per_hour`.
- A reason is required unless provided with `--reason`.
- Requested minutes are capped by the site's `max_minutes`.
- Optional daily allowances cap total Tier 2 minutes per site per local day.
- Optional per-site allowance session caps further limit a single opening.
- Optional per-site cooldowns are supported.
- Runtime state is written while the opening is active.
- Hosts entries for the opened Tier 2 site are temporarily omitted.
- Firefox is launched with a temporary disposable profile.
- The Firefox session runs in its own process group.
- A `systemd-run` restore job is scheduled.
- A live countdown is printed by default, with `--no-countdown` available for immediate return.
- Restore closes the temporary browser session, deletes the profile, clears runtime state, rebuilds the hosts file, and logs the restore.

## Watchdog

- `install-watchdog` writes:
  - `/etc/systemd/system/focus-hosts-watchdog.path`
  - `/etc/systemd/system/focus-hosts-watchdog.service`
- The path unit watches the configured hosts file.
- The service runs `focus-hosts --config CONFIG watch-repair`.
- `watch-repair` skips repair while a valid intentional opening is active.
- Expired runtime state is removed before repair.
- Repairs are logged.

## Statistics and Analytics

- `summary --today`, `summary --week`, and `summary --month` summarize local JSONL logs.
- Summary metrics include temporary openings, total opened minutes, denied attempts, restores, watchdog repairs, top opened sites, and common reasons.
- Analytics are local-only and derived from the existing log file.

## Breaks and Allowances

- Config supports an optional top-level `allowances` map keyed by Tier 2 site name.
- Allowances support `daily_minutes` and optional `max_session_minutes`.
- Allowance usage is derived from successful `allow` log entries since local midnight.
- `status` shows used and remaining allowance minutes for configured Tier 2 sites.

## Scheduled Blocks

- Config supports recurring schedules with days, start/end times, optional `tier1_extra`, optional `tier2_enabled`, and optional `mode`.
- Schedules can cross midnight, such as `23:00` to `06:00`.
- `tier1_extra` can reference a Tier 2 site name or a domain.
- Active schedules can promote configured sites/domains to Tier 1-style blocking.
- Active schedules can disable Tier 2 blocking entirely.
- A systemd timer can run `schedule-apply` once per minute.

## GUI

- `gui` starts a local HTTP server, defaulting to `127.0.0.1:9876`.
- The dashboard shows current block status, active schedules, open-for usage, allowances, recent activity, today's summary, top opened sites, common reasons, and configured paths.
- The GUI can rebuild hosts and close the current temporary opening through the same Rust policy layer as the CLI.
- Secondary navigation sections are scaffolded for tiers, schedules, logs, settings, locks, and future app blocking.
- A Tauri desktop shell is scaffolded under `app/` and `src-tauri/`.
- The Tauri frontend uses vendored KDE Breeze SVG icons with LGPL attribution.
- Tauri actions call Rust wrappers around the existing CLI policy layer instead of reimplementing policy in JavaScript.

## Configuration

- Default config path: `/etc/focus-hosts/config.yml`.
- `FOCUS_HOSTS_CONFIG` can override the config path.
- A local `focus-hosts.yml` is used if no explicit path or environment override exists.
- Settings support configurable hosts, log, and runtime state paths.
- `~` is expanded in configured paths.
- Tier 2 defaults are applied when optional values are missing.
- Tier 2 sites can define `example_url` for safer examples in `status` and `examples`.
- Optional `allowances` and `schedules` sections extend Tier 2 policy.

## Logging

- Default log path: `~/.local/state/focus-hosts/access.jsonl`.
- Log entries are JSONL.
- Logged actions include `allow`, `deny`, `restore`, `watchdog-skip`, and `watchdog-repair`.
- Recent `allow` entries are counted to enforce rolling-hour limits.
- Log appends are file-locked to avoid interleaved JSONL writes from concurrent processes.
- Malformed historical log lines are skipped with a warning.
- The tool attempts to repair log ownership after root-run commands create root-owned log files.

## Tests

- Unit tests cover URL classification, domain matching, normalization, Tier 2 example URLs, managed block stripping, hosts rendering, config parsing, tilde expansion, log reading, malformed log tolerance, rolling-hour allow counting, runtime state parsing, helper formatting, countdown formatting, and watchdog unit rendering.
- CLI integration tests cover `explain`, `status`, `examples`, and `logs` against temporary config/log/state files.
- CLI integration tests cover `summary` and `schedule-status`.
- Unit tests cover allowance accounting, schedule-aware hosts rendering, and schedule unit rendering.
- Unit tests cover dashboard payload generation for the GUI.
- Tests intentionally avoid privileged operations such as changing `/etc/hosts`, running `chattr`, launching Firefox, using `sudo`, or modifying systemd.
