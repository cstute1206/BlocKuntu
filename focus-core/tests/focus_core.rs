use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use focus_core::{
    evaluate_app, evaluate_url, migrate_database, record_visit_end, record_visit_heartbeat,
    record_visit_start, request_unlock, BlockReason, Config, ControlledBlockReason, Database,
    Decision, DetoxSession, Error, EvaluationContext, ProcessIdentity, UnlockError,
};
use rusqlite::Connection;

fn at_utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(0)
        .expect("zero offset is valid")
        .with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .expect("test timestamp is valid")
}

fn context<'a>(
    config: &'a Config,
    database: &'a Database,
    now: DateTime<FixedOffset>,
) -> EvaluationContext<'a> {
    EvaluationContext::new(config, database, now)
}

#[test]
fn url_matching_supports_subdomains_exact_urls_path_prefixes_and_fallback_allow() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "domain-hard"
        name = "Domain hard block"
        tier = "hard"
        patterns = [
          { kind = "domain", value = "example.com", match_subdomains = true }
        ]

        [[rules]]
        id = "exact-hard"
        name = "Exact hard block"
        tier = "hard"
        patterns = [
          { kind = "exact_url", value = "https://news.example.org/blocked/path" }
        ]

        [[rules]]
        id = "path-hard"
        name = "Path hard block"
        tier = "hard"
        patterns = [
          { kind = "path_prefix", value = "docs.example.net/private" }
        ]

        [[rules]]
        id = "contains-hard"
        name = "Contains hard block"
        tier = "hard"
        patterns = [
          { kind = "url_contains", value = "watch?v=shorts" }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    assert!(matches!(
        evaluate_url("https://example.com/", &ctx),
        Decision::Block(BlockReason::HardBlock { rule_id, .. }) if rule_id == "domain-hard"
    ));
    assert!(matches!(
        evaluate_url("https://deep.sub.example.com/inbox", &ctx),
        Decision::Block(BlockReason::HardBlock { rule_id, .. }) if rule_id == "domain-hard"
    ));
    assert_eq!(
        evaluate_url("https://notexample.com/", &ctx),
        Decision::Allow
    );

    assert!(matches!(
        evaluate_url("https://news.example.org/blocked/path#fragment", &ctx),
        Decision::Block(BlockReason::HardBlock { rule_id, .. }) if rule_id == "exact-hard"
    ));
    assert_eq!(
        evaluate_url("https://news.example.org/blocked/path/child", &ctx),
        Decision::Allow
    );

    assert!(matches!(
        evaluate_url("https://docs.example.net/private/report", &ctx),
        Decision::Block(BlockReason::HardBlock { rule_id, .. }) if rule_id == "path-hard"
    ));
    assert_eq!(
        evaluate_url("https://docs.example.net/public/report", &ctx),
        Decision::Allow
    );

    assert!(matches!(
        evaluate_url("https://video.example/watch?v=SHORTS&clip=1#comments", &ctx),
        Decision::Block(BlockReason::HardBlock { rule_id, .. }) if rule_id == "contains-hard"
    ));
    assert_eq!(
        evaluate_url("https://video.example/watch#v=shorts", &ctx),
        Decision::Allow
    );
}

#[test]
fn app_rules_match_process_identity_and_respect_schedules() {
    let config = Config::from_toml_str(
        r#"
        [[schedules]]
        id = "work-hours"
        name = "Work hours"

        [[schedules.windows]]
        weekday = "mon"
        start = "09:00"
        end = "17:00"

        [[app_rules]]
        id = "kmines-hard"
        name = "KMines"
        tier = "hard"
        schedule_ids = ["work-hours"]
        matchers = [
          { kind = "command_name", value = "kmines" },
          { kind = "executable_basename", value = "kmines" },
          { kind = "desktop_id", value = "org.kde.kmines.desktop" },
          { kind = "window_title_contains", value = "KMines" }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let process = ProcessIdentity {
        pid: Some(1234),
        executable_path: Some("/usr/bin/kmines".to_string()),
        executable_basename: Some("kmines".to_string()),
        command_name: Some("kmines".to_string()),
        desktop_id: Some("org.kde.kmines.desktop".to_string()),
        window_titles: vec!["KMines - 4 mines".to_string()],
    };

    let active_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert_eq!(
        evaluate_app(&process, &active_ctx),
        Decision::Block(BlockReason::HardBlock {
            rule_id: "kmines-hard".to_string(),
            rule_name: "KMines".to_string(),
        })
    );

    let inactive_ctx = context(&config, &database, at_utc(2026, 5, 18, 18, 0));
    assert_eq!(
        evaluate_app(&process, &inactive_ctx),
        Decision::Block(BlockReason::HardBlock {
            rule_id: "kmines-hard".to_string(),
            rule_name: "KMines".to_string(),
        })
    );
}

#[test]
fn controlled_app_rules_can_be_unlocked_by_rule_or_matcher_value() {
    let config = Config::from_toml_str(
        r#"
        [[app_rules]]
        id = "game-controlled"
        name = "Game controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        matchers = [
          { kind = "command_name", value = "game-bin" }
        ]

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let process = ProcessIdentity {
        pid: Some(1234),
        executable_path: None,
        executable_basename: None,
        command_name: Some("game-bin".to_string()),
        desktop_id: None,
        window_titles: Vec::new(),
    };
    let before_unlock = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    assert!(evaluate_app(&process, &before_unlock).is_block());
    let unlock = request_unlock(
        "game-bin",
        "I need this application for completing an urgent work task".to_string(),
        &before_unlock,
    )
    .expect("app unlock should be granted");
    assert_eq!(unlock.rule_id, "game-controlled");
    assert_eq!(unlock.minutes, 2);

    let during_unlock = context(&config, &database, at_utc(2026, 5, 18, 10, 1));
    assert_eq!(evaluate_app(&process, &during_unlock), Decision::Allow);
}

#[test]
fn app_rules_match_stable_process_identifiers_case_insensitively() {
    let config = Config::from_toml_str(
        r#"
        [[app_rules]]
        id = "vlc-hard"
        name = "VLC"
        tier = "hard"
        matchers = [
          { kind = "command_name", value = "VLC" },
          { kind = "executable_basename", value = "VLC" },
          { kind = "desktop_id", value = "ORG.VIDEOLAN.VLC.DESKTOP" }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let process = ProcessIdentity {
        pid: Some(4242),
        executable_path: Some("/usr/bin/vlc".to_string()),
        executable_basename: Some("vlc".to_string()),
        command_name: Some("vlc".to_string()),
        desktop_id: Some("org.videolan.vlc.desktop".to_string()),
        window_titles: vec!["VLC media player".to_string()],
    };

    let ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert!(matches!(
        evaluate_app(&process, &ctx),
        Decision::Block(BlockReason::HardBlock { rule_id, .. }) if rule_id == "vlc-hard"
    ));
}

#[test]
fn controlled_app_allowances_use_recorded_runtime() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "game-daily"
        daily_minutes = 1

        [[app_rules]]
        id = "game-controlled"
        name = "Game controlled"
        tier = "controlled_access"
        allowance_id = "game-daily"
        schedule_ids = ["always"]
        matchers = [
          { kind = "command_name", value = "game-bin" }
        ]

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let process = ProcessIdentity {
        pid: Some(1234),
        executable_path: None,
        executable_basename: None,
        command_name: Some("game-bin".to_string()),
        desktop_id: None,
        window_titles: Vec::new(),
    };

    let before_usage = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert_eq!(evaluate_app(&process, &before_usage), Decision::Allow);

    database
        .insert_app_usage_interval(
            "game-controlled",
            at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc),
            at_utc(2026, 5, 18, 10, 1).with_timezone(&Utc),
        )
        .expect("app usage interval should insert");

    let after_allowance = context(&config, &database, at_utc(2026, 5, 18, 10, 1));
    assert_eq!(
        evaluate_app(&process, &after_allowance),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "game-controlled".to_string(),
            rule_name: "Game controlled".to_string(),
            reason: ControlledBlockReason::AllowanceExhausted,
        })
    );
}

#[test]
fn detox_sessions_block_site_rules_until_the_absolute_end_time() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "daily"
        daily_minutes = 60

        [[rules]]
        id = "video"
        name = "Video"
        tier = "controlled_access"
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "video.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let starts_at = at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc);
    let before = context(&config, &database, at_utc(2026, 5, 18, 9, 59));
    assert_eq!(
        evaluate_url("https://watch.video.example/", &before),
        Decision::Allow
    );

    database
        .insert_detox_session(&DetoxSession {
            id: "detox-test".to_string(),
            name: Some("Deep work".to_string()),
            starts_at,
            ends_at: starts_at + chrono::Duration::minutes(60),
            cancelled_at: None,
            site_rule_ids: vec!["video".to_string()],
            app_rule_ids: Vec::new(),
        })
        .expect("detox session should insert");

    let during = context(&config, &database, at_utc(2026, 5, 18, 10, 30));
    assert!(matches!(
        evaluate_url("https://watch.video.example/", &during),
        Decision::Block(BlockReason::Detox {
            session_id,
            rule_id,
            ends_at,
            ..
        }) if session_id == "detox-test"
            && rule_id == "video"
            && ends_at == starts_at + chrono::Duration::minutes(60)
    ));

    let after = context(&config, &database, at_utc(2026, 5, 18, 11, 1));
    assert_eq!(
        evaluate_url("https://watch.video.example/", &after),
        Decision::Allow
    );
}

#[test]
fn detox_sessions_override_active_unlocks_and_inactive_app_schedules() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "social"
        name = "Social"
        tier = "controlled_access"
        schedule_ids = ["work-hours"]
        patterns = [
          { kind = "domain", value = "social.example", match_subdomains = false }
        ]

        [[schedules]]
        id = "work-hours"
        name = "Work hours"

        [[schedules.windows]]
        weekday = "mon"
        start = "09:00"
        end = "17:00"

        [[app_rules]]
        id = "game"
        name = "Game"
        tier = "controlled_access"
        schedule_ids = ["work-hours"]
        matchers = [
          { kind = "command_name", value = "game-bin" }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let starts_at = at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc);
    let before_detox = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    request_unlock(
        "https://social.example/",
        "I need temporary access to complete this scheduled work item".to_string(),
        &before_detox,
    )
    .expect("unlock should be active before detox starts");
    database
        .insert_detox_session(&DetoxSession {
            id: "detox-override".to_string(),
            name: None,
            starts_at,
            ends_at: starts_at + chrono::Duration::hours(10),
            cancelled_at: None,
            site_rule_ids: vec!["social".to_string()],
            app_rule_ids: vec!["game".to_string()],
        })
        .expect("detox session should insert");

    let during = context(&config, &database, at_utc(2026, 5, 18, 10, 1));
    assert!(matches!(
        evaluate_url("https://social.example/", &during),
        Decision::Block(BlockReason::Detox { rule_id, .. }) if rule_id == "social"
    ));

    let process = ProcessIdentity {
        pid: Some(1234),
        executable_path: None,
        executable_basename: None,
        command_name: Some("game-bin".to_string()),
        desktop_id: None,
        window_titles: Vec::new(),
    };
    let before_detox_app = context(&config, &database, at_utc(2026, 5, 18, 8, 0));
    assert_eq!(evaluate_app(&process, &before_detox_app), Decision::Allow);

    let during_app_detox = context(&config, &database, at_utc(2026, 5, 18, 18, 0));
    assert!(matches!(
        evaluate_app(&process, &during_app_detox),
        Decision::Block(BlockReason::Detox { rule_id, .. }) if rule_id == "game"
    ));
}

#[test]
fn detox_rejects_manual_unlock_without_consuming_reason_or_hourly_quota() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "social"
        name = "Social"
        tier = "controlled_access"
        schedule_ids = ["always"]
        patterns = [
          { kind = "domain", value = "social.example", match_subdomains = false }
        ]

        [[schedules]]
        id = "always"
        name = "Always"

        [[schedules.windows]]
        weekday = "mon"
        start = "10:30"
        end = "23:59"
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let starts_at = at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc);
    database
        .insert_detox_session(&DetoxSession {
            id: "detox-no-unlock".to_string(),
            name: Some("Protected focus".to_string()),
            starts_at,
            ends_at: starts_at + chrono::Duration::minutes(30),
            cancelled_at: None,
            site_rule_ids: vec!["social".to_string()],
            app_rule_ids: Vec::new(),
        })
        .expect("detox session should insert");

    let reason = "I need temporary access to complete this specific work item";
    let during = context(&config, &database, at_utc(2026, 5, 18, 10, 5));
    let denied = request_unlock("https://social.example/", reason.to_string(), &during)
        .expect_err("detox should reject manual unlock");
    assert!(
        matches!(
            &denied,
            Error::Unlock(UnlockError::TargetInActiveDetox {
                rule_id,
                session_id,
                ..
            }) if rule_id == "social" && session_id == "detox-no-unlock"
        ),
        "unexpected unlock error: {denied:?}"
    );

    let after = context(&config, &database, at_utc(2026, 5, 18, 10, 31));
    request_unlock("https://social.example/", reason.to_string(), &after)
        .expect("the rejected attempt must not consume its reason or hourly quota");
}

#[test]
fn clock_tamper_evaluation_fails_closed_for_time_sensitive_rules() {
    let config = Config::from_toml_str(
        r#"
        [[schedules]]
        id = "work-hours"
        name = "Work hours"

        [[schedules.windows]]
        weekday = "mon"
        start = "09:00"
        end = "17:00"

        [[schedules]]
        id = "always"
        name = "Always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"

        [[allowances]]
        id = "short"
        name = "Short"
        daily_minutes = 15

        [[rules]]
        id = "controlled"
        name = "Controlled"
        tier = "controlled_access"
        schedule_ids = ["work-hours"]
        allowance_id = "short"
        patterns = [
          { kind = "domain", value = "controlled.example", match_subdomains = true }
        ]

        [[rules]]
        id = "detox-target"
        name = "Detox target"
        tier = "controlled_access"
        schedule_ids = ["work-hours"]
        patterns = [
          { kind = "domain", value = "detox.example", match_subdomains = true }
        ]

        [[rules]]
        id = "unlock-target"
        name = "Unlock target"
        tier = "controlled_access"
        schedule_ids = ["always"]
        patterns = [
          { kind = "domain", value = "unlock.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    database
        .insert_detox_session(&DetoxSession {
            id: "ended-detox".to_string(),
            name: None,
            starts_at: at_utc(2026, 5, 18, 9, 0).with_timezone(&Utc),
            ends_at: at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc),
            cancelled_at: None,
            site_rule_ids: vec!["detox-target".to_string()],
            app_rule_ids: Vec::new(),
        })
        .expect("detox session should insert");

    let inactive = context(&config, &database, at_utc(2026, 5, 18, 18, 0));
    assert_eq!(
        evaluate_url("https://controlled.example/", &inactive),
        Decision::Allow
    );
    assert_eq!(
        evaluate_url("https://detox.example/", &inactive),
        Decision::Allow
    );

    request_unlock(
        "https://unlock.example/",
        "This access is required before testing the clock tamper state".to_string(),
        &inactive,
    )
    .expect("unlock should be granted");
    assert_eq!(
        evaluate_url("https://unlock.example/", &inactive),
        Decision::Allow
    );

    let tampered =
        context(&config, &database, at_utc(2026, 5, 18, 18, 0)).with_clock_tampered(true);
    assert_eq!(
        evaluate_url("https://controlled.example/", &tampered),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "controlled".to_string(),
            rule_name: "Controlled".to_string(),
            reason: ControlledBlockReason::UnlockRequired,
        })
    );
    assert!(matches!(
        evaluate_url("https://detox.example/", &tampered),
        Decision::Block(BlockReason::Detox { session_id, .. }) if session_id == "ended-detox"
    ));
    assert_eq!(
        evaluate_url("https://unlock.example/", &tampered),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "unlock-target".to_string(),
            rule_name: "Unlock target".to_string(),
            reason: ControlledBlockReason::UnlockRequired,
        })
    );
}

#[test]
fn schedules_and_allowances_transition_between_allow_and_block() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "social-daily"
        name = "Social daily allowance"
        daily_minutes = 30

        [[schedules]]
        id = "work-hours"
        name = "Work hours"

        [[schedules.windows]]
        weekday = "mon"
        start = "09:00"
        end = "17:00"

        [[rules]]
        id = "social-controlled"
        name = "Social controlled access"
        tier = "controlled_access"
        schedule_ids = ["work-hours"]
        allowance_id = "social-daily"
        patterns = [
          { kind = "domain", value = "social.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    let active_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert_eq!(
        evaluate_url("https://social.example/feed", &active_ctx),
        Decision::Allow
    );

    database
        .insert_visit_interval(
            "social-controlled",
            "social.example",
            "https://social.example/feed",
            "tab-1",
            at_utc(2026, 5, 18, 9, 0).with_timezone(&Utc),
            at_utc(2026, 5, 18, 9, 31).with_timezone(&Utc),
        )
        .expect("visit interval should insert");

    assert_eq!(
        evaluate_url("https://social.example/feed", &active_ctx),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "social-controlled".to_string(),
            rule_name: "Social controlled access".to_string(),
            reason: ControlledBlockReason::AllowanceExhausted,
        })
    );

    let inactive_ctx = context(&config, &database, at_utc(2026, 5, 18, 18, 0));
    assert_eq!(
        evaluate_url("https://social.example/feed", &inactive_ctx),
        Decision::Allow
    );
}

#[test]
fn zero_minute_allowance_blocks_immediately() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "zero-daily"
        daily_minutes = 0

        [[rules]]
        id = "zero-controlled"
        name = "Zero controlled access"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "zero-daily"
        patterns = [
          { kind = "domain", value = "zero.example", match_subdomains = false }
        ]

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"
        "#,
    )
    .expect("zero-minute allowance should parse");
    let database = Database::in_memory().expect("database should initialize");
    let ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    assert_eq!(
        evaluate_url("https://zero.example/", &ctx),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "zero-controlled".to_string(),
            rule_name: "Zero controlled access".to_string(),
            reason: ControlledBlockReason::AllowanceExhausted,
        })
    );
}

#[test]
fn grouped_schedule_days_apply_as_single_windows() {
    let config = Config::from_toml_str(
        r#"
        [[schedules]]
        id = "workday-hours"
        name = "Workday hours"

        [[schedules.windows]]
        weekday = "workdays"
        start = "09:00"
        end = "17:00"

        [[schedules]]
        id = "weekend-hours"
        name = "Weekend hours"

        [[schedules.windows]]
        weekday = "weekend"
        start = "09:00"
        end = "17:00"

        [[schedules]]
        id = "daily-hours"
        name = "Daily hours"

        [[schedules.windows]]
        weekday = "everyday"
        start = "09:00"
        end = "17:00"

        [[rules]]
        id = "workday-rule"
        name = "Workday rule"
        tier = "controlled_access"
        schedule_ids = ["workday-hours"]
        patterns = [
          { kind = "domain", value = "workday.example", match_subdomains = true }
        ]

        [[rules]]
        id = "weekend-rule"
        name = "Weekend rule"
        tier = "controlled_access"
        schedule_ids = ["weekend-hours"]
        patterns = [
          { kind = "domain", value = "weekend.example", match_subdomains = true }
        ]

        [[rules]]
        id = "daily-rule"
        name = "Daily rule"
        tier = "controlled_access"
        schedule_ids = ["daily-hours"]
        patterns = [
          { kind = "domain", value = "daily.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    database
        .replace_policy_config(&config)
        .expect("grouped schedule config should persist");
    let config = database
        .load_policy_config()
        .expect("grouped schedule config should reload");

    let monday = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert!(evaluate_url("https://workday.example/", &monday).is_block());
    assert_eq!(
        evaluate_url("https://weekend.example/", &monday),
        Decision::Allow
    );
    assert!(evaluate_url("https://daily.example/", &monday).is_block());

    let saturday = context(&config, &database, at_utc(2026, 5, 23, 10, 0));
    assert_eq!(
        evaluate_url("https://workday.example/", &saturday),
        Decision::Allow
    );
    assert!(evaluate_url("https://weekend.example/", &saturday).is_block());
    assert!(evaluate_url("https://daily.example/", &saturday).is_block());
}

#[test]
fn overlapping_rules_apply_the_strictest_active_result() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "broad-daily"
        daily_minutes = 60

        [[allowances]]
        id = "permissive-daily"
        daily_minutes = 60

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"

        [[rules]]
        id = "broad-controlled"
        name = "Broad controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "broad-daily"
        patterns = [
          { kind = "domain", value = "overlap.example", match_subdomains = true }
        ]

        [[rules]]
        id = "strict-controlled"
        name = "Strict controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        patterns = [
          { kind = "exact_url", value = "https://overlap.example/watch" }
        ]

        [[rules]]
        id = "hard-overlap"
        name = "Hard overlap"
        tier = "hard"
        patterns = [
          { kind = "domain", value = "hard-overlap.example", match_subdomains = true }
        ]

        [[rules]]
        id = "permissive-overlap"
        name = "Permissive overlap"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "permissive-daily"
        patterns = [
          { kind = "domain", value = "hard-overlap.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    assert_eq!(
        evaluate_url("https://overlap.example/watch", &ctx),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "strict-controlled".to_string(),
            rule_name: "Strict controlled".to_string(),
            reason: ControlledBlockReason::NoAllowance,
        })
    );
    let unlock = request_unlock(
        "https://overlap.example/watch",
        "I need this exact page to finish reviewing the assigned material".to_string(),
        &ctx,
    )
    .expect("unlock should target the blocking overlap rule");
    assert_eq!(unlock.rule_id, "strict-controlled");
    assert_eq!(
        evaluate_url("https://overlap.example/watch", &ctx),
        Decision::Allow
    );

    assert_eq!(
        evaluate_url("https://hard-overlap.example/", &ctx),
        Decision::Block(BlockReason::HardBlock {
            rule_id: "hard-overlap".to_string(),
            rule_name: "Hard overlap".to_string(),
        })
    );
}

#[test]
fn overlapping_allowances_report_the_strictest_matching_rule() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "broad-daily"
        daily_minutes = 30

        [[allowances]]
        id = "strict-daily"
        daily_minutes = 1

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"

        [[rules]]
        id = "broad-controlled"
        name = "Broad controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "broad-daily"
        patterns = [
          { kind = "domain", value = "overlap.example", match_subdomains = false }
        ]

        [[rules]]
        id = "strict-controlled"
        name = "Strict controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "strict-daily"
        patterns = [
          { kind = "domain", value = "overlap.example", match_subdomains = false }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    database
        .insert_visit_interval(
            "broad-controlled",
            "overlap.example",
            "https://overlap.example/watch",
            "tab-old",
            at_utc(2026, 5, 18, 9, 0).with_timezone(&Utc),
            at_utc(2026, 5, 18, 9, 31).with_timezone(&Utc),
        )
        .expect("visit interval should insert");

    let ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert_eq!(
        evaluate_url("https://overlap.example/watch", &ctx),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "strict-controlled".to_string(),
            rule_name: "Strict controlled".to_string(),
            reason: ControlledBlockReason::AllowanceExhausted,
        })
    );
}

#[test]
fn overlapping_allowance_visits_are_metered_against_the_strictest_rule() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "broad-daily"
        daily_minutes = 30

        [[allowances]]
        id = "strict-daily"
        daily_minutes = 1

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"

        [[rules]]
        id = "broad-controlled"
        name = "Broad controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "broad-daily"
        patterns = [
          { kind = "domain", value = "overlap.example", match_subdomains = false }
        ]

        [[rules]]
        id = "strict-controlled"
        name = "Strict controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "strict-daily"
        patterns = [
          { kind = "domain", value = "overlap.example", match_subdomains = false }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let start_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    let visit = record_visit_start("https://overlap.example/watch", "tab-7", &start_ctx)
        .expect("visit should start");
    assert_eq!(visit.rule_id.as_deref(), Some("strict-controlled"));

    let exhausted_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 2));
    assert_eq!(
        evaluate_url("https://overlap.example/watch", &exhausted_ctx),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "strict-controlled".to_string(),
            rule_name: "Strict controlled".to_string(),
            reason: ControlledBlockReason::AllowanceExhausted,
        })
    );
}

#[test]
fn overnight_schedule_windows_remain_active_after_midnight() {
    let config = Config::from_toml_str(
        r#"
        [[schedules]]
        id = "late"

        [[schedules.windows]]
        weekday = "fri"
        start = "22:00"
        end = "02:00"

        [[rules]]
        id = "late-hard"
        name = "Late controlled block"
        tier = "controlled_access"
        schedule_ids = ["late"]
        patterns = [
          { kind = "domain", value = "late.example", match_subdomains = false }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    let friday_late = context(&config, &database, at_utc(2026, 5, 22, 23, 0));
    let saturday_early = context(&config, &database, at_utc(2026, 5, 23, 1, 0));
    let saturday_late = context(&config, &database, at_utc(2026, 5, 23, 3, 0));

    assert!(evaluate_url("https://late.example/", &friday_late).is_block());
    assert!(evaluate_url("https://late.example/", &saturday_early).is_block());
    assert_eq!(
        evaluate_url("https://late.example/", &saturday_late),
        Decision::Allow
    );
}

#[test]
fn unlocks_apply_to_the_matched_site_rule_and_use_fixed_quota() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "controlled-no-allowance"
        name = "Controlled without allowance"
        tier = "controlled_access"
        schedule_ids = ["always"]
        patterns = [
          { kind = "domain", value = "focus.example", match_subdomains = true }
        ]

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let before_unlock = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    assert_eq!(
        evaluate_url("https://focus.example/", &before_unlock),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "controlled-no-allowance".to_string(),
            rule_name: "Controlled without allowance".to_string(),
            reason: ControlledBlockReason::NoAllowance,
        })
    );

    let unlock = request_unlock(
        "focus.example",
        "I need access to finish the current assigned research task".to_string(),
        &before_unlock,
    )
    .expect("unlock should be granted");
    assert_eq!(unlock.rule_id, "controlled-no-allowance");
    assert_eq!(unlock.target, "https://focus.example/");
    assert_eq!(unlock.minutes, 2);

    let during_unlock = context(&config, &database, at_utc(2026, 5, 18, 10, 1));
    assert_eq!(
        evaluate_url("https://focus.example/", &during_unlock),
        Decision::Allow
    );
    assert_eq!(
        evaluate_url("https://focus.example/watch?v=abc", &during_unlock),
        Decision::Allow
    );
    assert_eq!(
        evaluate_url("http://focus.example/", &during_unlock),
        Decision::Allow
    );
    assert_eq!(
        evaluate_url("https://www.focus.example/news", &during_unlock),
        Decision::Allow
    );

    let duplicate = request_unlock(
        "https://focus.example/",
        "I am requesting another access period while the first remains active".to_string(),
        &during_unlock,
    )
    .expect_err("active unlock should prevent a duplicate");
    assert!(matches!(
        duplicate,
        Error::Unlock(UnlockError::UnlockAlreadyActive { .. })
    ));

    let quota_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 3));
    let quota = request_unlock(
        "https://focus.example/other",
        "I need a different link for another part of the current task".to_string(),
        &quota_ctx,
    )
    .expect_err("quota should block another unlock in the same hour");
    assert!(matches!(
        quota,
        Error::Unlock(UnlockError::HourlyQuotaExceeded { limit: 1, .. })
    ));

    assert!(evaluate_url("https://focus.example/", &quota_ctx).is_block());

    let next_hour = context(&config, &database, at_utc(2026, 5, 18, 11, 1));
    let next_unlock = request_unlock(
        "https://focus.example/other",
        "I need this page for a separate task after the hourly limit reset".to_string(),
        &next_hour,
    )
    .expect("next-hour unlock should pass");
    assert_eq!(next_unlock.target, "https://focus.example/other");
}

#[test]
fn hourly_unlock_quota_applies_to_site_rules() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "quota-controlled"
        name = "Quota controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        patterns = [
          { kind = "domain", value = "quota.example", match_subdomains = false }
        ]

        [[rules]]
        id = "other-controlled"
        name = "Other controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        patterns = [
          { kind = "domain", value = "other.example", match_subdomains = false }
        ]

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    let first = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    request_unlock(
        "https://quota.example/",
        "I need the first temporary access period for this work item".to_string(),
        &first,
    )
    .expect("first unlock should pass");

    let second = context(&config, &database, at_utc(2026, 5, 18, 10, 10));
    let denied = request_unlock(
        "https://other.example/second",
        "I need a second temporary access period for another work item".to_string(),
        &second,
    )
    .expect_err("a second global unlock in an hour should be denied");
    assert!(matches!(
        denied,
        Error::Unlock(UnlockError::HourlyQuotaExceeded { limit: 1 })
    ));
}

#[test]
fn unlock_reasons_require_twenty_letters_and_cannot_be_reused() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "reason-controlled"
        name = "Reason controlled"
        tier = "controlled_access"
        schedule_ids = ["always"]
        patterns = [
          { kind = "domain", value = "reason.example", match_subdomains = false }
        ]

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let first = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    let too_short = request_unlock(
        "https://reason.example/",
        "Need this now".to_string(),
        &first,
    )
    .expect_err("short reason should be rejected");
    assert!(matches!(
        too_short,
        Error::Unlock(UnlockError::ReasonTooShort {
            minimum: 20,
            actual: 11
        })
    ));

    request_unlock(
        "https://reason.example/",
        "I need this page to complete the assigned review task".to_string(),
        &first,
    )
    .expect("first reason should be accepted");

    let next_hour = context(&config, &database, at_utc(2026, 5, 18, 11, 1));
    let reused = request_unlock(
        "https://reason.example/",
        "  i NEED   this PAGE to complete the assigned REVIEW task  ".to_string(),
        &next_hour,
    )
    .expect_err("normalized duplicate reason should be rejected");
    assert!(matches!(
        reused,
        Error::Unlock(UnlockError::ReasonAlreadyUsed)
    ));
}

#[test]
fn database_migration_creates_required_tables_and_runtime_tables_work() {
    let database = Database::in_memory().expect("database should initialize");
    let mut statement = database
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .expect("sqlite_master query should prepare");
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("table query should run");
    let mut tables = rows
        .map(|row| row.expect("table row should decode"))
        .collect::<Vec<_>>();
    tables.sort();

    for required in [
        "unlocks",
        "visits",
        "app_usage_sessions",
        "events",
        "heartbeats",
        "service_state",
        "detox_sessions",
        "detox_session_site_rules",
        "detox_session_app_rules",
        "policy_defaults",
        "policy_strict_mode",
        "policy_allowances",
        "policy_schedules",
        "policy_schedule_windows",
        "policy_site_lists",
        "policy_site_list_patterns",
        "policy_site_list_schedules",
        "policy_app_rules",
        "policy_app_rule_matchers",
        "policy_app_rule_schedules",
    ] {
        assert!(
            tables.iter().any(|table| table == required),
            "missing required table {required}; present tables: {tables:?}"
        );
    }

    let policy_allowances_sql: String = database
        .connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'policy_allowances'",
            [],
            |row| row.get(0),
        )
        .expect("policy_allowances schema should query");
    assert!(policy_allowances_sql.contains("daily_minutes >= 0"));

    let policy_patterns_sql: String = database
        .connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'policy_site_list_patterns'",
            [],
            |row| row.get(0),
        )
        .expect("policy_site_list_patterns schema should query");
    assert!(policy_patterns_sql.contains("'url_contains'"));

    let now = at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc);
    database
        .upsert_heartbeat("extension", Some("ok"), now)
        .expect("heartbeat should upsert");
    database
        .set_service_state("mode", "enforcing", now)
        .expect("state should upsert");
    assert_eq!(
        database
            .service_state("mode")
            .expect("state should query")
            .as_deref(),
        Some("enforcing")
    );
}

#[test]
fn database_migration_adds_url_contains_to_existing_site_pattern_constraint() {
    let conn = Connection::open_in_memory().expect("database should open");
    conn.execute_batch(
        r#"
        CREATE TABLE policy_site_list_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            list_id TEXT NOT NULL,
            kind TEXT NOT NULL CHECK (kind IN ('domain', 'exact_url', 'url_prefix', 'path_prefix')),
            value TEXT NOT NULL,
            match_subdomains INTEGER NOT NULL DEFAULT 0,
            position INTEGER NOT NULL DEFAULT 0
        );

        INSERT INTO policy_site_list_patterns (
            list_id,
            kind,
            value,
            match_subdomains,
            position
        )
        VALUES ('legacy', 'url_prefix', 'https://legacy.example/', 0, 7);
        "#,
    )
    .expect("legacy schema should initialize");

    migrate_database(&conn).expect("migration should add url_contains");

    let table_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'policy_site_list_patterns'",
            [],
            |row| row.get(0),
        )
        .expect("schema should query");
    assert!(table_sql.contains("'url_contains'"));

    conn.execute(
        r#"
        INSERT INTO policy_site_lists (
            id,
            name,
            tier,
            enabled
        )
        VALUES (?1, ?2, ?3, ?4)
        "#,
        ("new", "New", "hard", 1_i64),
    )
    .expect("site list should insert");

    conn.execute(
        r#"
        INSERT INTO policy_site_list_patterns (
            list_id,
            kind,
            value,
            match_subdomains,
            position
        )
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        ("new", "url_contains", "watch?v=shorts", 0_i64, 1_i64),
    )
    .expect("url_contains should satisfy migrated constraint");

    let legacy_value: String = conn
        .query_row(
            "SELECT value FROM policy_site_list_patterns WHERE list_id = 'legacy'",
            [],
            |row| row.get(0),
        )
        .expect("legacy row should survive migration");
    assert_eq!(legacy_value, "https://legacy.example/");
}

#[test]
fn database_migration_relaxes_policy_allowance_zero_constraint() {
    let conn = Connection::open_in_memory().expect("database should open");
    conn.execute_batch(
        r#"
        CREATE TABLE policy_allowances (
            id TEXT PRIMARY KEY,
            name TEXT,
            daily_minutes INTEGER NOT NULL CHECK (daily_minutes > 0)
        );

        INSERT INTO policy_allowances (id, name, daily_minutes)
        VALUES ('old-daily', 'Old daily', 15);
        "#,
    )
    .expect("old policy_allowances schema should create");

    migrate_database(&conn).expect("database should migrate");

    let policy_allowances_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'policy_allowances'",
            [],
            |row| row.get(0),
        )
        .expect("policy_allowances schema should query");
    assert!(policy_allowances_sql.contains("daily_minutes >= 0"));

    let old_daily: i64 = conn
        .query_row(
            "SELECT daily_minutes FROM policy_allowances WHERE id = 'old-daily'",
            [],
            |row| row.get(0),
        )
        .expect("old allowance should be preserved");
    assert_eq!(old_daily, 15);

    conn.execute(
        "INSERT INTO policy_allowances (id, name, daily_minutes) VALUES ('zero-daily', NULL, 0)",
        [],
    )
    .expect("zero-minute allowance should insert after migration");
}

#[test]
fn policy_config_roundtrips_through_sqlite() {
    let config = Config::from_toml_str(
        r#"
        [strict_mode]
        require_firefox_extension = true
        require_chrome_extension = true
        kill_supported_browser_if_extension_stale = true
        block_unsupported_browsers = true
        grace_seconds = 30

        [[allowances]]
        id = "daily"
        name = "Daily allowance"
        daily_minutes = 0

        [[schedules]]
        id = "work"
        name = "Work"

        [[schedules.windows]]
        weekday = "mon"
        start = "09:00"
        end = "17:00"

        [[rules]]
        id = "hard-list"
        name = "Hard List"
        tier = "hard"
        schedule_ids = ["work"]
        patterns = [
          { kind = "domain", value = "hard.example", match_subdomains = true }
        ]

        [[rules]]
        id = "tier-two-list"
        name = "Tier 2 List"
        tier = "controlled_access"
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "tier-two.example", match_subdomains = false },
          { kind = "url_prefix", value = "https://video.example/watch/" }
        ]

        [[app_rules]]
        id = "game-controlled"
        name = "Game controlled"
        tier = "controlled_access"
        matchers = [
          { kind = "command_name", value = "game-bin" },
          { kind = "window_title_contains", value = "Game" }
        ]

        [[app_rules]]
        id = "kmines-hard"
        name = "KMines"
        tier = "hard"
        schedule_ids = ["work"]
        matchers = [
          { kind = "executable_basename", value = "kmines" },
          { kind = "desktop_id", value = "org.kde.kmines.desktop" }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    assert!(!database
        .has_policy_config()
        .expect("policy presence should query"));
    database
        .replace_policy_config(&config)
        .expect("policy config should persist");
    assert!(database
        .has_policy_config()
        .expect("policy presence should query"));

    let loaded = database
        .load_policy_config()
        .expect("policy config should load");
    assert_eq!(loaded, config);
}

#[test]
fn empty_policy_config_is_still_marked_as_persisted() {
    let config = Config::default();
    let database = Database::in_memory().expect("database should initialize");

    assert!(!database
        .has_policy_config()
        .expect("new database should not have policy"));
    database
        .replace_policy_config(&config)
        .expect("empty policy config should persist");
    assert!(database
        .has_policy_config()
        .expect("persisted empty policy should be detected"));
    assert_eq!(
        database
            .load_policy_config()
            .expect("empty policy should load"),
        config
    );
}

#[test]
fn policy_config_roundtrips_through_toml_export() {
    let config = Config::from_toml_str(
        r#"
        [strict_mode]
        require_firefox_extension = true
        require_chrome_extension = true
        kill_supported_browser_if_extension_stale = true
        block_unsupported_browsers = true
        grace_seconds = 30

        [[rules]]
        id = "hard-list"
        name = "Hard List"
        tier = "hard"
        patterns = [
          { kind = "domain", value = "hard.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("config should parse");

    let exported = config
        .to_toml_string()
        .expect("config should serialize to TOML");
    let imported = Config::from_toml_str(&exported).expect("exported TOML should parse");

    assert_eq!(imported, config);
    assert!(exported.contains("[[rules]]"));
    assert!(exported.contains("hard.example"));
    assert!(!exported.contains("unlock_policy"));
    assert!(!exported.contains("[defaults]"));
}

#[test]
fn corrupt_and_unsafe_configurations_fail_safely() {
    assert!(Config::from_toml_str("not valid toml =").is_err());

    let no_patterns = Config::from_toml_str(
        r#"
        [[rules]]
        id = "bad"
        name = "Bad"
        tier = "hard"
        "#,
    );
    assert!(no_patterns.is_err());

    let hard_with_unlock = Config::from_toml_str(
        r#"
        [[rules]]
        id = "hard"
        name = "Hard"
        tier = "hard"
        patterns = [
          { kind = "domain", value = "hard.example", match_subdomains = false }
        ]

        [rules.unlock_policy]
        max_session_minutes = 5
        cooldown_minutes = 10
        max_unlocks_per_hour = 1
        "#,
    );
    assert!(hard_with_unlock.is_err());

    let app_without_matchers = Config::from_toml_str(
        r#"
        [[app_rules]]
        id = "app"
        name = "App"
        tier = "hard"
        "#,
    );
    assert!(app_without_matchers.is_err());

    let app_with_allowance = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "daily"
        daily_minutes = 15

        [[app_rules]]
        id = "app"
        name = "App"
        tier = "controlled_access"
        allowance_id = "daily"
        matchers = [
          { kind = "command_name", value = "app" }
        ]
        "#,
    )
    .expect("controlled app allowances should parse");
    assert_eq!(
        app_with_allowance.app_rules[0].allowance_id.as_deref(),
        Some("daily")
    );

    let hard_app_with_allowance = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "daily"
        daily_minutes = 15

        [[app_rules]]
        id = "app"
        name = "App"
        tier = "hard"
        allowance_id = "daily"
        matchers = [
          { kind = "command_name", value = "app" }
        ]
        "#,
    );
    assert!(hard_app_with_allowance.is_err());

    let shared_allowance = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "daily"
        daily_minutes = 15

        [[rules]]
        id = "first"
        name = "First"
        tier = "controlled_access"
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "first.example", match_subdomains = false }
        ]

        [[rules]]
        id = "second"
        name = "Second"
        tier = "controlled_access"
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "second.example", match_subdomains = false }
        ]
        "#,
    );
    assert!(shared_allowance.is_err());

    let zero_strict_grace = Config::from_toml_str(
        r#"
        [strict_mode]
        grace_seconds = 0
        "#,
    );
    assert!(zero_strict_grace.is_err());
}

#[test]
fn visit_lifecycle_records_start_heartbeat_and_end() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "daily"
        daily_minutes = 15

        [[rules]]
        id = "visit-rule"
        name = "Visit rule"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "visit.example", match_subdomains = false }
        ]

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let start_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    let visit = record_visit_start("https://visit.example/page", "tab-7", &start_ctx)
        .expect("visit should start");

    assert_eq!(visit.rule_id.as_deref(), Some("visit-rule"));

    let heartbeat_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 5));
    record_visit_heartbeat(visit.id, &heartbeat_ctx).expect("heartbeat should update");

    let end_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 16));
    record_visit_end(visit.id, &end_ctx).expect("visit should end");

    assert_eq!(
        evaluate_url("https://visit.example/page", &end_ctx),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "visit-rule".to_string(),
            rule_name: "Visit rule".to_string(),
            reason: ControlledBlockReason::AllowanceExhausted,
        })
    );
}
