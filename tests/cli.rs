use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "focus-hosts-cli-test-{name}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_config(dir: &Path) -> PathBuf {
    let config_path = dir.join("config.yml");
    let hosts_path = dir.join("hosts");
    let log_path = dir.join("access.jsonl");
    let state_path = dir.join("open.json");

    fs::write(
        &config_path,
        format!(
            "\
tier1:
  - twitch.tv
tier2:
  youtube:
    domains:
      - youtube.com
      - www.youtube.com
    example_url: https://www.youtube.com/watch?v=fLdSLs09Dk8
    default_minutes: 2
    max_minutes: 2
    cooldown_seconds: 0
  reddit:
    domains:
      - reddit.com
      - old.reddit.com
    example_url: https://old.reddit.com/r/rust/
    default_minutes: 3
    max_minutes: 3
    cooldown_seconds: 0
settings:
  hosts_path: {}
  log_path: {}
  state_path: {}
  open_limit_per_hour: 2
  redirect_ip: 0.0.0.0
",
            hosts_path.display(),
            log_path.display(),
            state_path.display()
        ),
    )
    .unwrap();

    config_path
}

fn write_scheduled_config(dir: &Path) -> PathBuf {
    let config_path = write_config(dir);
    let content = fs::read_to_string(&config_path).unwrap();
    fs::write(
        &config_path,
        format!(
            r#"{content}
schedules:
  always:
    days: [mon, tue, wed, thu, fri, sat, sun]
    start: "00:00"
    end: "00:00"
    tier2_enabled: false
    mode: strict
"#
        ),
    )
    .unwrap();
    config_path
}

fn run_focus(config_path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_focus-hosts"))
        .arg("--config")
        .arg(config_path)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_explain_classifies_tier2_url() {
    let dir = temp_dir("explain");
    let config_path = write_config(&dir);

    let output = run_focus(
        &config_path,
        &["explain", "https://www.youtube.com/watch?v=fLdSLs09Dk8"],
    );

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Allowed through open-for: youtube opens for 2 minutes."));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_status_prints_tier2_examples() {
    let dir = temp_dir("status");
    let config_path = write_config(&dir);

    let output = run_focus(&config_path, &["status"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Tier 2 sites: 2"));
    assert!(stdout.contains("example: \"https://www.youtube.com/watch?v=fLdSLs09Dk8\""));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_examples_prints_quoted_open_for_commands() {
    let dir = temp_dir("examples");
    let config_path = write_config(&dir);

    let output = run_focus(&config_path, &["examples"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Use quotes around URLs"));
    assert!(stdout.contains("focus-hosts open-for \"https://www.youtube.com/watch?v=fLdSLs09Dk8\""));
    assert!(stdout.contains("--reason \"short intentional break\""));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_logs_skips_malformed_historical_lines() {
    let dir = temp_dir("logs");
    let config_path = write_config(&dir);
    let log_path = dir.join("access.jsonl");
    fs::write(
        &log_path,
        "\
{\"ts\":\"2026-05-18T10:00:00Z\",\"action\":\"allow\",\"site\":\"youtube\",\"url\":\"https://www.youtube.com/watch?v=fLdSLs09Dk8\",\"reason\":\"test\",\"minutes\":2,\"detail\":\"temporary Tier 2 opening\"}
not json
{\"ts\":\"2026-05-18T10:02:00Z\",\"action\":\"restore\",\"site\":\"youtube\",\"url\":null,\"reason\":null,\"minutes\":null,\"detail\":\"restored all configured hosts blocks\"}
",
    )
    .unwrap();

    let output = run_focus(&config_path, &["logs", "--tail", "2"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("allow youtube 2m"));
    assert!(stdout.contains("restore youtube -"));
    assert!(stderr.contains("Warning: skipped malformed log line 2"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_summary_prints_local_stats() {
    let dir = temp_dir("summary");
    let config_path = write_config(&dir);
    let log_path = dir.join("access.jsonl");
    let now = chrono::Utc::now();
    fs::write(
        &log_path,
        format!(
            "{}\n{}\n",
            serde_json::json!({
                "ts": now,
                "action": "allow",
                "site": "youtube",
                "url": "https://www.youtube.com/watch?v=fLdSLs09Dk8",
                "reason": "intentional break",
                "minutes": 2,
                "detail": "temporary Tier 2 opening"
            }),
            serde_json::json!({
                "ts": now,
                "action": "deny",
                "site": "youtube",
                "url": "https://www.youtube.com/",
                "reason": null,
                "minutes": null,
                "detail": "open-for hourly limit reached"
            })
        ),
    )
    .unwrap();

    let output = run_focus(&config_path, &["summary", "--today"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("temporary openings: 1"));
    assert!(stdout.contains("total opened minutes: 2"));
    assert!(stdout.contains("denied attempts: 1"));
    assert!(stdout.contains("- youtube: 2 minute(s), 1 opening(s)"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_schedule_status_reports_active_schedule() {
    let dir = temp_dir("schedule-status");
    let config_path = write_scheduled_config(&dir);

    let output = run_focus(&config_path, &["schedule-status"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("active schedules: always"),
        "stdout was:\n{stdout}\nstderr was:\n{}\nconfig was:\n{}",
        String::from_utf8_lossy(&output.stderr),
        fs::read_to_string(&config_path).unwrap(),
    );
    assert!(stdout.contains("scheduled Tier 2 blocking: disabled"));

    let _ = fs::remove_dir_all(dir);
}
