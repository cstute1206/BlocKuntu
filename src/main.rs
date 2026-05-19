use anyhow::{anyhow, bail, Context, Result};
use chrono::{
    DateTime, Datelike, Duration, Local, LocalResult, NaiveTime, TimeZone, Timelike, Utc, Weekday,
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::Duration as StdDuration;
use url::Url;

const BEGIN_MARKER: &str = "# BEGIN focus-hosts";
const END_MARKER: &str = "# END focus-hosts";
const DEFAULT_CONFIG: &str = "/etc/focus-hosts/config.yml";
const DEFAULT_HOSTS: &str = "/etc/hosts";
const DEFAULT_LOG: &str = "~/.local/state/focus-hosts/access.jsonl";
const DEFAULT_STATE: &str = "/run/focus-hosts/open.json";
const DEFAULT_REDIRECT_IP: &str = "0.0.0.0";
const DEFAULT_OPEN_LIMIT: usize = 2;
const DEFAULT_OPEN_MINUTES: u64 = 2;
const WATCHDOG_SERVICE: &str = "focus-hosts-watchdog.service";
const WATCHDOG_PATH: &str = "focus-hosts-watchdog.path";
const SCHEDULE_SERVICE: &str = "focus-hosts-schedule.service";
const SCHEDULE_TIMER: &str = "focus-hosts-schedule.timer";

#[derive(Parser, Debug)]
#[command(name = "focus-hosts")]
#[command(about = "Two-tier hosts-file blocker with short Tier 2 access windows")]
struct Cli {
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show configured tiers and remaining open-for uses in this rolling hour.
    Status,
    /// Explain how a URL is classified.
    Explain { url: String },
    /// Print safe quoted example open-for commands for configured Tier 2 sites.
    Examples,
    /// Rebuild /etc/hosts with all Tier 1 and Tier 2 domains blocked.
    Rebuild,
    /// Apply chattr +i to the configured hosts file.
    Lock,
    /// Remove chattr +i from the configured hosts file.
    Unlock,
    /// Open a Tier 2 URL by unblocking its domain for a short timed window.
    OpenFor {
        url: String,
        #[arg(short, long)]
        reason: Option<String>,
        #[arg(short, long)]
        minutes: Option<u64>,
        #[arg(long)]
        no_countdown: bool,
    },
    /// Restore all Tier 2 blocks. Intended for systemd-run timer use.
    RestoreSite { site: String },
    /// Repair /etc/hosts unless a deliberate open-for window is active.
    WatchRepair,
    /// Install and enable the systemd watchdog path/service.
    InstallWatchdog,
    /// Disable and remove the systemd watchdog path/service.
    UninstallWatchdog,
    /// Install this binary to a location on PATH.
    InstallCli {
        #[arg(short, long, default_value = "/usr/local/bin/focus-hosts")]
        dest: PathBuf,
    },
    /// Remove the installed focus-hosts binary.
    UninstallCli {
        #[arg(short, long, default_value = "/usr/local/bin/focus-hosts")]
        dest: PathBuf,
    },
    /// Print recent log lines.
    Logs {
        #[arg(short, long, default_value_t = 25)]
        tail: usize,
    },
    /// Print local-only usage statistics from the JSONL log.
    Summary {
        #[arg(long)]
        today: bool,
        #[arg(long)]
        week: bool,
        #[arg(long)]
        month: bool,
    },
    /// Show active recurring schedules and their current block policy.
    ScheduleStatus,
    /// Rebuild /etc/hosts using the currently active schedules.
    ScheduleApply,
    /// Install and enable a systemd timer that reapplies schedules every minute.
    InstallSchedules,
    /// Disable and remove the systemd schedule timer/service.
    UninstallSchedules,
    /// Serve the local web dashboard GUI.
    Gui {
        #[arg(long, default_value = "127.0.0.1:9876")]
        bind: String,
    },
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default)]
    tier1: Vec<String>,
    #[serde(default)]
    tier2: BTreeMap<String, Tier2Site>,
    #[serde(default)]
    allowances: BTreeMap<String, Allowance>,
    #[serde(default)]
    schedules: BTreeMap<String, Schedule>,
    #[serde(default)]
    settings: Settings,
}

#[derive(Debug, Deserialize)]
struct Tier2Site {
    domains: Vec<String>,
    #[serde(default)]
    example_url: Option<String>,
    #[serde(default = "default_open_minutes")]
    default_minutes: u64,
    #[serde(default = "default_open_minutes")]
    max_minutes: u64,
    #[serde(default)]
    cooldown_seconds: u64,
}

#[derive(Debug, Deserialize)]
struct Allowance {
    daily_minutes: u64,
    #[serde(default)]
    max_session_minutes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Schedule {
    #[serde(default)]
    days: Vec<String>,
    start: String,
    end: String,
    #[serde(default)]
    tier1_extra: Vec<String>,
    #[serde(default)]
    tier2_enabled: Option<bool>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Settings {
    #[serde(default = "default_hosts_path")]
    hosts_path: PathBuf,
    #[serde(default = "default_log_path")]
    log_path: PathBuf,
    #[serde(default = "default_state_path")]
    state_path: PathBuf,
    #[serde(default = "default_open_limit")]
    open_limit_per_hour: usize,
    #[serde(default = "default_redirect_ip")]
    redirect_ip: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hosts_path: default_hosts_path(),
            log_path: default_log_path(),
            state_path: default_state_path(),
            open_limit_per_hour: DEFAULT_OPEN_LIMIT,
            redirect_ip: DEFAULT_REDIRECT_IP.to_string(),
        }
    }
}

#[derive(Debug)]
enum Classification<'a> {
    Tier1 {
        domain: &'a str,
    },
    Tier2 {
        site: &'a str,
        site_cfg: &'a Tier2Site,
    },
    Unknown {
        host: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct LogEntry {
    ts: DateTime<Utc>,
    action: String,
    site: Option<String>,
    url: Option<String>,
    reason: Option<String>,
    minutes: Option<u64>,
    detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RuntimeState {
    site: String,
    expires_at: DateTime<Utc>,
    #[serde(default)]
    browser_pgid: Option<u32>,
    #[serde(default)]
    profile_path: Option<PathBuf>,
}

#[derive(Debug)]
struct BrowserSession {
    process_group_id: u32,
    profile_path: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = find_config_path(cli.config.as_deref())?;
    let cfg = load_config(&config_path)?;

    match cli.command {
        Commands::Status => status(&cfg),
        Commands::Explain { url } => explain(&cfg, &url),
        Commands::Examples => print_examples(&cfg),
        Commands::Rebuild => {
            rebuild_hosts(&cfg, None)?;
            println!(
                "Rebuilt {} and applied chattr +i.",
                cfg.settings.hosts_path.display()
            );
            Ok(())
        }
        Commands::Lock => {
            set_immutable(&cfg.settings.hosts_path, true)?;
            println!(
                "Locked {} with chattr +i.",
                cfg.settings.hosts_path.display()
            );
            Ok(())
        }
        Commands::Unlock => {
            set_immutable(&cfg.settings.hosts_path, false)?;
            println!(
                "Unlocked {} with chattr -i.",
                cfg.settings.hosts_path.display()
            );
            Ok(())
        }
        Commands::OpenFor {
            url,
            reason,
            minutes,
            no_countdown,
        } => open_for(&cfg, &config_path, &url, reason, minutes, no_countdown),
        Commands::RestoreSite { site } => restore_site(&cfg, site),
        Commands::WatchRepair => watch_repair(&cfg),
        Commands::InstallWatchdog => install_watchdog(&cfg, &config_path),
        Commands::UninstallWatchdog => uninstall_watchdog(),
        Commands::InstallCli { dest } => install_cli(&dest),
        Commands::UninstallCli { dest } => uninstall_cli(&dest),
        Commands::Logs { tail } => print_logs(&cfg.settings.log_path, tail),
        Commands::Summary { today, week, month } => {
            print_summary(&cfg.settings.log_path, summary_period(today, week, month))
        }
        Commands::ScheduleStatus => schedule_status(&cfg),
        Commands::ScheduleApply => {
            rebuild_hosts(&cfg, None)?;
            print_schedule_state(&cfg, Local::now());
            Ok(())
        }
        Commands::InstallSchedules => install_schedules(&config_path),
        Commands::UninstallSchedules => uninstall_schedules(),
        Commands::Gui { bind } => run_gui_server(&config_path, &bind),
    }
}

fn status(cfg: &Config) -> Result<()> {
    let used = count_recent_allows(&cfg.settings.log_path, Utc::now() - Duration::hours(1))?;
    let remaining = cfg.settings.open_limit_per_hour.saturating_sub(used);
    let runtime_state = read_runtime_state(&cfg.settings.state_path)?;

    println!("hosts: {}", cfg.settings.hosts_path.display());
    println!("log: {}", cfg.settings.log_path.display());
    println!("state: {}", cfg.settings.state_path.display());
    println!("Tier 1 domains: {}", cfg.tier1.len());
    println!("Tier 2 sites: {}", cfg.tier2.len());
    println!(
        "open-for remaining this rolling hour: {}/{}",
        remaining, cfg.settings.open_limit_per_hour
    );
    print_schedule_state(cfg, Local::now());

    if let Some(state) = runtime_state {
        if state.expires_at > Utc::now() {
            println!(
                "active open-for: {} remaining {}",
                state.site,
                format_remaining(state.expires_at - Utc::now())
            );
        } else {
            println!("active open-for: expired state for {}", state.site);
        }
    }

    for (name, site) in &cfg.tier2 {
        if let Some(allowance) = allowance_status(cfg, name, Utc::now())? {
            println!(
                "- {name}: {} minute window, allowance today: {}/{} minute(s) used, {} remaining, domains: {}, example: \"{}\"",
                site.default_minutes,
                allowance.used_minutes,
                allowance.daily_minutes,
                allowance.remaining_minutes,
                site.domains.join(", "),
                example_url_for_site(site)
            );
            continue;
        }

        println!(
            "- {name}: {} minute window, domains: {}, example: \"{}\"",
            site.default_minutes,
            site.domains.join(", "),
            example_url_for_site(site)
        );
    }

    Ok(())
}

fn explain(cfg: &Config, raw_url: &str) -> Result<()> {
    match classify_url(cfg, raw_url)? {
        Classification::Tier1 { domain } => {
            println!("Denied: {domain} is Tier 1 and cannot be opened.");
        }
        Classification::Tier2 { site, site_cfg } => {
            println!(
                "Allowed through open-for: {site} opens for {} minutes.",
                site_cfg.default_minutes
            );
        }
        Classification::Unknown { host } => {
            println!("Denied: {host} is not configured as Tier 2.");
        }
    }
    Ok(())
}

fn print_examples(cfg: &Config) -> Result<()> {
    println!("Use quotes around URLs so shell characters like '&' stay inside the URL.");
    println!();

    for (name, site) in &cfg.tier2 {
        let url = example_url_for_site(site);
        println!("{name}:");
        println!("  focus-hosts open-for \"{url}\"");
        println!("  focus-hosts open-for \"{url}\" --reason \"short intentional break\"");
        println!();
    }

    Ok(())
}

fn example_url_for_site(site: &Tier2Site) -> String {
    site.example_url.clone().unwrap_or_else(|| {
        let domain = site
            .domains
            .first()
            .map(String::as_str)
            .unwrap_or("example.com");
        format!("https://{domain}/")
    })
}

fn open_for(
    cfg: &Config,
    config_path: &Path,
    raw_url: &str,
    reason_arg: Option<String>,
    minutes_arg: Option<u64>,
    no_countdown: bool,
) -> Result<()> {
    let (site_name, site_cfg) = match classify_url(cfg, raw_url)? {
        Classification::Tier1 { domain } => {
            append_log(
                &cfg.settings.log_path,
                LogEntry {
                    ts: Utc::now(),
                    action: "deny".to_string(),
                    site: None,
                    url: Some(raw_url.to_string()),
                    reason: None,
                    minutes: None,
                    detail: Some(format!("{domain} is Tier 1")),
                },
            )?;
            bail!("Denied: {domain} is Tier 1 and cannot be opened.");
        }
        Classification::Tier2 { site, site_cfg } => (site.to_string(), site_cfg),
        Classification::Unknown { host } => {
            append_log(
                &cfg.settings.log_path,
                LogEntry {
                    ts: Utc::now(),
                    action: "deny".to_string(),
                    site: None,
                    url: Some(raw_url.to_string()),
                    reason: None,
                    minutes: None,
                    detail: Some(format!("{host} is not configured as Tier 2")),
                },
            )?;
            bail!("Denied: {host} is not configured as Tier 2.");
        }
    };

    let now_local = Local::now();
    if active_schedules_block_site(cfg, &site_name, now_local) {
        append_log(
            &cfg.settings.log_path,
            LogEntry {
                ts: Utc::now(),
                action: "deny".to_string(),
                site: Some(site_name.clone()),
                url: Some(raw_url.to_string()),
                reason: None,
                minutes: None,
                detail: Some("active schedule promotes site to Tier 1".to_string()),
            },
        )?;
        bail!("Denied: {raw_url} is blocked by an active schedule and cannot be opened.");
    }
    let active = active_schedules(cfg, now_local);
    if !tier2_enabled_for_schedules(&active_schedule_refs(cfg, &active)) {
        println!("No open-for needed: Tier 2 blocking is disabled by the active schedule policy.");
        return Ok(());
    }

    let used = count_recent_allows(&cfg.settings.log_path, Utc::now() - Duration::hours(1))?;
    if used >= cfg.settings.open_limit_per_hour {
        append_log(
            &cfg.settings.log_path,
            LogEntry {
                ts: Utc::now(),
                action: "deny".to_string(),
                site: Some(site_name.clone()),
                url: Some(raw_url.to_string()),
                reason: None,
                minutes: None,
                detail: Some("open-for hourly limit reached".to_string()),
            },
        )?;
        bail!(
            "Denied: open-for was already used {used} time(s) in the last hour. Limit is {}.",
            cfg.settings.open_limit_per_hour
        );
    }

    let minutes = minutes_arg
        .unwrap_or(site_cfg.default_minutes)
        .min(site_cfg.max_minutes);
    let minutes = cap_minutes_by_allowance(cfg, &site_name, minutes, Utc::now())?;
    if minutes == 0 {
        append_log(
            &cfg.settings.log_path,
            LogEntry {
                ts: Utc::now(),
                action: "deny".to_string(),
                site: Some(site_name),
                url: Some(raw_url.to_string()),
                reason: None,
                minutes: None,
                detail: Some("daily allowance exhausted".to_string()),
            },
        )?;
        bail!("Denied: daily allowance for this site is exhausted.");
    }

    let reason = match reason_arg {
        Some(reason) => reason,
        None => prompt_reason()?,
    };

    if site_cfg.cooldown_seconds > 0 {
        println!(
            "Cooldown: waiting {} second(s) before opening {site_name}.",
            site_cfg.cooldown_seconds
        );
        thread::sleep(StdDuration::from_secs(site_cfg.cooldown_seconds));
    }

    write_runtime_state(
        &cfg.settings.state_path,
        &RuntimeState {
            site: site_name.clone(),
            expires_at: Utc::now() + Duration::minutes(minutes as i64),
            browser_pgid: None,
            profile_path: None,
        },
    )?;

    if let Err(err) = rebuild_hosts(cfg, Some(&site_name)) {
        let _ = remove_runtime_state(&cfg.settings.state_path);
        return Err(err);
    }
    let browser_session = match start_firefox_session(&site_name, raw_url) {
        Ok(session) => session,
        Err(err) => {
            let _ = remove_runtime_state(&cfg.settings.state_path);
            let _ = rebuild_hosts(cfg, None);
            return Err(err);
        }
    };

    if let Err(err) = write_runtime_state(
        &cfg.settings.state_path,
        &RuntimeState {
            site: site_name.clone(),
            expires_at: Utc::now() + Duration::minutes(minutes as i64),
            browser_pgid: Some(browser_session.process_group_id),
            profile_path: Some(browser_session.profile_path.clone()),
        },
    ) {
        let _ = stop_browser_process_group(browser_session.process_group_id);
        let _ = remove_path(&browser_session.profile_path);
        let _ = remove_runtime_state(&cfg.settings.state_path);
        let _ = rebuild_hosts(cfg, None);
        return Err(err);
    }

    if let Err(err) = schedule_restore(config_path, &site_name, minutes) {
        let _ = stop_browser_process_group(browser_session.process_group_id);
        let _ = remove_path(&browser_session.profile_path);
        let _ = remove_runtime_state(&cfg.settings.state_path);
        let _ = rebuild_hosts(cfg, None);
        return Err(err);
    }

    if let Err(err) = append_log(
        &cfg.settings.log_path,
        LogEntry {
            ts: Utc::now(),
            action: "allow".to_string(),
            site: Some(site_name.clone()),
            url: Some(raw_url.to_string()),
            reason: Some(reason),
            minutes: Some(minutes),
            detail: Some("temporary Tier 2 opening".to_string()),
        },
    ) {
        let _ = stop_browser_process_group(browser_session.process_group_id);
        let _ = remove_path(&browser_session.profile_path);
        let _ = remove_runtime_state(&cfg.settings.state_path);
        let _ = rebuild_hosts(cfg, None);
        return Err(err);
    }

    println!(
        "{site_name} is open for {minutes} minute(s) in a temporary Firefox session. A restore job was scheduled."
    );
    if !no_countdown {
        print_countdown_until(Utc::now() + Duration::minutes(minutes as i64))?;
    }
    Ok(())
}

fn watch_repair(cfg: &Config) -> Result<()> {
    if let Some(state) = read_runtime_state(&cfg.settings.state_path)? {
        if state.expires_at > Utc::now() {
            append_log(
                &cfg.settings.log_path,
                LogEntry {
                    ts: Utc::now(),
                    action: "watchdog-skip".to_string(),
                    site: Some(state.site),
                    url: None,
                    reason: None,
                    minutes: None,
                    detail: Some(format!(
                        "temporary open-for window active until {}",
                        state.expires_at
                    )),
                },
            )?;
            set_immutable(&cfg.settings.hosts_path, true)?;
            return Ok(());
        }

        remove_runtime_state(&cfg.settings.state_path)?;
    }

    rebuild_hosts(cfg, None)?;
    append_log(
        &cfg.settings.log_path,
        LogEntry {
            ts: Utc::now(),
            action: "watchdog-repair".to_string(),
            site: None,
            url: None,
            reason: None,
            minutes: None,
            detail: Some("restored configured hosts block after file change".to_string()),
        },
    )?;
    Ok(())
}

fn restore_site(cfg: &Config, site: String) -> Result<()> {
    if let Some(state) = read_runtime_state(&cfg.settings.state_path)? {
        stop_browser_session(&state)?;
    }
    remove_runtime_state(&cfg.settings.state_path)?;
    rebuild_hosts(cfg, None)?;
    append_log(
        &cfg.settings.log_path,
        LogEntry {
            ts: Utc::now(),
            action: "restore".to_string(),
            site: Some(site),
            url: None,
            reason: None,
            minutes: None,
            detail: Some("restored all configured hosts blocks".to_string()),
        },
    )?;
    Ok(())
}

fn rebuild_hosts(cfg: &Config, temporarily_allowed_site: Option<&str>) -> Result<()> {
    let current = fs::read_to_string(&cfg.settings.hosts_path).unwrap_or_default();
    let next = build_hosts_content(&current, cfg, temporarily_allowed_site);

    if current != next {
        write_hosts_file(&cfg.settings.hosts_path, &next)?;
    }
    set_immutable(&cfg.settings.hosts_path, true)?;
    Ok(())
}

fn build_hosts_content(
    current: &str,
    cfg: &Config,
    temporarily_allowed_site: Option<&str>,
) -> String {
    let unmanaged = strip_managed_block(current);
    let managed = render_managed_block(cfg, temporarily_allowed_site);

    if unmanaged.trim().is_empty() {
        format!("{managed}\n")
    } else {
        format!("{}\n\n{managed}\n", unmanaged.trim_end())
    }
}

fn render_managed_block(cfg: &Config, temporarily_allowed_site: Option<&str>) -> String {
    render_managed_block_at(cfg, temporarily_allowed_site, Local::now())
}

fn render_managed_block_at(
    cfg: &Config,
    temporarily_allowed_site: Option<&str>,
    now: DateTime<Local>,
) -> String {
    let mut lines = vec![
        BEGIN_MARKER.to_string(),
        "# Managed by focus-hosts. Manual edits inside this block will be replaced.".to_string(),
    ];
    let active = active_schedules(cfg, now);
    let active_refs = active_schedule_refs(cfg, &active);
    let tier2_enabled = tier2_enabled_for_schedules(&active_refs);

    for domain in &cfg.tier1 {
        lines.push(format!(
            "{} {}",
            cfg.settings.redirect_ip,
            normalize_domain(domain)
        ));
    }

    for domain in scheduled_tier1_extra_domains(cfg, &active) {
        lines.push(format!("{} {}", cfg.settings.redirect_ip, domain));
    }

    if tier2_enabled {
        for (site_name, site) in &cfg.tier2 {
            if Some(site_name.as_str()) == temporarily_allowed_site {
                lines.push(format!("# Tier 2 temporarily open: {site_name}"));
                continue;
            }

            for domain in &site.domains {
                lines.push(format!(
                    "{} {}",
                    cfg.settings.redirect_ip,
                    normalize_domain(domain)
                ));
            }
        }
    } else {
        lines.push("# Tier 2 disabled by active schedule".to_string());
    }

    lines.push(END_MARKER.to_string());
    lines.join("\n")
}

fn strip_managed_block(input: &str) -> String {
    let mut output = Vec::new();
    let mut in_block = false;

    for line in input.lines() {
        if line.trim() == BEGIN_MARKER {
            in_block = true;
            continue;
        }
        if line.trim() == END_MARKER {
            in_block = false;
            continue;
        }
        if !in_block {
            output.push(line);
        }
    }

    output.join("\n")
}

fn write_hosts_file(path: &Path, content: &str) -> Result<()> {
    set_immutable(path, false)?;
    write_root_file(path, content)
}

fn write_root_file(path: &Path, content: &str) -> Result<()> {
    if is_root() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
        return Ok(());
    }

    let tmp_path = std::env::temp_dir().join(format!("focus-hosts-{}.hosts", std::process::id()));
    fs::write(&tmp_path, content).with_context(|| format!("writing {}", tmp_path.display()))?;

    if let Some(parent) = path.parent() {
        run(Command::new("sudo").arg("mkdir").arg("-p").arg(parent))?;
    }
    run(Command::new("sudo").arg("cp").arg(&tmp_path).arg(path))?;
    let _ = fs::remove_file(&tmp_path);
    Ok(())
}

fn set_immutable(path: &Path, immutable: bool) -> Result<()> {
    let flag = if immutable { "+i" } else { "-i" };

    if is_root() {
        run(Command::new("chattr").arg(flag).arg(path))
            .with_context(|| format!("running chattr {flag} {}", path.display()))
    } else {
        run(Command::new("sudo").arg("chattr").arg(flag).arg(path))
            .with_context(|| format!("running sudo chattr {flag} {}", path.display()))
    }
}

fn start_firefox_session(site_name: &str, raw_url: &str) -> Result<BrowserSession> {
    let profile_path = create_temp_profile(site_name)?;
    let mut command = firefox_command(raw_url, &profile_path);
    let child = spawn_in_own_process_group(&mut command).with_context(|| {
        format!(
            "opening Firefox with temporary profile {}",
            profile_path.display()
        )
    })?;

    Ok(BrowserSession {
        process_group_id: child.id(),
        profile_path,
    })
}

fn firefox_command(raw_url: &str, profile_path: &Path) -> Command {
    if is_root() {
        if let Some(user) = std::env::var_os("SUDO_USER") {
            let mut command = Command::new("sudo");
            command.arg("-u").arg(user).arg("env");

            command.arg(format!("HOME={}", home_dir().display()));
            for key in [
                "DISPLAY",
                "XAUTHORITY",
                "DBUS_SESSION_BUS_ADDRESS",
                "WAYLAND_DISPLAY",
                "XDG_RUNTIME_DIR",
            ] {
                if let Some(value) = std::env::var_os(key) {
                    command.arg(format!("{key}={}", value.to_string_lossy()));
                }
            }

            command
                .arg("firefox")
                .arg("--no-remote")
                .arg("--profile")
                .arg(profile_path)
                .arg("--new-window")
                .arg(raw_url);
            return command;
        }
    }

    let mut command = Command::new("firefox");
    command
        .arg("--no-remote")
        .arg("--profile")
        .arg(profile_path)
        .arg("--new-window")
        .arg(raw_url);
    command
}

fn create_temp_profile(site_name: &str) -> Result<PathBuf> {
    let profile_path = std::env::temp_dir().join(format!(
        "focus-hosts-firefox-{}-{}-{}",
        sanitize_name(site_name),
        std::process::id(),
        Utc::now().timestamp()
    ));
    fs::create_dir_all(&profile_path)
        .with_context(|| format!("creating {}", profile_path.display()))?;

    if is_root() {
        if let Some((uid, gid)) = desired_user_owner() {
            chown_path_recursive(&profile_path, uid, gid)?;
        }
    }

    Ok(profile_path)
}

fn spawn_in_own_process_group(command: &mut Command) -> Result<Child> {
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command.spawn().context("spawning Firefox")
}

fn stop_browser_session(state: &RuntimeState) -> Result<()> {
    if let Some(process_group_id) = state.browser_pgid {
        stop_browser_process_group(process_group_id)?;
    }

    if let Some(profile_path) = &state.profile_path {
        remove_path(profile_path)?;
    }

    Ok(())
}

fn stop_browser_process_group(process_group_id: u32) -> Result<()> {
    let pgid = -(process_group_id as i32);
    send_signal(pgid, libc::SIGTERM)?;
    thread::sleep(StdDuration::from_millis(1500));
    send_signal(pgid, libc::SIGKILL)?;
    Ok(())
}

fn send_signal(pid: i32, signal: i32) -> Result<()> {
    let result = unsafe { libc::kill(pid, signal) };
    if result == 0 {
        return Ok(());
    }

    let err = io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }

    Err(err).with_context(|| format!("sending signal {signal} to pid {pid}"))
}

fn remove_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    if is_root() {
        fs::remove_dir_all(path).or_else(|err| {
            if err.kind() == io::ErrorKind::NotADirectory {
                fs::remove_file(path)
            } else {
                Err(err)
            }
        })?;
        return Ok(());
    }

    run(Command::new("rm").arg("-rf").arg(path))
}

fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn schedule_restore(config_path: &Path, site_name: &str, minutes: u64) -> Result<()> {
    let exe = std::env::current_exe().context("finding current executable path")?;
    let config_path = absolute_path(config_path)?;
    let delay = format!("{}s", minutes * 60);
    let unit = format!(
        "focus-hosts-restore-{}-{}",
        site_name,
        Utc::now().timestamp()
    );

    let status = Command::new("sudo")
        .arg("systemd-run")
        .arg("--unit")
        .arg(unit)
        .arg("--description")
        .arg("focus-hosts Tier 2 restore")
        .arg("--on-active")
        .arg(delay)
        .arg("--collect")
        .arg(format!("--setenv=HOME={}", home_dir().display()))
        .arg(exe)
        .arg("--config")
        .arg(config_path)
        .arg("restore-site")
        .arg(site_name)
        .status();

    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!("systemd-run failed with status {status}"),
        Err(err) => Err(err).context("scheduling restore with sudo systemd-run"),
    }
}

fn install_watchdog(cfg: &Config, config_path: &Path) -> Result<()> {
    let exe = absolute_path(&std::env::current_exe().context("finding current executable path")?)?;
    let config_path = absolute_path(config_path)?;
    let home = home_dir();
    let readme = absolute_path(Path::new("docs/README.md"))
        .unwrap_or_else(|_| PathBuf::from("docs/README.md"));
    let (service, path_unit) = render_watchdog_units(cfg, &exe, &config_path, &home, &readme);

    write_root_file(
        &PathBuf::from(format!("/etc/systemd/system/{WATCHDOG_SERVICE}")),
        &service,
    )?;
    write_root_file(
        &PathBuf::from(format!("/etc/systemd/system/{WATCHDOG_PATH}")),
        &path_unit,
    )?;
    run(Command::new("sudo").arg("systemctl").arg("daemon-reload"))?;
    run(Command::new("sudo")
        .arg("systemctl")
        .arg("enable")
        .arg("--now")
        .arg(WATCHDOG_PATH))?;

    println!("Installed and enabled {WATCHDOG_PATH}.");
    println!(
        "It watches {} and runs watch-repair after manual changes.",
        cfg.settings.hosts_path.display()
    );
    Ok(())
}

fn render_watchdog_units(
    cfg: &Config,
    exe: &Path,
    config_path: &Path,
    home: &Path,
    readme: &Path,
) -> (String, String) {
    let service = format!(
        "[Unit]\n\
         Description=Repair focus-hosts managed hosts block\n\
         Documentation=file://{readme}\n\n\
         [Service]\n\
         Type=oneshot\n\
         Environment=HOME={home}\n\
         ExecStart={exe} --config {config} watch-repair\n",
        readme = readme.display(),
        home = systemd_quote(&home.display().to_string()),
        exe = systemd_quote(&exe.display().to_string()),
        config = systemd_quote(&config_path.display().to_string()),
    );

    let path_unit = format!(
        "[Unit]\n\
         Description=Watch {hosts} for manual changes\n\n\
         [Path]\n\
         PathChanged={hosts}\n\
         Unit={service}\n\n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        hosts = cfg.settings.hosts_path.display(),
        service = WATCHDOG_SERVICE,
    );

    (service, path_unit)
}

fn uninstall_watchdog() -> Result<()> {
    let _ = run(Command::new("sudo")
        .arg("systemctl")
        .arg("disable")
        .arg("--now")
        .arg(WATCHDOG_PATH));
    run(Command::new("sudo")
        .arg("rm")
        .arg("-f")
        .arg(format!("/etc/systemd/system/{WATCHDOG_SERVICE}"))
        .arg(format!("/etc/systemd/system/{WATCHDOG_PATH}")))?;
    run(Command::new("sudo").arg("systemctl").arg("daemon-reload"))?;
    println!("Removed {WATCHDOG_PATH} and {WATCHDOG_SERVICE}.");
    Ok(())
}

fn install_cli(dest: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("finding current executable path")?;

    if is_root() {
        run(Command::new("install")
            .arg("-m")
            .arg("0755")
            .arg(&exe)
            .arg(dest))?;
    } else {
        run(Command::new("sudo")
            .arg("install")
            .arg("-m")
            .arg("0755")
            .arg(&exe)
            .arg(dest))?;
    }

    println!("Installed focus-hosts to {}.", dest.display());
    Ok(())
}

fn uninstall_cli(dest: &Path) -> Result<()> {
    if is_root() {
        run(Command::new("rm").arg("-f").arg(dest))?;
    } else {
        run(Command::new("sudo").arg("rm").arg("-f").arg(dest))?;
    }

    println!("Removed {}.", dest.display());
    Ok(())
}

fn systemd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn classify_url<'a>(cfg: &'a Config, raw_url: &str) -> Result<Classification<'a>> {
    let url = parse_user_url(raw_url).with_context(|| {
        format!(
            "invalid URL: {raw_url}. Use a full URL like \"https://www.youtube.com/watch?v=...\" and quote it if it contains '&'."
        )
    })?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("URL has no host: {raw_url}"))?
        .trim_end_matches('.')
        .to_ascii_lowercase();

    for domain in &cfg.tier1 {
        let domain = normalize_domain(domain);
        if domain_matches(&host, &domain) {
            return Ok(Classification::Tier1 { domain });
        }
    }

    for (site_name, site) in &cfg.tier2 {
        for domain in &site.domains {
            let domain = normalize_domain(domain);
            if domain_matches(&host, &domain) {
                return Ok(Classification::Tier2 {
                    site: site_name.as_str(),
                    site_cfg: site,
                });
            }
        }
    }

    Ok(Classification::Unknown { host })
}

fn parse_user_url(raw_url: &str) -> Result<Url> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        bail!("URL is empty");
    }

    match Url::parse(trimmed) {
        Ok(url) => Ok(url),
        Err(first_err) => {
            let with_scheme = format!("https://{trimmed}");
            Url::parse(&with_scheme).map_err(|_| first_err.into())
        }
    }
}

fn domain_matches(host: &str, configured: &str) -> bool {
    host == configured || host.ends_with(&format!(".{configured}"))
}

fn normalize_domain(domain: &str) -> &str {
    domain
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .trim_end_matches('.')
}

fn prompt_reason() -> Result<String> {
    print!("Reason: ");
    io::stdout().flush()?;

    let mut reason = String::new();
    io::stdin().read_line(&mut reason)?;
    let reason = reason.trim().to_string();

    if reason.is_empty() {
        bail!("A reason is required.");
    }

    Ok(reason)
}

fn print_countdown_until(expires_at: DateTime<Utc>) -> Result<()> {
    println!("Countdown started. Press Ctrl+C to hide it; the restore job is already scheduled.");

    loop {
        let remaining = expires_at - Utc::now();
        print!("\rremaining: {}", format_remaining(remaining));
        io::stdout().flush()?;

        if remaining.num_seconds() <= 0 {
            break;
        }

        thread::sleep(StdDuration::from_secs(1));
    }

    println!("\nWindow ended. The restore job should re-apply the hosts blocks.");
    Ok(())
}

fn format_remaining(duration: Duration) -> String {
    let total_seconds = duration.num_seconds().max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn append_log(path: &Path, entry: LogEntry) -> Result<()> {
    if let Some(parent) = path.parent() {
        if is_root() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        } else if let Err(err) = fs::create_dir_all(parent) {
            if err.kind() == io::ErrorKind::PermissionDenied {
                repair_log_ownership(path)?;
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            } else {
                return Err(err).with_context(|| format!("creating {}", parent.display()));
            }
        }
    }

    if let Err(err) = try_append_log(path, &entry) {
        if is_permission_denied(&err) && !is_root() {
            repair_log_ownership(path)?;
            try_append_log(path, &entry)?;
        } else {
            return Err(err);
        }
    }

    if is_root() {
        let _ = repair_log_ownership(path);
    }

    Ok(())
}

fn try_append_log(path: &Path, entry: &LogEntry) -> Result<()> {
    let line = serde_json::to_string(entry)? + "\n";
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let fd = file.as_raw_fd();
    lock_file(fd, path)?;

    let write_result = file
        .write_all(line.as_bytes())
        .and_then(|_| file.flush())
        .with_context(|| format!("writing {}", path.display()));
    let unlock_result = unlock_file(fd, path);

    write_result?;
    unlock_result?;
    Ok(())
}

fn lock_file(fd: i32, path: &Path) -> Result<()> {
    let result = unsafe { libc::flock(fd, libc::LOCK_EX) };
    if result == 0 {
        return Ok(());
    }

    Err(io::Error::last_os_error()).with_context(|| format!("locking {}", path.display()))
}

fn unlock_file(fd: i32, path: &Path) -> Result<()> {
    let result = unsafe { libc::flock(fd, libc::LOCK_UN) };
    if result == 0 {
        return Ok(());
    }

    Err(io::Error::last_os_error()).with_context(|| format!("unlocking {}", path.display()))
}

fn is_permission_denied(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|io_err| io_err.kind() == io::ErrorKind::PermissionDenied)
    })
}

fn repair_log_ownership(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let Some((uid, gid)) = desired_user_owner() else {
        return Ok(());
    };
    let owner = format!("{uid}:{gid}");

    if is_root() {
        run(Command::new("chown").arg("-R").arg(owner).arg(parent))
    } else {
        run(Command::new("sudo")
            .arg("chown")
            .arg("-R")
            .arg(owner)
            .arg(parent))
    }
}

fn chown_path_recursive(path: &Path, uid: u32, gid: u32) -> Result<()> {
    let owner = format!("{uid}:{gid}");

    if is_root() {
        run(Command::new("chown").arg("-R").arg(owner).arg(path))
    } else {
        run(Command::new("sudo")
            .arg("chown")
            .arg("-R")
            .arg(owner)
            .arg(path))
    }
}

fn desired_user_owner() -> Option<(u32, u32)> {
    let sudo_uid = std::env::var("SUDO_UID")
        .ok()
        .and_then(|value| value.parse().ok());
    let sudo_gid = std::env::var("SUDO_GID")
        .ok()
        .and_then(|value| value.parse().ok());
    if let (Some(uid), Some(gid)) = (sudo_uid, sudo_gid) {
        return Some((uid, gid));
    }

    let home = home_dir();
    let metadata = fs::metadata(home).ok()?;
    Some((metadata.uid(), metadata.gid()))
}

fn write_runtime_state(path: &Path, state: &RuntimeState) -> Result<()> {
    let content = serde_json::to_string(state)? + "\n";
    write_root_file(path, &content)
}

fn read_runtime_state(path: &Path) -> Result<Option<RuntimeState>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };

    if content.trim().is_empty() {
        return Ok(None);
    }

    let state = serde_json::from_str(content.trim())
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(state))
}

fn remove_runtime_state(path: &Path) -> Result<()> {
    if is_root() {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err).with_context(|| format!("removing {}", path.display())),
        }
    }

    run(Command::new("sudo").arg("rm").arg("-f").arg(path))
}

fn count_recent_allows(path: &Path, since: DateTime<Utc>) -> Result<usize> {
    let entries = read_logs(path)?;
    Ok(entries
        .into_iter()
        .filter(|entry| entry.action == "allow" && entry.ts >= since)
        .count())
}

fn read_logs(path: &Path) -> Result<Vec<LogEntry>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied && !is_root() => {
            repair_log_ownership(path)?;
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
        }
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };

    let mut entries = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(line) {
            Ok(entry) => entries.push(entry),
            Err(err) => eprintln!(
                "Warning: skipped malformed log line {} in {}: {}",
                idx + 1,
                path.display(),
                err
            ),
        }
    }
    Ok(entries)
}

fn print_logs(path: &Path, tail: usize) -> Result<()> {
    let entries = read_logs(path)?;
    let start = entries.len().saturating_sub(tail);

    for entry in &entries[start..] {
        println!(
            "{} {} {} {} {}",
            entry.ts,
            entry.action,
            entry.site.as_deref().unwrap_or("-"),
            entry
                .minutes
                .map(|minutes| format!("{minutes}m"))
                .unwrap_or_else(|| "-".to_string()),
            entry.url.as_deref().unwrap_or("-")
        );
        if let Some(reason) = &entry.reason {
            println!("  reason: {reason}");
        }
        if let Some(detail) = &entry.detail {
            println!("  detail: {detail}");
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum SummaryPeriod {
    Today,
    Week,
    Month,
}

#[derive(Debug)]
struct AllowanceStatus {
    daily_minutes: u64,
    used_minutes: u64,
    remaining_minutes: u64,
}

fn summary_period(_today: bool, week: bool, month: bool) -> SummaryPeriod {
    if month {
        SummaryPeriod::Month
    } else if week {
        SummaryPeriod::Week
    } else {
        SummaryPeriod::Today
    }
}

fn print_summary(path: &Path, period: SummaryPeriod) -> Result<()> {
    let now = Local::now();
    let since = match period {
        SummaryPeriod::Today => start_of_today_local(now),
        SummaryPeriod::Week => (now - Duration::days(7)).with_timezone(&Utc),
        SummaryPeriod::Month => (now - Duration::days(30)).with_timezone(&Utc),
    };
    let entries = read_logs(path)?;
    let entries = entries
        .into_iter()
        .filter(|entry| entry.ts >= since)
        .collect::<Vec<_>>();

    let mut opened_by_site: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut allows = 0usize;
    let mut denies = 0usize;
    let mut restores = 0usize;
    let mut repairs = 0usize;
    let mut total_minutes = 0u64;

    for entry in &entries {
        match entry.action.as_str() {
            "allow" => {
                allows += 1;
                let minutes = entry.minutes.unwrap_or(0);
                total_minutes += minutes;
                let site = entry.site.clone().unwrap_or_else(|| "-".to_string());
                let site_entry = opened_by_site.entry(site).or_default();
                site_entry.0 += 1;
                site_entry.1 += minutes;
                if let Some(reason) = &entry.reason {
                    *reasons.entry(reason.clone()).or_default() += 1;
                }
            }
            "deny" => denies += 1,
            "restore" => restores += 1,
            "watchdog-repair" => repairs += 1,
            _ => {}
        }
    }

    println!("Summary since {}", since.with_timezone(&Local));
    println!("temporary openings: {allows}");
    println!("total opened minutes: {total_minutes}");
    println!("denied attempts: {denies}");
    println!("restores: {restores}");
    println!("watchdog repairs: {repairs}");

    if !opened_by_site.is_empty() {
        println!("top opened sites:");
        let mut sites = opened_by_site.into_iter().collect::<Vec<_>>();
        sites.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then_with(|| a.0.cmp(&b.0)));
        for (site, (count, minutes)) in sites.into_iter().take(10) {
            println!("- {site}: {minutes} minute(s), {count} opening(s)");
        }
    }

    if !reasons.is_empty() {
        println!("common reasons:");
        let mut reasons = reasons.into_iter().collect::<Vec<_>>();
        reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for (reason, count) in reasons.into_iter().take(10) {
            println!("- {reason}: {count}");
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct GuiDashboard {
    generated_at: DateTime<Utc>,
    paths: GuiPaths,
    status: GuiStatus,
    current_opening: Option<GuiOpening>,
    allowances: Vec<GuiAllowance>,
    recent_activity: Vec<GuiActivity>,
    today_summary: GuiTodaySummary,
    top_sites_week: Vec<GuiRankedMetric>,
    common_reasons_week: Vec<GuiRankedMetric>,
    next_schedule: Option<GuiNextSchedule>,
}

#[derive(Debug, Serialize)]
struct GuiPaths {
    hosts: String,
    config: String,
    log: String,
}

#[derive(Debug, Serialize)]
struct GuiStatus {
    tier1_domains: usize,
    tier2_sites: usize,
    open_limit_per_hour: usize,
    opens_used_this_hour: usize,
    opens_remaining_this_hour: usize,
    reset_seconds: Option<i64>,
    active_schedules: Vec<String>,
    tier2_blocking_enabled: bool,
    system_healthy: bool,
}

#[derive(Debug, Serialize)]
struct GuiOpening {
    site: String,
    remaining_seconds: i64,
    expires_at: DateTime<Utc>,
    url: Option<String>,
    reason: Option<String>,
    minutes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct GuiAllowance {
    site: String,
    used_minutes: u64,
    daily_minutes: u64,
    remaining_minutes: u64,
    max_session_minutes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct GuiActivity {
    ts: DateTime<Utc>,
    action: String,
    site: Option<String>,
    url: Option<String>,
    reason: Option<String>,
    minutes: Option<u64>,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct GuiTodaySummary {
    opens: usize,
    opened_minutes: u64,
    denied: usize,
    restores: usize,
    repairs: usize,
    hourly_activity: Vec<usize>,
}

#[derive(Debug, Serialize)]
struct GuiRankedMetric {
    label: String,
    value: u64,
    count: usize,
}

#[derive(Debug, Serialize)]
struct GuiNextSchedule {
    name: String,
    starts_at: DateTime<Local>,
    seconds_until: i64,
    mode: Option<String>,
}

fn run_gui_server(config_path: &Path, bind: &str) -> Result<()> {
    let listener =
        TcpListener::bind(bind).with_context(|| format!("binding GUI server to {bind}"))?;
    println!("focus-hosts GUI listening at http://{bind}");
    println!("Press Ctrl+C to stop the GUI server.");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(err) = handle_gui_request(&mut stream, config_path) {
                    let body = serde_json::json!({ "ok": false, "error": err.to_string() });
                    let _ = write_http_response(
                        &mut stream,
                        "500 Internal Server Error",
                        "application/json",
                        &body.to_string(),
                    );
                }
            }
            Err(err) => eprintln!("Warning: GUI connection failed: {err}"),
        }
    }

    Ok(())
}

pub fn gui_dashboard_json_for_config(explicit_config: Option<&Path>) -> Result<String> {
    let config_path = find_config_path(explicit_config)?;
    let cfg = load_config(&config_path)?;
    let dashboard = build_gui_dashboard(&cfg, &config_path)?;
    serde_json::to_string(&dashboard).context("serializing dashboard state")
}

pub fn gui_rebuild_for_config(explicit_config: Option<&Path>) -> Result<()> {
    let config_path = find_config_path(explicit_config)?;
    let cfg = load_config(&config_path)?;
    rebuild_hosts(&cfg, None)
}

pub fn gui_close_current_for_config(explicit_config: Option<&Path>) -> Result<String> {
    let config_path = find_config_path(explicit_config)?;
    let cfg = load_config(&config_path)?;
    if let Some(state) = read_runtime_state(&cfg.settings.state_path)? {
        let site = state.site.clone();
        restore_site(&cfg, state.site)?;
        Ok(format!("closed active opening for {site}"))
    } else {
        Ok("no active opening".to_string())
    }
}

fn handle_gui_request(stream: &mut TcpStream, config_path: &Path) -> Result<()> {
    let request = read_http_request(stream)?;

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            write_http_response(stream, "200 OK", "text/html; charset=utf-8", GUI_HTML)
        }
        ("GET", "/style.css") => {
            write_http_response(stream, "200 OK", "text/css; charset=utf-8", GUI_CSS)
        }
        ("GET", "/app.js") => write_http_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            GUI_JS,
        ),
        ("GET", "/api/dashboard") => {
            let cfg = load_config(config_path)?;
            let dashboard = build_gui_dashboard(&cfg, config_path)?;
            let body = serde_json::to_string(&dashboard)?;
            write_http_response(stream, "200 OK", "application/json", &body)
        }
        ("POST", "/api/rebuild") => {
            let cfg = load_config(config_path)?;
            rebuild_hosts(&cfg, None)?;
            write_json_ok(stream, "hosts rebuilt")
        }
        ("POST", "/api/close-current") => {
            let cfg = load_config(config_path)?;
            if let Some(state) = read_runtime_state(&cfg.settings.state_path)? {
                restore_site(&cfg, state.site)?;
                write_json_ok(stream, "current opening closed")
            } else {
                write_json_ok(stream, "no active opening")
            }
        }
        _ => write_http_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            "not found",
        ),
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut buffer = [0u8; 8192];
    let read = stream.read(&mut buffer)?;
    if read == 0 {
        bail!("empty request");
    }
    let raw = String::from_utf8_lossy(&buffer[..read]);
    let request_line = raw
        .lines()
        .next()
        .ok_or_else(|| anyhow!("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow!("missing request method"))?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| anyhow!("missing request path"))?
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();

    Ok(HttpRequest { method, path })
}

fn write_json_ok(stream: &mut TcpStream, message: &str) -> Result<()> {
    let body = serde_json::json!({ "ok": true, "message": message }).to_string();
    write_http_response(stream, "200 OK", "application/json", &body)
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn build_gui_dashboard(cfg: &Config, config_path: &Path) -> Result<GuiDashboard> {
    let now = Utc::now();
    let logs = read_logs(&cfg.settings.log_path)?;
    let hour_since = now - Duration::hours(1);
    let recent_allows = logs
        .iter()
        .filter(|entry| entry.action == "allow" && entry.ts >= hour_since)
        .collect::<Vec<_>>();
    let opens_used = recent_allows.len();
    let reset_seconds = recent_allows
        .iter()
        .map(|entry| (entry.ts + Duration::hours(1) - now).num_seconds())
        .filter(|seconds| *seconds > 0)
        .min();
    let runtime_state = read_runtime_state(&cfg.settings.state_path)?;
    let current_opening = runtime_state
        .filter(|state| state.expires_at > now)
        .map(|state| {
            let latest = latest_allow_for_site(&logs, &state.site);
            GuiOpening {
                site: state.site,
                remaining_seconds: (state.expires_at - now).num_seconds().max(0),
                expires_at: state.expires_at,
                url: latest.and_then(|entry| entry.url.clone()),
                reason: latest.and_then(|entry| entry.reason.clone()),
                minutes: latest.and_then(|entry| entry.minutes),
            }
        });
    let active_schedule_names = active_schedules(cfg, Local::now())
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let active_schedule_name_refs = active_schedule_names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let tier2_blocking_enabled =
        tier2_enabled_for_schedules(&active_schedule_refs(cfg, &active_schedule_name_refs));

    Ok(GuiDashboard {
        generated_at: now,
        paths: GuiPaths {
            hosts: cfg.settings.hosts_path.display().to_string(),
            config: config_path.display().to_string(),
            log: cfg.settings.log_path.display().to_string(),
        },
        status: GuiStatus {
            tier1_domains: cfg.tier1.len(),
            tier2_sites: cfg.tier2.len(),
            open_limit_per_hour: cfg.settings.open_limit_per_hour,
            opens_used_this_hour: opens_used,
            opens_remaining_this_hour: cfg.settings.open_limit_per_hour.saturating_sub(opens_used),
            reset_seconds,
            active_schedules: active_schedule_names,
            tier2_blocking_enabled,
            system_healthy: true,
        },
        current_opening,
        allowances: build_gui_allowances(cfg, &logs, now),
        recent_activity: logs
            .iter()
            .rev()
            .take(8)
            .map(|entry| GuiActivity {
                ts: entry.ts,
                action: entry.action.clone(),
                site: entry.site.clone(),
                url: entry.url.clone(),
                reason: entry.reason.clone(),
                minutes: entry.minutes,
                detail: entry.detail.clone(),
            })
            .collect(),
        today_summary: build_gui_today_summary(&logs, now),
        top_sites_week: ranked_sites_since(&logs, now - Duration::days(7)),
        common_reasons_week: ranked_reasons_since(&logs, now - Duration::days(7)),
        next_schedule: next_schedule_preview(cfg, Local::now()),
    })
}

fn latest_allow_for_site<'a>(logs: &'a [LogEntry], site: &str) -> Option<&'a LogEntry> {
    logs.iter()
        .rev()
        .find(|entry| entry.action == "allow" && entry.site.as_deref() == Some(site))
}

fn build_gui_allowances(cfg: &Config, logs: &[LogEntry], now: DateTime<Utc>) -> Vec<GuiAllowance> {
    let since = start_of_today_local(now.with_timezone(&Local));
    cfg.allowances
        .iter()
        .map(|(site, allowance)| {
            let used_minutes = logs
                .iter()
                .filter(|entry| {
                    entry.action == "allow"
                        && entry.ts >= since
                        && entry.site.as_deref() == Some(site.as_str())
                })
                .map(|entry| entry.minutes.unwrap_or(0))
                .sum::<u64>();
            GuiAllowance {
                site: site.clone(),
                used_minutes,
                daily_minutes: allowance.daily_minutes,
                remaining_minutes: allowance.daily_minutes.saturating_sub(used_minutes),
                max_session_minutes: allowance.max_session_minutes,
            }
        })
        .collect()
}

fn build_gui_today_summary(logs: &[LogEntry], now: DateTime<Utc>) -> GuiTodaySummary {
    let since = start_of_today_local(now.with_timezone(&Local));
    let mut summary = GuiTodaySummary {
        opens: 0,
        opened_minutes: 0,
        denied: 0,
        restores: 0,
        repairs: 0,
        hourly_activity: vec![0; 24],
    };

    for entry in logs.iter().filter(|entry| entry.ts >= since) {
        let hour = entry.ts.with_timezone(&Local).hour() as usize;
        if let Some(slot) = summary.hourly_activity.get_mut(hour) {
            *slot += 1;
        }

        match entry.action.as_str() {
            "allow" => {
                summary.opens += 1;
                summary.opened_minutes += entry.minutes.unwrap_or(0);
            }
            "deny" => summary.denied += 1,
            "restore" => summary.restores += 1,
            "watchdog-repair" => summary.repairs += 1,
            _ => {}
        }
    }

    summary
}

fn ranked_sites_since(logs: &[LogEntry], since: DateTime<Utc>) -> Vec<GuiRankedMetric> {
    let mut values: BTreeMap<String, (u64, usize)> = BTreeMap::new();
    for entry in logs
        .iter()
        .filter(|entry| entry.action == "allow" && entry.ts >= since)
    {
        let label = entry.site.clone().unwrap_or_else(|| "-".to_string());
        let metric = values.entry(label).or_default();
        metric.0 += entry.minutes.unwrap_or(0);
        metric.1 += 1;
    }

    let mut ranked = values
        .into_iter()
        .map(|(label, (value, count))| GuiRankedMetric {
            label,
            value,
            count,
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    ranked.truncate(5);
    ranked
}

fn ranked_reasons_since(logs: &[LogEntry], since: DateTime<Utc>) -> Vec<GuiRankedMetric> {
    let mut values: BTreeMap<String, usize> = BTreeMap::new();
    for entry in logs
        .iter()
        .filter(|entry| entry.action == "allow" && entry.ts >= since)
    {
        if let Some(reason) = &entry.reason {
            *values.entry(reason.clone()).or_default() += 1;
        }
    }

    let mut ranked = values
        .into_iter()
        .map(|(label, count)| GuiRankedMetric {
            label,
            value: count as u64,
            count,
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    ranked.truncate(5);
    ranked
}

fn next_schedule_preview(cfg: &Config, now: DateTime<Local>) -> Option<GuiNextSchedule> {
    let mut best: Option<GuiNextSchedule> = None;

    for (name, schedule) in &cfg.schedules {
        let Ok(start) = parse_schedule_time(&schedule.start) else {
            continue;
        };

        for offset_days in 0..=7 {
            let date = (now + Duration::days(offset_days)).date_naive();
            if !schedule_includes_weekday(schedule, date.weekday()) {
                continue;
            }
            let Some(starts_at) = local_datetime(date, start) else {
                continue;
            };
            if starts_at <= now {
                continue;
            }
            let seconds_until = (starts_at - now).num_seconds();
            let candidate = GuiNextSchedule {
                name: name.clone(),
                starts_at,
                seconds_until,
                mode: schedule.mode.clone(),
            };
            if best
                .as_ref()
                .is_none_or(|existing| candidate.starts_at < existing.starts_at)
            {
                best = Some(candidate);
            }
        }
    }

    best
}

fn local_datetime(date: chrono::NaiveDate, time: NaiveTime) -> Option<DateTime<Local>> {
    match Local.from_local_datetime(&date.and_time(time)) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(earliest, _) => Some(earliest),
        LocalResult::None => None,
    }
}

const GUI_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Focus Control</title>
  <link rel="stylesheet" href="/style.css">
</head>
<body>
  <div class="app-shell">
    <header class="titlebar">
      <div class="brand">
        <div class="brand-mark">FH</div>
        <div>
          <strong>Focus Control</strong>
          <span>focus-hosts</span>
        </div>
      </div>
      <div class="window-actions">
        <span></span><span></span><span></span>
      </div>
    </header>

    <aside class="sidebar">
      <nav>
        <button class="nav-item active" data-view="dashboard"><span>DB</span>Dashboard</button>
        <button class="nav-item" data-view="open"><span>OP</span>Open Temporarily</button>
        <button class="nav-item" data-view="tiers"><span>TR</span>Tiers</button>
        <button class="nav-item" data-view="schedules"><span>SC</span>Schedules</button>
        <button class="nav-item" data-view="apps"><span>AP</span>Apps</button>
        <button class="nav-item" data-view="logs"><span>LA</span>Logs & Analytics</button>
        <button class="nav-item" data-view="settings"><span>ST</span>Settings</button>
        <button class="nav-item" data-view="locks"><span>LK</span>Locks</button>
      </nav>

      <section class="next-card">
        <p>Next schedule</p>
        <strong id="nextScheduleName">None</strong>
        <span id="nextScheduleStarts">No upcoming schedule</span>
      </section>
      <footer>focus-hosts v0.1.0</footer>
    </aside>

    <main>
      <section class="top-row">
        <div>
          <h1 id="viewTitle">Dashboard</h1>
          <p id="viewSubtitle">Overview of your focus environment</p>
        </div>
        <div class="system-pill">
          <span class="ok-dot"></span>
          <div><strong id="healthText">System healthy</strong><span id="scheduleText">Loading</span></div>
        </div>
        <button class="quick-button" id="refreshButton">Refresh</button>
      </section>

      <section class="view view-dashboard active">
        <div class="grid cards-top">
          <article class="card status-card">
            <h2>Current status</h2>
            <div class="status-body">
              <div class="shield">OK</div>
              <div>
                <strong id="blockStatus">Loading</strong>
                <p id="blockDetail">Reading focus-hosts state</p>
                <p id="openingDetail">Please wait</p>
              </div>
            </div>
            <button class="full-button" id="rebuildButton">Rebuild hosts now</button>
          </article>

          <article class="card">
            <h2>Opens this hour</h2>
            <div class="big-number"><span id="opensUsed">0</span> / <span id="opensLimit">0</span></div>
            <p>Remaining opens</p>
            <div class="meter"><span id="opensMeter"></span></div>
            <p class="muted" id="resetsIn">Reset time unknown</p>
          </article>

          <article class="card allowance-card">
            <h2>Today's allowance</h2>
            <div id="allowanceList" class="stack-list"></div>
            <button class="link-button" data-view-link="logs">View all allowances</button>
          </article>

          <article class="card watchdog-card">
            <h2>Watchdog</h2>
            <div class="watchdog-body">
              <div class="shield small">OK</div>
              <div><strong>Active & healthy</strong><p>No issues detected</p></div>
            </div>
            <button class="full-button" data-view-link="logs">View recent repairs</button>
          </article>
        </div>

        <div class="grid middle-grid">
          <article class="card opening-card">
            <h2>Current opening</h2>
            <div id="currentOpening"></div>
          </article>

          <article class="card activity-card">
            <h2>Recent activity</h2>
            <div id="activityList" class="activity-list"></div>
            <button class="link-button" data-view-link="logs">View all logs</button>
          </article>
        </div>

        <div class="grid lower-grid">
          <article class="card summary-card">
            <div class="card-heading">
              <h2>Today's summary</h2>
              <button class="link-button" data-view-link="logs">View full summary</button>
            </div>
            <div class="summary-stats">
              <div><span>Opens</span><strong id="summaryOpens">0</strong><small id="summaryMinutes">0m total</small></div>
              <div><span>Denied</span><strong id="summaryDenied">0</strong><small>Sites blocked</small></div>
              <div><span>Restores</span><strong id="summaryRestores">0</strong><small>All blocks</small></div>
              <div><span>Repairs</span><strong id="summaryRepairs">0</strong><small>By watchdog</small></div>
            </div>
            <div id="heatmap" class="heatmap"></div>
            <p class="muted">Activity by hour (local time)</p>
          </article>

          <article class="card">
            <h2>Top opened sites (week)</h2>
            <div id="topSites" class="rank-list"></div>
            <button class="link-button" data-view-link="logs">View analytics</button>
          </article>

          <article class="card">
            <h2>Common reasons (week)</h2>
            <div id="commonReasons" class="rank-list"></div>
            <button class="link-button" data-view-link="logs">View all reasons</button>
          </article>
        </div>
      </section>

      <section class="view view-placeholder">
        <article class="card placeholder-card">
          <h2 id="placeholderTitle">Coming next</h2>
          <p id="placeholderText">This first GUI pass focuses on the dashboard and safe actions.</p>
        </article>
      </section>

      <footer class="status-strip">
        <span id="hostsPath">Hosts file: -</span>
        <span id="configPath">Config: -</span>
        <span id="logPath">Log: -</span>
        <button id="exportButton">Export diagnostics</button>
      </footer>
    </main>
  </div>
  <script src="/app.js"></script>
</body>
</html>
"#;

const GUI_CSS: &str = r#"* {
  box-sizing: border-box;
}

:root {
  --bg: #071019;
  --panel: #111d28;
  --panel-2: #142230;
  --line: #2a3948;
  --text: #eef5fb;
  --muted: #a9b6c4;
  --green: #4fd278;
  --orange: #ff6422;
  --yellow: #ffc72f;
  --red: #ff4058;
  --purple: #a775ff;
}

body {
  margin: 0;
  min-height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

button {
  color: inherit;
  font: inherit;
}

.app-shell {
  min-height: 100vh;
  display: grid;
  grid-template-columns: 280px 1fr;
  grid-template-rows: 72px 1fr;
  background:
    radial-gradient(circle at 10% 0%, rgba(79, 210, 120, 0.08), transparent 24rem),
    linear-gradient(135deg, #071019 0%, #0a1520 50%, #101924 100%);
}

.titlebar {
  grid-column: 1 / -1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border: 1px solid var(--line);
  border-bottom: 0;
  padding: 0 28px;
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
}

.brand strong,
.brand span {
  display: block;
}

.brand span {
  color: var(--green);
}

.brand-mark {
  width: 36px;
  height: 36px;
  border: 2px solid #b9d7e9;
  border-radius: 12px;
  display: grid;
  place-items: center;
  color: #b9d7e9;
  font-size: 12px;
  font-weight: 800;
}

.window-actions {
  display: flex;
  gap: 18px;
}

.window-actions span {
  width: 14px;
  height: 14px;
  border: 1px solid #8c9aaa;
  border-radius: 4px;
  opacity: 0.8;
}

.sidebar {
  grid-row: 2;
  border-left: 1px solid var(--line);
  border-right: 1px solid var(--line);
  padding: 24px 14px;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

nav {
  display: grid;
  gap: 10px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 14px;
  width: 100%;
  min-height: 48px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: #c8d2de;
  text-align: left;
  padding: 0 16px;
  cursor: pointer;
}

.nav-item span {
  width: 24px;
  color: #9eadbc;
  font-size: 12px;
  font-weight: 800;
}

.nav-item.active {
  background: rgba(255, 255, 255, 0.07);
  color: var(--green);
  box-shadow: inset 3px 0 0 var(--green);
}

.next-card {
  margin-top: auto;
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.03);
  border-radius: 10px;
  padding: 18px;
}

.next-card p,
.next-card span,
.sidebar footer,
.muted,
.card p,
.card small {
  color: var(--muted);
}

.next-card strong {
  display: block;
  color: #bd99ff;
  font-size: 20px;
  margin: 8px 0;
}

main {
  grid-row: 2;
  min-width: 0;
  padding: 22px 30px 0;
  border-right: 1px solid var(--line);
}

.top-row {
  display: grid;
  grid-template-columns: 1fr auto auto;
  align-items: center;
  gap: 18px;
  margin-bottom: 22px;
}

h1,
h2,
p {
  margin: 0;
}

h1 {
  font-size: 24px;
}

h2 {
  font-size: 16px;
  margin-bottom: 16px;
}

.system-pill {
  display: flex;
  align-items: center;
  gap: 10px;
}

.system-pill span {
  display: block;
  color: var(--muted);
}

.ok-dot {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  border: 2px solid var(--green);
  box-shadow: 0 0 24px rgba(79, 210, 120, 0.26);
}

.quick-button,
.full-button,
#exportButton {
  min-height: 42px;
  border: 1px solid #364758;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.05);
  padding: 0 18px;
  cursor: pointer;
}

.quick-button:hover,
.full-button:hover,
#exportButton:hover {
  border-color: var(--green);
}

.view {
  display: none;
}

.view.active {
  display: block;
}

.grid {
  display: grid;
  gap: 14px;
}

.cards-top {
  grid-template-columns: 1.35fr 0.95fr 1.05fr 1.05fr;
}

.middle-grid {
  grid-template-columns: 1.15fr 1.5fr;
  margin-top: 14px;
}

.lower-grid {
  grid-template-columns: 1.55fr 1fr 1fr;
  margin-top: 14px;
}

.card {
  border: 1px solid var(--line);
  background: linear-gradient(145deg, rgba(255, 255, 255, 0.06), rgba(255, 255, 255, 0.025));
  border-radius: 8px;
  padding: 20px;
  min-width: 0;
}

.status-body,
.watchdog-body,
.opening-main,
.activity-row,
.rank-row,
.allowance-row,
.card-heading {
  display: flex;
  align-items: center;
  gap: 16px;
}

.status-body {
  margin-bottom: 22px;
}

.shield {
  width: 72px;
  height: 72px;
  border-radius: 26px 26px 34px 34px;
  display: grid;
  place-items: center;
  background: rgba(79, 210, 120, 0.22);
  border: 5px solid rgba(79, 210, 120, 0.7);
  color: white;
  font-weight: 900;
  box-shadow: 0 12px 24px rgba(79, 210, 120, 0.16);
}

.shield.small {
  width: 58px;
  height: 58px;
  font-size: 12px;
}

#blockStatus {
  color: var(--green);
  font-size: 22px;
}

.big-number {
  font-size: 36px;
  font-weight: 800;
  margin: 22px 0 4px;
}

.meter,
.bar {
  height: 14px;
  border-radius: 999px;
  background: rgba(170, 185, 200, 0.15);
  overflow: hidden;
}

.meter span,
.bar span {
  display: block;
  height: 100%;
  width: 0;
  border-radius: inherit;
  background: var(--green);
}

.stack-list {
  display: grid;
  gap: 12px;
}

.allowance-row,
.rank-row {
  justify-content: space-between;
}

.allowance-label,
.rank-label {
  min-width: 0;
}

.allowance-label strong,
.rank-label strong {
  display: block;
}

.allowance-row .bar,
.rank-row .bar {
  margin-top: 6px;
  width: 100%;
  min-width: 160px;
  height: 8px;
}

.link-button {
  border: 0;
  background: transparent;
  color: var(--green);
  cursor: pointer;
  margin-top: 14px;
  padding: 0;
}

.opening-card {
  min-height: 246px;
}

.opening-main {
  justify-content: space-between;
}

.site-badge {
  width: 64px;
  height: 64px;
  border-radius: 14px;
  display: grid;
  place-items: center;
  background: var(--orange);
  color: white;
  font-weight: 900;
  flex: 0 0 auto;
}

.opening-title {
  display: flex;
  align-items: center;
  gap: 16px;
}

.countdown {
  color: var(--orange);
  font-size: 44px;
  font-weight: 900;
  text-align: right;
}

.opening-progress {
  margin: 22px 0 14px;
}

.opening-progress span {
  background: var(--orange);
}

.opening-actions {
  display: flex;
  gap: 14px;
  margin-top: 22px;
}

.activity-list {
  display: grid;
}

.activity-row {
  justify-content: space-between;
  padding: 9px 0;
  border-bottom: 1px solid rgba(255, 255, 255, 0.07);
}

.activity-row:last-child {
  border-bottom: 0;
}

.activity-icon {
  width: 26px;
  height: 26px;
  border-radius: 999px;
  display: grid;
  place-items: center;
  font-size: 11px;
  font-weight: 800;
  flex: 0 0 auto;
}

.activity-icon.allow {
  background: rgba(79, 210, 120, 0.18);
  color: var(--green);
}

.activity-icon.restore,
.activity-icon.watchdog-repair {
  background: rgba(76, 164, 255, 0.18);
  color: #59aaff;
}

.activity-icon.deny {
  background: rgba(255, 64, 88, 0.16);
  color: var(--red);
}

.activity-row strong,
.activity-row span {
  display: block;
}

.activity-row span,
.rank-row span {
  color: var(--muted);
}

.summary-stats {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
}

.summary-stats div {
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 12px;
  text-align: center;
}

.summary-stats span,
.summary-stats small {
  display: block;
}

.summary-stats strong {
  display: block;
  font-size: 28px;
  margin: 2px 0;
}

.heatmap {
  margin-top: 16px;
  display: grid;
  grid-template-columns: repeat(24, 1fr);
  gap: 6px;
}

.heat-cell {
  height: 13px;
  border-radius: 4px;
  background: rgba(170, 185, 200, 0.16);
}

.heat-cell.level-1 {
  background: rgba(79, 210, 120, 0.35);
}

.heat-cell.level-2 {
  background: rgba(79, 210, 120, 0.62);
}

.heat-cell.level-3 {
  background: var(--green);
}

.rank-list {
  display: grid;
  gap: 14px;
}

.rank-number {
  width: 20px;
  color: var(--muted);
}

.rank-label {
  flex: 1;
}

.status-strip {
  min-height: 64px;
  margin: 16px -30px 0;
  padding: 0 30px;
  border-top: 1px solid var(--line);
  display: grid;
  grid-template-columns: 1fr 1fr 1fr auto;
  align-items: center;
  gap: 16px;
  color: var(--muted);
}

.placeholder-card {
  max-width: 680px;
}

.empty {
  color: var(--muted);
  padding: 20px 0;
}

@media (max-width: 1200px) {
  .app-shell {
    grid-template-columns: 86px 1fr;
  }

  .brand div:last-child,
  .nav-item:not(.active),
  .nav-item.active {
    font-size: 0;
  }

  .nav-item span {
    font-size: 12px;
  }

  .next-card,
  .sidebar footer {
    display: none;
  }

  .cards-top,
  .middle-grid,
  .lower-grid {
    grid-template-columns: 1fr 1fr;
  }
}

@media (max-width: 820px) {
  .app-shell {
    display: block;
  }

  .titlebar,
  .sidebar {
    position: static;
  }

  .sidebar {
    border: 0;
    padding: 10px;
  }

  nav {
    grid-template-columns: repeat(4, 1fr);
  }

  .top-row,
  .cards-top,
  .middle-grid,
  .lower-grid,
  .status-strip {
    grid-template-columns: 1fr;
  }

  main {
    padding: 18px;
  }

  .status-strip {
    margin: 16px -18px 0;
    padding: 14px 18px;
  }
}
"#;

const GUI_JS: &str = r##"let dashboard = null;

const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => Array.from(document.querySelectorAll(selector));

function fmtDuration(seconds) {
  if (seconds == null) return "unknown";
  const safe = Math.max(0, Math.floor(seconds));
  const h = Math.floor(safe / 3600);
  const m = Math.floor((safe % 3600) / 60);
  const s = safe % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function fmtClock(value) {
  if (!value) return "-";
  return new Date(value).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function pct(used, total) {
  if (!total) return 0;
  return Math.max(0, Math.min(100, (used / total) * 100));
}

function colorForIndex(index) {
  return ["#ff6422", "#ffc72f", "#ff4058", "#a775ff", "#4fd278"][index % 5];
}

async function loadDashboard() {
  const response = await fetch("/api/dashboard", { cache: "no-store" });
  if (!response.ok) throw new Error(await response.text());
  dashboard = await response.json();
  renderDashboard();
}

async function postAction(path) {
  const response = await fetch(path, { method: "POST" });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok || payload.ok === false) {
    throw new Error(payload.error || payload.message || "Action failed");
  }
  await loadDashboard();
}

function renderDashboard() {
  const status = dashboard.status;
  const opening = dashboard.current_opening;
  $("#healthText").textContent = status.system_healthy ? "System healthy" : "Needs attention";
  $("#scheduleText").textContent = status.active_schedules.length
    ? `Active: ${status.active_schedules.join(", ")}`
    : "No active schedule";

  $("#blockStatus").textContent = opening ? "Temporarily open" : "Fully blocked";
  $("#blockDetail").textContent = status.tier2_blocking_enabled
    ? "Tier 1 and Tier 2 are blocked"
    : "Tier 2 blocking disabled by schedule";
  $("#openingDetail").textContent = opening
    ? `${opening.site} closes in ${fmtDuration(opening.remaining_seconds)}`
    : "No temporary openings";

  $("#opensUsed").textContent = status.opens_used_this_hour;
  $("#opensLimit").textContent = status.open_limit_per_hour;
  $("#opensMeter").style.width = `${pct(status.opens_used_this_hour, status.open_limit_per_hour)}%`;
  $("#resetsIn").textContent = status.reset_seconds
    ? `Resets in ${fmtDuration(status.reset_seconds)}`
    : "No opens waiting to reset";

  $("#hostsPath").textContent = `Hosts file: ${dashboard.paths.hosts}`;
  $("#configPath").textContent = `Config: ${dashboard.paths.config}`;
  $("#logPath").textContent = `Log: ${dashboard.paths.log}`;

  renderNextSchedule();
  renderAllowances();
  renderOpening();
  renderActivity();
  renderSummary();
  renderRankList("#topSites", dashboard.top_sites_week, "m");
  renderRankList("#commonReasons", dashboard.common_reasons_week, "");
}

function renderNextSchedule() {
  const next = dashboard.next_schedule;
  $("#nextScheduleName").textContent = next ? next.name : "None";
  $("#nextScheduleStarts").textContent = next
    ? `starts in ${fmtDuration(next.seconds_until)}`
    : "No upcoming schedule";
}

function renderAllowances() {
  const list = $("#allowanceList");
  list.innerHTML = "";
  if (!dashboard.allowances.length) {
    list.innerHTML = `<p class="empty">No allowances configured</p>`;
    return;
  }

  dashboard.allowances.slice(0, 4).forEach((item, index) => {
    const row = document.createElement("div");
    row.className = "allowance-item";
    const percent = pct(item.used_minutes, item.daily_minutes);
    row.innerHTML = `
      <div class="allowance-row">
        <div class="allowance-label"><strong>${item.site}</strong></div>
        <span>${item.used_minutes}m / ${item.daily_minutes}m</span>
      </div>
      <div class="bar"><span style="width:${percent}%; background:${colorForIndex(index)}"></span></div>
    `;
    list.appendChild(row);
  });
}

function renderOpening() {
  const root = $("#currentOpening");
  const opening = dashboard.current_opening;
  if (!opening) {
    root.innerHTML = `
      <p class="empty">No Tier 2 site is open right now.</p>
      <div class="opening-actions">
        <button class="full-button" id="refreshOpening">Refresh status</button>
      </div>
    `;
    $("#refreshOpening").addEventListener("click", loadDashboard);
    return;
  }

  const total = opening.minutes ? opening.minutes * 60 : opening.remaining_seconds;
  const used = Math.max(0, total - opening.remaining_seconds);
  root.innerHTML = `
    <div class="opening-main">
      <div class="opening-title">
        <div class="site-badge">${opening.site.slice(0, 2).toUpperCase()}</div>
        <div>
          <h3>${opening.site}</h3>
          <p>Opened for ${opening.reason || "temporary access"}</p>
        </div>
      </div>
      <div>
        <div class="countdown">${fmtDuration(opening.remaining_seconds)}</div>
        <p class="muted">remaining</p>
      </div>
    </div>
    <div class="bar opening-progress"><span style="width:${pct(used, total)}%"></span></div>
    <p class="muted">Will be restored at ${fmtClock(opening.expires_at)}</p>
    <div class="opening-actions">
      <button class="full-button" id="closeOpening">Close now</button>
      <button class="full-button" disabled>Extend...</button>
      <button class="full-button" ${opening.url ? "" : "disabled"} id="viewSession">View session</button>
    </div>
  `;
  $("#closeOpening").addEventListener("click", () => postAction("/api/close-current").catch(alert));
  const view = $("#viewSession");
  if (view && opening.url) view.addEventListener("click", () => window.open(opening.url, "_blank"));
}

function renderActivity() {
  const list = $("#activityList");
  list.innerHTML = "";
  if (!dashboard.recent_activity.length) {
    list.innerHTML = `<p class="empty">No activity logged yet</p>`;
    return;
  }

  dashboard.recent_activity.forEach((entry) => {
    const row = document.createElement("div");
    row.className = "activity-row";
    const action = entry.action;
    const label = entry.site || entry.url || "system";
    row.innerHTML = `
      <div class="activity-row">
        <div class="activity-icon ${action}">${action.slice(0, 2).toUpperCase()}</div>
        <div>
          <strong>${titleCase(action)} ${label}</strong>
          <span>${fmtClock(entry.ts)}</span>
        </div>
      </div>
      <div>
        <strong>${entry.minutes ? `${entry.minutes} min` : ""}</strong>
        <span>${entry.detail || entry.reason || ""}</span>
      </div>
    `;
    list.appendChild(row);
  });
}

function renderSummary() {
  const summary = dashboard.today_summary;
  $("#summaryOpens").textContent = summary.opens;
  $("#summaryMinutes").textContent = `${summary.opened_minutes}m total`;
  $("#summaryDenied").textContent = summary.denied;
  $("#summaryRestores").textContent = summary.restores;
  $("#summaryRepairs").textContent = summary.repairs;

  const heatmap = $("#heatmap");
  heatmap.innerHTML = "";
  const max = Math.max(1, ...summary.hourly_activity);
  summary.hourly_activity.forEach((count, hour) => {
    const cell = document.createElement("div");
    const level = count === 0 ? 0 : Math.max(1, Math.ceil((count / max) * 3));
    cell.className = `heat-cell level-${level}`;
    cell.title = `${hour}:00 - ${count} event(s)`;
    heatmap.appendChild(cell);
  });
}

function renderRankList(selector, rows, suffix) {
  const root = $(selector);
  root.innerHTML = "";
  if (!rows.length) {
    root.innerHTML = `<p class="empty">No data yet</p>`;
    return;
  }
  const max = Math.max(1, ...rows.map((row) => row.value));
  rows.forEach((row, index) => {
    const el = document.createElement("div");
    el.className = "rank-row";
    const value = suffix ? `${row.value}${suffix}` : `${row.value}`;
    el.innerHTML = `
      <span class="rank-number">${index + 1}</span>
      <div class="rank-label">
        <strong>${row.label}</strong>
        <div class="bar"><span style="width:${pct(row.value, max)}%; background:${colorForIndex(index)}"></span></div>
      </div>
      <span>${value}</span>
    `;
    root.appendChild(el);
  });
}

function titleCase(value) {
  return value.replace(/-/g, " ").replace(/\b\w/g, (ch) => ch.toUpperCase());
}

function showView(view) {
  $$(".nav-item").forEach((button) => button.classList.toggle("active", button.dataset.view === view));
  $(".view-dashboard").classList.toggle("active", view === "dashboard");
  $(".view-placeholder").classList.toggle("active", view !== "dashboard");
  const labels = {
    open: ["Open Temporarily", "Temporary Tier 2 windows will be added here."],
    tiers: ["Tiers", "Domain editing will be added here."],
    schedules: ["Schedules", "Recurring block policy will be editable here."],
    apps: ["Apps", "Application blocking is still a future feature."],
    logs: ["Logs & Analytics", "Detailed log tables will be added here."],
    settings: ["Settings", "Configuration editing will be added here."],
    locks: ["Locks", "Lock modes will be added here."]
  };
  const label = labels[view] || ["Dashboard", "Overview of your focus environment"];
  $("#viewTitle").textContent = label[0];
  $("#viewSubtitle").textContent = label[1];
  $("#placeholderTitle").textContent = label[0];
  $("#placeholderText").textContent = label[1];
}

$$(".nav-item").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.view));
});

$$("[data-view-link]").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.viewLink));
});

$("#refreshButton").addEventListener("click", () => loadDashboard().catch(alert));
$("#rebuildButton").addEventListener("click", () => postAction("/api/rebuild").catch(alert));
$("#exportButton").addEventListener("click", () => {
  const blob = new Blob([JSON.stringify(dashboard, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "focus-hosts-diagnostics.json";
  link.click();
  URL.revokeObjectURL(url);
});

loadDashboard().catch((err) => {
  $("#blockStatus").textContent = "Unable to load";
  $("#blockDetail").textContent = err.message;
});

setInterval(() => loadDashboard().catch(() => {}), 10000);
"##;

fn allowance_status(
    cfg: &Config,
    site_name: &str,
    now: DateTime<Utc>,
) -> Result<Option<AllowanceStatus>> {
    let Some(allowance) = cfg.allowances.get(site_name) else {
        return Ok(None);
    };
    let since = start_of_today_local(now.with_timezone(&Local));
    let used_minutes = used_allowance_minutes(&cfg.settings.log_path, site_name, since)?;
    let remaining_minutes = allowance.daily_minutes.saturating_sub(used_minutes);

    Ok(Some(AllowanceStatus {
        daily_minutes: allowance.daily_minutes,
        used_minutes,
        remaining_minutes,
    }))
}

fn cap_minutes_by_allowance(
    cfg: &Config,
    site_name: &str,
    requested_minutes: u64,
    now: DateTime<Utc>,
) -> Result<u64> {
    let Some(allowance) = cfg.allowances.get(site_name) else {
        return Ok(requested_minutes);
    };
    let since = start_of_today_local(now.with_timezone(&Local));
    let used_minutes = used_allowance_minutes(&cfg.settings.log_path, site_name, since)?;
    let remaining = allowance.daily_minutes.saturating_sub(used_minutes);
    let session_cap = allowance.max_session_minutes.unwrap_or(requested_minutes);

    Ok(requested_minutes.min(session_cap).min(remaining))
}

fn used_allowance_minutes(path: &Path, site_name: &str, since: DateTime<Utc>) -> Result<u64> {
    let entries = read_logs(path)?;
    Ok(entries
        .into_iter()
        .filter(|entry| {
            entry.action == "allow" && entry.ts >= since && entry.site.as_deref() == Some(site_name)
        })
        .map(|entry| entry.minutes.unwrap_or(0))
        .sum())
}

fn start_of_today_local(now: DateTime<Local>) -> DateTime<Utc> {
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    match Local.from_local_datetime(&midnight) {
        LocalResult::Single(dt) => dt.with_timezone(&Utc),
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        LocalResult::None => {
            let elapsed = Duration::hours(now.hour() as i64)
                + Duration::minutes(now.minute() as i64)
                + Duration::seconds(now.second() as i64);
            (now - elapsed).with_timezone(&Utc)
        }
    }
}

fn schedule_status(cfg: &Config) -> Result<()> {
    print_schedule_state(cfg, Local::now());
    Ok(())
}

fn print_schedule_state(cfg: &Config, now: DateTime<Local>) {
    if cfg.schedules.is_empty() {
        println!("active schedules: none configured");
        return;
    }

    let active = active_schedules(cfg, now);
    if active.is_empty() {
        println!("active schedules: none");
    } else {
        println!("active schedules: {}", active.join(", "));
        for name in &active {
            if let Some(mode) = cfg
                .schedules
                .get(*name)
                .and_then(|schedule| schedule.mode.as_deref())
            {
                println!("- {name}: mode {mode}");
            }
        }
    }

    println!(
        "scheduled Tier 2 blocking: {}",
        if tier2_enabled_for_schedules(&active_schedule_refs(cfg, &active)) {
            "enabled"
        } else {
            "disabled"
        }
    );
}

fn active_schedules<'a>(cfg: &'a Config, now: DateTime<Local>) -> Vec<&'a str> {
    cfg.schedules
        .iter()
        .filter_map(|(name, schedule)| {
            if schedule_is_active(schedule, now) {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect()
}

fn active_schedule_refs<'a>(cfg: &'a Config, names: &[&str]) -> Vec<&'a Schedule> {
    names
        .iter()
        .filter_map(|name| cfg.schedules.get(*name))
        .collect()
}

fn tier2_enabled_for_schedules(active_names_or_refs: &[&Schedule]) -> bool {
    active_names_or_refs
        .iter()
        .all(|schedule| schedule.tier2_enabled.unwrap_or(true))
}

fn scheduled_tier1_extra_domains(cfg: &Config, active_names: &[&str]) -> Vec<String> {
    let mut domains = Vec::new();
    for schedule in active_schedule_refs(cfg, active_names) {
        for item in &schedule.tier1_extra {
            if let Some(site) = cfg.tier2.get(item) {
                domains.extend(
                    site.domains
                        .iter()
                        .map(|domain| normalize_domain(domain).to_string()),
                );
            } else {
                domains.push(normalize_domain(item).to_string());
            }
        }
    }
    domains.sort();
    domains.dedup();
    domains
}

fn active_schedules_block_site(cfg: &Config, site_name: &str, now: DateTime<Local>) -> bool {
    let active = active_schedules(cfg, now);
    scheduled_tier1_extra_domains(cfg, &active)
        .iter()
        .any(|domain| {
            cfg.tier2
                .get(site_name)
                .is_some_and(|site| site.domains.iter().any(|site_domain| site_domain == domain))
        })
}

fn schedule_is_active(schedule: &Schedule, now: DateTime<Local>) -> bool {
    let Ok(start) = parse_schedule_time(&schedule.start) else {
        return false;
    };
    let Ok(end) = parse_schedule_time(&schedule.end) else {
        return false;
    };
    let now_time = now.time();

    if start < end {
        schedule_includes_weekday(schedule, now.weekday()) && now_time >= start && now_time < end
    } else {
        let previous = previous_weekday(now.weekday());
        (schedule_includes_weekday(schedule, now.weekday()) && now_time >= start)
            || (schedule_includes_weekday(schedule, previous) && now_time < end)
    }
}

fn schedule_includes_weekday(schedule: &Schedule, weekday: Weekday) -> bool {
    schedule
        .days
        .iter()
        .filter_map(|day| parse_weekday(day).ok())
        .any(|day| day == weekday)
}

fn parse_schedule_time(value: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .with_context(|| format!("invalid schedule time {value:?}; expected HH:MM"))
}

fn parse_weekday(value: &str) -> Result<Weekday> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mon" | "monday" => Ok(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Ok(Weekday::Tue),
        "wed" | "wednesday" => Ok(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Ok(Weekday::Thu),
        "fri" | "friday" => Ok(Weekday::Fri),
        "sat" | "saturday" => Ok(Weekday::Sat),
        "sun" | "sunday" => Ok(Weekday::Sun),
        _ => bail!("invalid weekday {value:?}"),
    }
}

fn previous_weekday(weekday: Weekday) -> Weekday {
    match weekday {
        Weekday::Mon => Weekday::Sun,
        Weekday::Tue => Weekday::Mon,
        Weekday::Wed => Weekday::Tue,
        Weekday::Thu => Weekday::Wed,
        Weekday::Fri => Weekday::Thu,
        Weekday::Sat => Weekday::Fri,
        Weekday::Sun => Weekday::Sat,
    }
}

fn install_schedules(config_path: &Path) -> Result<()> {
    let exe = absolute_path(&std::env::current_exe().context("finding current executable path")?)?;
    let config_path = absolute_path(config_path)?;
    let home = home_dir();
    let (service, timer) = render_schedule_units(&exe, &config_path, &home);

    write_root_file(
        &PathBuf::from(format!("/etc/systemd/system/{SCHEDULE_SERVICE}")),
        &service,
    )?;
    write_root_file(
        &PathBuf::from(format!("/etc/systemd/system/{SCHEDULE_TIMER}")),
        &timer,
    )?;
    run(Command::new("sudo").arg("systemctl").arg("daemon-reload"))?;
    run(Command::new("sudo")
        .arg("systemctl")
        .arg("enable")
        .arg("--now")
        .arg(SCHEDULE_TIMER))?;

    println!("Installed and enabled {SCHEDULE_TIMER}.");
    println!("It runs schedule-apply once per minute.");
    Ok(())
}

fn render_schedule_units(exe: &Path, config_path: &Path, home: &Path) -> (String, String) {
    let service = format!(
        "[Unit]\n\
         Description=Apply focus-hosts recurring schedules\n\n\
         [Service]\n\
         Type=oneshot\n\
         Environment=HOME={home}\n\
         ExecStart={exe} --config {config} schedule-apply\n",
        home = systemd_quote(&home.display().to_string()),
        exe = systemd_quote(&exe.display().to_string()),
        config = systemd_quote(&config_path.display().to_string()),
    );

    let timer = format!(
        "[Unit]\n\
         Description=Run focus-hosts schedule checks\n\n\
         [Timer]\n\
         OnBootSec=30s\n\
         OnUnitActiveSec=1min\n\
         Unit={service}\n\n\
         [Install]\n\
         WantedBy=timers.target\n",
        service = SCHEDULE_SERVICE,
    );

    (service, timer)
}

fn uninstall_schedules() -> Result<()> {
    let _ = run(Command::new("sudo")
        .arg("systemctl")
        .arg("disable")
        .arg("--now")
        .arg(SCHEDULE_TIMER));
    run(Command::new("sudo")
        .arg("rm")
        .arg("-f")
        .arg(format!("/etc/systemd/system/{SCHEDULE_SERVICE}"))
        .arg(format!("/etc/systemd/system/{SCHEDULE_TIMER}")))?;
    run(Command::new("sudo").arg("systemctl").arg("daemon-reload"))?;
    println!("Removed {SCHEDULE_TIMER} and {SCHEDULE_SERVICE}.");
    Ok(())
}

fn find_config_path(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }

    if let Ok(path) = std::env::var("FOCUS_HOSTS_CONFIG") {
        return Ok(PathBuf::from(path));
    }

    let local = PathBuf::from("focus-hosts.yml");
    if local.exists() {
        return Ok(local);
    }

    Ok(PathBuf::from(DEFAULT_CONFIG))
}

fn load_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path).with_context(|| {
        format!(
            "reading config {}. Start with focus-hosts.yml.example or create {}",
            path.display(),
            DEFAULT_CONFIG
        )
    })?;
    let mut cfg: Config = serde_yaml::from_str(&content)
        .with_context(|| format!("parsing YAML config {}", path.display()))?;

    cfg.tier1 = cfg
        .tier1
        .into_iter()
        .map(|domain| normalize_domain(&domain).to_string())
        .collect();

    for site in cfg.tier2.values_mut() {
        site.domains = site
            .domains
            .iter()
            .map(|domain| normalize_domain(domain).to_string())
            .collect();
        if let Some(example_url) = &site.example_url {
            let parsed = parse_user_url(example_url)
                .with_context(|| format!("invalid Tier 2 example_url: {example_url}"))?;
            site.example_url = Some(parsed.to_string());
        }
        if site.default_minutes > site.max_minutes {
            site.default_minutes = site.max_minutes;
        }
    }

    for site_name in cfg.allowances.keys() {
        if !cfg.tier2.contains_key(site_name) {
            bail!("allowance {site_name:?} does not match a configured Tier 2 site");
        }
    }

    for (name, schedule) in &mut cfg.schedules {
        parse_schedule_time(&schedule.start)
            .with_context(|| format!("invalid start time in schedule {name:?}"))?;
        parse_schedule_time(&schedule.end)
            .with_context(|| format!("invalid end time in schedule {name:?}"))?;
        for day in &schedule.days {
            parse_weekday(day).with_context(|| format!("invalid day in schedule {name:?}"))?;
        }
        schedule.tier1_extra = schedule
            .tier1_extra
            .iter()
            .map(|item| {
                if cfg.tier2.contains_key(item) {
                    item.to_string()
                } else {
                    normalize_domain(item).to_string()
                }
            })
            .collect();
    }

    cfg.settings.hosts_path = expand_tilde(&cfg.settings.hosts_path);
    cfg.settings.log_path = expand_tilde(&cfg.settings.log_path);
    cfg.settings.state_path = expand_tilde(&cfg.settings.state_path);

    Ok(cfg)
}

fn expand_tilde(path: &Path) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path.to_path_buf();
    };

    if raw == "~" {
        return home_dir();
    }

    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().join(rest);
    }

    path.to_path_buf()
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

fn home_dir() -> PathBuf {
    if is_root() {
        if let Some(user) = std::env::var_os("SUDO_USER") {
            if let Some(home) = passwd_home(&user.to_string_lossy()) {
                return home;
            }
        }
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn passwd_home(user: &str) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    for line in passwd.lines() {
        let mut parts = line.split(':');
        if parts.next()? != user {
            continue;
        }

        let home = parts.nth(4)?;
        return Some(PathBuf::from(home));
    }

    None
}

fn default_hosts_path() -> PathBuf {
    PathBuf::from(DEFAULT_HOSTS)
}

fn default_log_path() -> PathBuf {
    PathBuf::from(DEFAULT_LOG)
}

fn default_state_path() -> PathBuf {
    PathBuf::from(DEFAULT_STATE)
}

fn default_open_limit() -> usize {
    DEFAULT_OPEN_LIMIT
}

fn default_open_minutes() -> u64 {
    DEFAULT_OPEN_MINUTES
}

fn default_redirect_ip() -> String {
    DEFAULT_REDIRECT_IP.to_string()
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn run(command: &mut Command) -> Result<()> {
    let program = command.get_program().to_string_lossy().to_string();
    let args = command
        .get_args()
        .map(OsStr::to_string_lossy)
        .map(|arg| arg.to_string())
        .collect::<Vec<_>>();

    let status = command
        .status()
        .with_context(|| format!("running {program} {}", args.join(" ")))?;
    if !status.success() {
        bail!("{program} {} failed with status {status}", args.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_config() -> Config {
        Config {
            tier1: vec!["twitch.tv".to_string()],
            tier2: BTreeMap::from([(
                "reddit".to_string(),
                Tier2Site {
                    domains: vec!["reddit.com".to_string(), "old.reddit.com".to_string()],
                    example_url: Some("https://old.reddit.com/r/rust/".to_string()),
                    default_minutes: 2,
                    max_minutes: 2,
                    cooldown_seconds: 0,
                },
            )]),
            allowances: BTreeMap::new(),
            schedules: BTreeMap::new(),
            settings: Settings::default(),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "focus-hosts-test-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn classifies_tiers() {
        let cfg = test_config();

        assert!(matches!(
            classify_url(&cfg, "https://www.twitch.tv/foo").unwrap(),
            Classification::Tier1 { .. }
        ));
        assert!(matches!(
            classify_url(&cfg, "https://old.reddit.com/r/rust").unwrap(),
            Classification::Tier2 { site: "reddit", .. }
        ));
        assert!(matches!(
            classify_url(&cfg, "https://example.com").unwrap(),
            Classification::Unknown { .. }
        ));
    }

    #[test]
    fn classifies_subdomains_and_rejects_similar_domains() {
        let cfg = test_config();

        assert!(matches!(
            classify_url(&cfg, "https://clips.twitch.tv/example").unwrap(),
            Classification::Tier1 {
                domain: "twitch.tv"
            }
        ));
        assert!(matches!(
            classify_url(&cfg, "https://not-twitch.tv/example").unwrap(),
            Classification::Unknown { .. }
        ));
        assert!(domain_matches("old.reddit.com", "reddit.com"));
        assert!(!domain_matches("badreddit.com", "reddit.com"));
    }

    #[test]
    fn rejects_invalid_or_hostless_urls() {
        let cfg = test_config();

        assert!(classify_url(&cfg, "not a url").is_err());
        assert!(classify_url(&cfg, "file:///tmp/example").is_err());
    }

    #[test]
    fn accepts_urls_without_an_explicit_scheme() {
        let cfg = test_config();

        assert!(matches!(
            classify_url(&cfg, "old.reddit.com/r/rust").unwrap(),
            Classification::Tier2 { site: "reddit", .. }
        ));
    }

    #[test]
    fn normalizes_domain_text_for_configured_domains() {
        assert_eq!(normalize_domain(" https://reddit.com./ "), "reddit.com");
        assert_eq!(normalize_domain("http://old.reddit.com/"), "old.reddit.com");
    }

    #[test]
    fn tier2_example_urls_are_configurable_with_a_safe_fallback() {
        let cfg = test_config();
        let reddit = cfg.tier2.get("reddit").unwrap();
        let fallback = Tier2Site {
            domains: vec!["youtube.com".to_string()],
            example_url: None,
            default_minutes: 2,
            max_minutes: 2,
            cooldown_seconds: 0,
        };

        assert_eq!(
            example_url_for_site(reddit),
            "https://old.reddit.com/r/rust/"
        );
        assert_eq!(example_url_for_site(&fallback), "https://youtube.com/");
    }

    #[test]
    fn strips_existing_managed_block() {
        let input =
            "127.0.0.1 localhost\n# BEGIN focus-hosts\n0.0.0.0 reddit.com\n# END focus-hosts\n";
        assert_eq!(strip_managed_block(input).trim(), "127.0.0.1 localhost");
    }

    #[test]
    fn strips_only_managed_content_and_keeps_surrounding_lines() {
        let input = "before\n# BEGIN focus-hosts\nblocked\n# END focus-hosts\nafter\n";
        assert_eq!(strip_managed_block(input), "before\nafter");
    }

    #[test]
    fn render_managed_block_contains_all_configured_blocks() {
        let cfg = test_config();
        let block = render_managed_block(&cfg, None);

        assert!(block.starts_with(BEGIN_MARKER));
        assert!(block.contains("# Managed by focus-hosts."));
        assert!(block.contains("0.0.0.0 twitch.tv"));
        assert!(block.contains("0.0.0.0 reddit.com"));
        assert!(block.contains("0.0.0.0 old.reddit.com"));
        assert!(block.ends_with(END_MARKER));
    }

    #[test]
    fn render_omits_temporarily_allowed_tier2_site() {
        let cfg = test_config();
        let block = render_managed_block(&cfg, Some("reddit"));

        assert!(block.contains("0.0.0.0 twitch.tv"));
        assert!(block.contains("# Tier 2 temporarily open: reddit"));
        assert!(!block.contains("0.0.0.0 reddit.com"));
    }

    #[test]
    fn active_schedule_can_disable_tier2_and_add_extra_tier1_domains() {
        let mut cfg = test_config();
        let today = Local::now().weekday();
        cfg.schedules.insert(
            "deep_work".to_string(),
            Schedule {
                days: vec![format!("{today:?}").to_ascii_lowercase()],
                start: "00:00".to_string(),
                end: "00:00".to_string(),
                tier1_extra: vec!["reddit".to_string()],
                tier2_enabled: Some(false),
                mode: Some("strict".to_string()),
            },
        );

        let block = render_managed_block(&cfg, None);

        assert!(block.contains("0.0.0.0 twitch.tv"));
        assert!(block.contains("0.0.0.0 reddit.com"));
        assert!(block.contains("# Tier 2 disabled by active schedule"));
    }

    #[test]
    fn build_hosts_content_preserves_unmanaged_lines_and_replaces_old_block() {
        let cfg = test_config();
        let current =
            "127.0.0.1 localhost\n# BEGIN focus-hosts\nold block\n# END focus-hosts\n::1 localhost\n";
        let next = build_hosts_content(current, &cfg, None);

        assert!(next.contains("127.0.0.1 localhost"));
        assert!(next.contains("::1 localhost"));
        assert!(next.contains("0.0.0.0 twitch.tv"));
        assert!(!next.contains("old block"));
        assert_eq!(next.matches(BEGIN_MARKER).count(), 1);
        assert_eq!(next.matches(END_MARKER).count(), 1);
        assert!(next.ends_with('\n'));
    }

    #[test]
    fn build_hosts_content_can_render_only_the_managed_block() {
        let cfg = test_config();
        let next = build_hosts_content("", &cfg, None);

        assert!(next.starts_with(BEGIN_MARKER));
        assert!(next.ends_with('\n'));
    }

    #[test]
    fn load_config_applies_defaults_normalization_and_maximum_window() {
        let dir = temp_dir("config");
        let config_path = dir.join("config.yml");
        let hosts_path = dir.join("hosts");
        let log_path = dir.join("access.jsonl");
        let state_path = dir.join("open.json");
        let yaml = format!(
            "\
tier1:
  - https://twitch.tv/
tier2:
  reddit:
    domains:
      - https://reddit.com/
      - old.reddit.com.
    example_url: old.reddit.com/r/rust/
    default_minutes: 5
    max_minutes: 2
allowances:
  reddit:
    daily_minutes: 10
    max_session_minutes: 3
schedules:
  workday:
    days: [mon, tue, wed, thu, fri]
    start: \"09:00\"
    end: \"17:30\"
    tier1_extra:
      - reddit
    tier2_enabled: false
settings:
  hosts_path: {}
  log_path: {}
  state_path: {}
  redirect_ip: 127.0.0.1
",
            hosts_path.display(),
            log_path.display(),
            state_path.display()
        );
        fs::write(&config_path, yaml).unwrap();

        let cfg = load_config(&config_path).unwrap();
        let reddit = cfg.tier2.get("reddit").unwrap();

        assert_eq!(cfg.tier1, vec!["twitch.tv"]);
        assert_eq!(reddit.domains, vec!["reddit.com", "old.reddit.com"]);
        assert_eq!(
            reddit.example_url.as_deref(),
            Some("https://old.reddit.com/r/rust/")
        );
        assert_eq!(reddit.default_minutes, 2);
        assert_eq!(reddit.max_minutes, 2);
        assert_eq!(reddit.cooldown_seconds, 0);
        let allowance = cfg.allowances.get("reddit").unwrap();
        assert_eq!(allowance.daily_minutes, 10);
        assert_eq!(allowance.max_session_minutes, Some(3));
        let schedule = cfg.schedules.get("workday").unwrap();
        assert_eq!(schedule.days, vec!["mon", "tue", "wed", "thu", "fri"]);
        assert_eq!(schedule.tier1_extra, vec!["reddit"]);
        assert_eq!(schedule.tier2_enabled, Some(false));
        assert_eq!(cfg.settings.hosts_path, hosts_path);
        assert_eq!(cfg.settings.log_path, log_path);
        assert_eq!(cfg.settings.state_path, state_path);
        assert_eq!(cfg.settings.open_limit_per_hour, DEFAULT_OPEN_LIMIT);
        assert_eq!(cfg.settings.redirect_ip, "127.0.0.1");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_config_expands_tilde_paths() {
        let dir = temp_dir("tilde-config");
        let config_path = dir.join("config.yml");
        fs::write(
            &config_path,
            "\
tier1: []
tier2: {}
settings:
  hosts_path: ~/focus-hosts-test-hosts
  log_path: ~/focus-hosts-test-log.jsonl
  state_path: ~/focus-hosts-test-open.json
",
        )
        .unwrap();

        let cfg = load_config(&config_path).unwrap();
        let home = home_dir();

        assert_eq!(cfg.settings.hosts_path, home.join("focus-hosts-test-hosts"));
        assert_eq!(
            cfg.settings.log_path,
            home.join("focus-hosts-test-log.jsonl")
        );
        assert_eq!(
            cfg.settings.state_path,
            home.join("focus-hosts-test-open.json")
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_config_path_wins() {
        let path = PathBuf::from("/tmp/explicit-focus-hosts.yml");
        assert_eq!(find_config_path(Some(&path)).unwrap(), path);
    }

    #[test]
    fn read_logs_returns_empty_when_missing() {
        let dir = temp_dir("missing-log");
        let entries = read_logs(&dir.join("missing.jsonl")).unwrap();

        assert!(entries.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn read_logs_skips_malformed_lines() {
        let dir = temp_dir("malformed-log");
        let log_path = dir.join("access.jsonl");
        let valid = LogEntry {
            ts: Utc::now(),
            action: "allow".to_string(),
            site: Some("reddit".to_string()),
            url: Some("https://reddit.com".to_string()),
            reason: Some("test".to_string()),
            minutes: Some(2),
            detail: None,
        };
        fs::write(
            &log_path,
            format!(
                "{}\nthis is not json\n{}\n",
                serde_json::to_string(&valid).unwrap(),
                serde_json::to_string(&valid).unwrap()
            ),
        )
        .unwrap();

        let entries = read_logs(&log_path).unwrap();

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.action == "allow"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn count_recent_allows_filters_action_and_time_window() {
        let dir = temp_dir("logs");
        let log_path = dir.join("access.jsonl");
        let now = Utc::now();
        let entries = [
            LogEntry {
                ts: now,
                action: "allow".to_string(),
                site: Some("reddit".to_string()),
                url: Some("https://reddit.com".to_string()),
                reason: Some("test".to_string()),
                minutes: Some(2),
                detail: None,
            },
            LogEntry {
                ts: now - Duration::hours(2),
                action: "allow".to_string(),
                site: Some("reddit".to_string()),
                url: None,
                reason: None,
                minutes: Some(2),
                detail: None,
            },
            LogEntry {
                ts: now,
                action: "deny".to_string(),
                site: Some("reddit".to_string()),
                url: None,
                reason: None,
                minutes: None,
                detail: None,
            },
        ];
        let content = entries
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        fs::write(&log_path, content).unwrap();

        assert_eq!(
            count_recent_allows(&log_path, now - Duration::hours(1)).unwrap(),
            1
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn allowances_count_today_and_cap_sessions() {
        let dir = temp_dir("allowances");
        let log_path = dir.join("access.jsonl");
        let mut cfg = test_config();
        cfg.settings.log_path = log_path.clone();
        cfg.allowances.insert(
            "reddit".to_string(),
            Allowance {
                daily_minutes: 10,
                max_session_minutes: Some(4),
            },
        );
        let now = Utc::now();
        let entries = [
            LogEntry {
                ts: now - Duration::minutes(10),
                action: "allow".to_string(),
                site: Some("reddit".to_string()),
                url: None,
                reason: Some("test".to_string()),
                minutes: Some(7),
                detail: None,
            },
            LogEntry {
                ts: now - Duration::minutes(5),
                action: "allow".to_string(),
                site: Some("other".to_string()),
                url: None,
                reason: None,
                minutes: Some(10),
                detail: None,
            },
        ];
        let content = entries
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        fs::write(&log_path, content).unwrap();

        let status = allowance_status(&cfg, "reddit", now).unwrap().unwrap();
        assert_eq!(status.used_minutes, 7);
        assert_eq!(status.remaining_minutes, 3);
        assert_eq!(cap_minutes_by_allowance(&cfg, "reddit", 6, now).unwrap(), 3);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn gui_dashboard_payload_uses_logs_allowances_and_paths() {
        let dir = temp_dir("gui-dashboard");
        let log_path = dir.join("access.jsonl");
        let state_path = dir.join("open.json");
        let config_path = dir.join("config.yml");
        let mut cfg = test_config();
        cfg.settings.log_path = log_path.clone();
        cfg.settings.state_path = state_path;
        cfg.settings.hosts_path = dir.join("hosts");
        cfg.allowances.insert(
            "reddit".to_string(),
            Allowance {
                daily_minutes: 10,
                max_session_minutes: Some(2),
            },
        );
        let now = Utc::now();
        let entries = [
            LogEntry {
                ts: now,
                action: "allow".to_string(),
                site: Some("reddit".to_string()),
                url: Some("https://old.reddit.com/r/rust/".to_string()),
                reason: Some("break".to_string()),
                minutes: Some(2),
                detail: None,
            },
            LogEntry {
                ts: now,
                action: "deny".to_string(),
                site: Some("reddit".to_string()),
                url: None,
                reason: None,
                minutes: None,
                detail: Some("test deny".to_string()),
            },
        ];
        let content = entries
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        fs::write(&log_path, content).unwrap();

        let dashboard = build_gui_dashboard(&cfg, &config_path).unwrap();

        assert_eq!(dashboard.status.tier1_domains, 1);
        assert_eq!(dashboard.status.tier2_sites, 1);
        assert_eq!(dashboard.status.opens_used_this_hour, 1);
        assert_eq!(dashboard.today_summary.opens, 1);
        assert_eq!(dashboard.today_summary.denied, 1);
        assert_eq!(dashboard.allowances[0].used_minutes, 2);
        assert_eq!(dashboard.allowances[0].remaining_minutes, 8);
        assert_eq!(dashboard.top_sites_week[0].label, "reddit");
        assert_eq!(dashboard.common_reasons_week[0].label, "break");
        assert_eq!(dashboard.paths.config, config_path.display().to_string());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn read_runtime_state_handles_missing_empty_and_valid_files() {
        let dir = temp_dir("state");
        let state_path = dir.join("open.json");

        assert!(read_runtime_state(&state_path).unwrap().is_none());

        fs::write(&state_path, "\n").unwrap();
        assert!(read_runtime_state(&state_path).unwrap().is_none());

        let expected = RuntimeState {
            site: "reddit".to_string(),
            expires_at: Utc::now() + Duration::minutes(2),
            browser_pgid: Some(123),
            profile_path: Some(dir.join("profile")),
        };
        fs::write(&state_path, serde_json::to_string(&expected).unwrap()).unwrap();
        let actual = read_runtime_state(&state_path).unwrap().unwrap();

        assert_eq!(actual.site, expected.site);
        assert_eq!(actual.browser_pgid, expected.browser_pgid);
        assert_eq!(actual.profile_path, expected.profile_path);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn utility_helpers_make_safe_names_and_systemd_values() {
        assert_eq!(sanitize_name("reddit/main feed"), "reddit-main-feed");
        assert_eq!(
            systemd_quote(r#"/tmp/a "quoted" path"#),
            r#""/tmp/a \"quoted\" path""#
        );
        assert_eq!(format_remaining(Duration::seconds(-1)), "00:00");
        assert_eq!(format_remaining(Duration::seconds(65)), "01:05");
        assert_eq!(format_remaining(Duration::seconds(3661)), "1:01:01");
    }

    #[test]
    fn render_watchdog_units_points_to_hosts_file_and_repair_command() {
        let mut cfg = test_config();
        cfg.settings.hosts_path = PathBuf::from("/tmp/test hosts");

        let (service, path_unit) = render_watchdog_units(
            &cfg,
            Path::new("/usr/local/bin/focus-hosts"),
            Path::new("/etc/focus-hosts/config.yml"),
            Path::new("/home/christian"),
            Path::new("/repo/README.md"),
        );

        assert!(service.contains("Documentation=file:///repo/README.md"));
        assert!(service.contains(
            "ExecStart=\"/usr/local/bin/focus-hosts\" --config \"/etc/focus-hosts/config.yml\" watch-repair"
        ));
        assert!(path_unit.contains("PathChanged=/tmp/test hosts"));
        assert!(path_unit.contains(&format!("Unit={WATCHDOG_SERVICE}")));
        assert!(path_unit.contains("WantedBy=multi-user.target"));
    }

    #[test]
    fn render_schedule_units_runs_schedule_apply() {
        let (service, timer) = render_schedule_units(
            Path::new("/usr/local/bin/focus-hosts"),
            Path::new("/etc/focus-hosts/config.yml"),
            Path::new("/home/christian"),
        );

        assert!(service.contains(
            "ExecStart=\"/usr/local/bin/focus-hosts\" --config \"/etc/focus-hosts/config.yml\" schedule-apply"
        ));
        assert!(timer.contains("OnUnitActiveSec=1min"));
        assert!(timer.contains(&format!("Unit={SCHEDULE_SERVICE}")));
    }
}
