use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use focus_core::{
    evaluate_app, evaluate_url, metered_app_rule_ids_for_process, migrate_database,
    record_visit_end, record_visit_heartbeat, record_visit_start, request_unlock,
    site_usage_is_metered, BlockReason, Config, ControlledBlockReason, Database, Decision,
    DetoxSession, Error, EvaluationContext, ListMode, ProcessIdentity, UnlockError,
};
use rusqlite::Connection;

fn at_utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(0)
        .expect("zero offset is valid")
        .with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .expect("test timestamp is valid")
}

fn at_offset(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    offset_seconds: i32,
) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(offset_seconds)
        .expect("test offset is valid")
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
fn active_tier2_website_allowlists_are_intersected_and_blocklists_still_win() {
    let config = Config::from_toml_str(
        r#"
        [[schedules]]
        id = "work-hours"
        windows = [{ weekday = "mon", start = "09:00", end = "17:00" }]

        [[rules]]
        id = "work-sites"
        name = "Work sites"
        tier = "scheduled_block"
        mode = "allowlist"
        schedule_ids = ["work-hours"]
        patterns = [
          { kind = "domain", value = "example.com", match_subdomains = true }
        ]

        [[rules]]
        id = "documentation-only"
        name = "Documentation only"
        tier = "scheduled_block"
        mode = "allowlist"
        schedule_ids = ["work-hours"]
        patterns = [
          { kind = "url_prefix", value = "https://docs.example.com/" }
        ]

        [[rules]]
        id = "private-docs"
        name = "Private docs"
        tier = "hard"
        patterns = [
          { kind = "path_prefix", value = "docs.example.com/private" }
        ]
        "#,
    )
    .expect("allowlist config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    assert_eq!(
        evaluate_url("https://docs.example.com/public", &ctx),
        Decision::Allow
    );
    assert!(matches!(
        evaluate_url("https://git.example.com/", &ctx),
        Decision::Block(BlockReason::ScheduledBlock { rule_id, .. })
            if rule_id == "documentation-only"
    ));
    assert!(matches!(
        evaluate_url("https://outside.example.org/", &ctx),
        Decision::Block(BlockReason::ScheduledBlock { rule_id, .. })
            if rule_id == "work-sites"
    ));
    assert!(matches!(
        evaluate_url("https://docs.example.com/private/report", &ctx),
        Decision::Block(BlockReason::HardBlock { rule_id, .. })
            if rule_id == "private-docs"
    ));
}

#[test]
fn disabled_website_allowlists_are_rejected() {
    let error = Config::from_toml_str(
        r#"
        [[rules]]
        id = "disabled-allowlist"
        name = "Disabled allowlist"
        tier = "scheduled_block"
        enabled = false
        mode = "allowlist"
        patterns = [
          { kind = "domain", value = "allowed.example", match_subdomains = true }
        ]
        "#,
    )
    .expect_err("disabled allowlist should be rejected");
    assert!(error.to_string().contains("cannot be disabled"));
}

#[test]
fn tier2_website_allowlists_follow_schedules_and_detox() {
    let config = Config::from_toml_str(
        r#"
        [[schedules]]
        id = "work-hours"

        [[schedules.windows]]
        weekday = "mon"
        start = "09:00"
        end = "17:00"

        [[rules]]
        id = "scheduled-work-sites"
        name = "Scheduled work sites"
        tier = "scheduled_block"
        mode = "allowlist"
        schedule_ids = ["work-hours"]
        patterns = [
          { kind = "domain", value = "work.example", match_subdomains = true }
        ]

        [[rules]]
        id = "detox-work-sites"
        name = "Detox work sites"
        tier = "scheduled_block"
        mode = "allowlist"
        patterns = [
          { kind = "domain", value = "detox.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("scheduled allowlists should parse");
    let database = Database::in_memory().expect("database should initialize");

    let during_schedule = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert_eq!(
        evaluate_url("https://work.example/", &during_schedule),
        Decision::Allow
    );
    assert!(matches!(
        evaluate_url("https://outside.example/", &during_schedule),
        Decision::Block(BlockReason::ScheduledBlock { rule_id, .. })
            if rule_id == "scheduled-work-sites"
    ));

    let after_schedule = context(&config, &database, at_utc(2026, 5, 18, 18, 0));
    assert_eq!(
        evaluate_url("https://outside.example/", &after_schedule),
        Decision::Allow
    );

    let starts_at = at_utc(2026, 5, 18, 18, 0).with_timezone(&Utc);
    database
        .insert_detox_session(&DetoxSession {
            id: "allowlist-detox".to_string(),
            name: Some("Allowlist Detox".to_string()),
            starts_at,
            ends_at: starts_at + chrono::Duration::minutes(30),
            cancelled_at: None,
            site_rule_ids: vec!["detox-work-sites".to_string()],
            app_rule_ids: Vec::new(),
        })
        .expect("detox should insert");
    let during_detox = context(&config, &database, at_utc(2026, 5, 18, 18, 10));
    assert_eq!(
        evaluate_url("https://detox.example/", &during_detox),
        Decision::Allow
    );
    assert!(matches!(
        evaluate_url("https://outside.example/", &during_detox),
        Decision::Block(BlockReason::Detox { rule_id, .. })
            if rule_id == "detox-work-sites"
    ));
}

#[test]
fn tier3_website_allowlists_meter_outside_sites_and_support_unlocks() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "outside-daily"
        daily_minutes = 1

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"

        [[rules]]
        id = "focus-only"
        name = "Focus only"
        tier = "controlled_access"
        mode = "allowlist"
        allowance_id = "outside-daily"
        schedule_ids = ["always"]
        patterns = [
          { kind = "domain", value = "allowed.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("Tier 3 allowlist should parse");
    let database = Database::in_memory().expect("database should initialize");
    let start = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    assert_eq!(
        evaluate_url("https://allowed.example/", &start),
        Decision::Allow
    );
    assert!(!site_usage_is_metered("https://allowed.example/", &start));
    assert_eq!(
        evaluate_url("https://outside.example/", &start),
        Decision::Allow
    );
    assert!(site_usage_is_metered("https://outside.example/", &start));

    let visit = record_visit_start("https://outside.example/", "tab-tier3", &start)
        .expect("outside visit should start");
    assert_eq!(visit.rule_id.as_deref(), Some("focus-only"));
    let exhausted = context(&config, &database, at_utc(2026, 5, 18, 10, 2));
    record_visit_heartbeat(visit.id, &exhausted).expect("outside visit should be metered");

    assert_eq!(
        evaluate_url("https://outside.example/", &exhausted),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "focus-only".to_string(),
            rule_name: "Focus only".to_string(),
            reason: ControlledBlockReason::AllowanceExhausted,
        })
    );
    assert_eq!(
        evaluate_url("https://allowed.example/", &exhausted),
        Decision::Allow
    );

    let unlock = request_unlock(
        "https://outside.example/",
        "I need temporary access to finish this specific assigned work task".to_string(),
        &exhausted,
    )
    .expect("non-allowlisted site should support a Tier 3 unlock");
    assert_eq!(unlock.rule_id, "focus-only");
    assert_eq!(
        evaluate_url("https://outside.example/", &exhausted),
        Decision::Allow
    );
}

#[test]
fn tier3_website_allowlists_keep_allowance_and_unlocks_during_detox() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "detox-outside-daily"
        daily_minutes = 1

        [[rules]]
        id = "detox-focus-only"
        name = "Detox focus only"
        tier = "controlled_access"
        mode = "allowlist"
        allowance_id = "detox-outside-daily"
        patterns = [
          { kind = "domain", value = "allowed.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("Tier 3 Detox allowlist should parse");
    let database = Database::in_memory().expect("database should initialize");
    let starts_at = at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc);
    database
        .insert_detox_session(&DetoxSession {
            id: "tier3-allowlist-detox".to_string(),
            name: Some("Tier 3 allowlist".to_string()),
            starts_at,
            ends_at: starts_at + chrono::Duration::hours(2),
            cancelled_at: None,
            site_rule_ids: vec!["detox-focus-only".to_string()],
            app_rule_ids: Vec::new(),
        })
        .expect("Detox should start");

    let start = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert_eq!(
        evaluate_url("https://outside.example/", &start),
        Decision::Allow
    );
    assert!(site_usage_is_metered("https://outside.example/", &start));
    let visit = record_visit_start("https://outside.example/", "tab-detox-tier3", &start)
        .expect("outside Detox visit should start");
    let exhausted = context(&config, &database, at_utc(2026, 5, 18, 10, 2));
    record_visit_heartbeat(visit.id, &exhausted).expect("Detox visit should be metered");
    assert!(matches!(
        evaluate_url("https://outside.example/", &exhausted),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id,
            reason: ControlledBlockReason::AllowanceExhausted,
            ..
        }) if rule_id == "detox-focus-only"
    ));

    request_unlock(
        "https://outside.example/",
        "I need temporary access to finish this specific Detox work task".to_string(),
        &exhausted,
    )
    .expect("Tier 3 allowlist should retain manual unlock during Detox");
    assert_eq!(
        evaluate_url("https://outside.example/", &exhausted),
        Decision::Allow
    );
}

#[test]
fn unsupported_allowlist_combinations_are_rejected() {
    let tier1 = Config::from_toml_str(
        r#"
        [[rules]]
        id = "tier1-allowlist"
        name = "Tier 1 allowlist"
        tier = "hard"
        mode = "allowlist"
        patterns = [
          { kind = "domain", value = "allowed.example", match_subdomains = true }
        ]
        "#,
    )
    .expect_err("Tier 1 website allowlists should be rejected");
    assert!(tier1.to_string().contains("must use Tier 2 or Tier 3"));

    let app = Config::from_toml_str(
        r#"
        [[app_rules]]
        id = "app-allowlist"
        name = "Application allowlist"
        tier = "hard"
        mode = "allowlist"
        matchers = [
          { kind = "desktop_id", value = "org.example.Editor.desktop" }
        ]
        "#,
    )
    .expect_err("Tier 1 application allowlists should be rejected");
    assert!(app.to_string().contains("must use Tier 2 or Tier 3"));

    for tier in ["scheduled_block", "controlled_access"] {
        Config::from_toml_str(&format!(
            r#"
            [[app_rules]]
            id = "app-allowlist"
            name = "Application allowlist"
            tier = "{tier}"
            mode = "allowlist"
            matchers = [
              {{ kind = "desktop_id", value = "org.example.Editor.desktop" }}
            ]
            "#
        ))
        .expect("Tier 2 and Tier 3 application allowlists should parse");
    }
}

#[test]
fn legacy_recovery_normalizes_website_allowlists_to_enabled_tier2() {
    let config = Config::from_legacy_recovery_toml_str(
        r#"
        [[rules]]
        id = "legacy-allowlist"
        name = "Legacy allowlist"
        tier = "hard"
        enabled = false
        mode = "allowlist"
        patterns = [
          { kind = "domain", value = "allowed.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("legacy recovery allowlist should normalize");

    assert_eq!(config.rules[0].tier, focus_core::RuleTier::ScheduledBlock);
    assert!(config.rules[0].enabled);
    assert_eq!(config.rules[0].mode, ListMode::Allowlist);
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
fn tier2_application_allowlists_invert_matching_only_while_active() {
    let config = Config::from_toml_str(
        r#"
        [[schedules]]
        id = "work-hours"
        windows = [{ weekday = "mon", start = "09:00", end = "17:00" }]

        [[app_rules]]
        id = "work-apps"
        name = "Work applications"
        tier = "scheduled_block"
        mode = "allowlist"
        schedule_ids = ["work-hours"]
        matchers = [
          { kind = "desktop_id", value = "org.example.Editor.desktop" }
        ]
        "#,
    )
    .expect("Tier 2 application allowlist should parse");
    let database = Database::in_memory().expect("database should initialize");
    let editor = ProcessIdentity {
        pid: Some(100),
        executable_path: Some("/usr/bin/editor".to_string()),
        executable_basename: Some("editor".to_string()),
        command_name: Some("editor".to_string()),
        desktop_id: Some("org.example.Editor.desktop".to_string()),
        window_titles: vec!["Editor".to_string()],
    };
    let game = ProcessIdentity {
        pid: Some(200),
        executable_path: Some("/usr/bin/game".to_string()),
        executable_basename: Some("game".to_string()),
        command_name: Some("game".to_string()),
        desktop_id: Some("org.example.Game.desktop".to_string()),
        window_titles: vec!["Game".to_string()],
    };

    let active = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert_eq!(evaluate_app(&editor, &active), Decision::Allow);
    assert!(matches!(
        evaluate_app(&game, &active),
        Decision::Block(BlockReason::ScheduledBlock { rule_id, .. }) if rule_id == "work-apps"
    ));

    let inactive = context(&config, &database, at_utc(2026, 5, 18, 18, 0));
    assert_eq!(evaluate_app(&game, &inactive), Decision::Allow);
}

#[test]
fn tier2_application_allowlists_activate_through_detox() {
    let config = Config::from_toml_str(
        r#"
        [[app_rules]]
        id = "detox-apps"
        name = "Detox applications"
        tier = "scheduled_block"
        mode = "allowlist"
        matchers = [
          { kind = "command_name", value = "editor" }
        ]
        "#,
    )
    .expect("Tier 2 application allowlist should parse");
    let database = Database::in_memory().expect("database should initialize");
    let starts_at = at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc);
    database
        .insert_detox_session(&DetoxSession {
            id: "app-allowlist-detox".to_string(),
            name: Some("Work apps only".to_string()),
            starts_at,
            ends_at: starts_at + chrono::Duration::hours(1),
            cancelled_at: None,
            site_rule_ids: Vec::new(),
            app_rule_ids: vec!["detox-apps".to_string()],
        })
        .expect("Detox should insert");
    let editor = ProcessIdentity {
        pid: Some(100),
        executable_path: None,
        executable_basename: None,
        command_name: Some("editor".to_string()),
        desktop_id: None,
        window_titles: Vec::new(),
    };
    let game = ProcessIdentity {
        pid: Some(200),
        executable_path: None,
        executable_basename: None,
        command_name: Some("game".to_string()),
        desktop_id: None,
        window_titles: Vec::new(),
    };
    let during = context(&config, &database, at_utc(2026, 5, 18, 10, 1));

    assert_eq!(evaluate_app(&editor, &during), Decision::Allow);
    assert!(matches!(
        evaluate_app(&game, &during),
        Decision::Block(BlockReason::Detox { rule_id, .. }) if rule_id == "detox-apps"
    ));
}

#[test]
fn tier3_application_allowlists_meter_outside_apps_and_support_manual_unlock() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "outside-daily"
        daily_minutes = 1

        [[schedules]]
        id = "always"
        windows = [{ weekday = "everyday", start = "00:00", end = "23:59" }]

        [[app_rules]]
        id = "focus-apps"
        name = "Focus applications"
        tier = "controlled_access"
        mode = "allowlist"
        allowance_id = "outside-daily"
        schedule_ids = ["always"]
        matchers = [
          { kind = "desktop_id", value = "org.example.Editor.desktop" }
        ]
        "#,
    )
    .expect("Tier 3 application allowlist should parse");
    let database = Database::in_memory().expect("database should initialize");
    let editor = ProcessIdentity {
        pid: Some(100),
        executable_path: None,
        executable_basename: None,
        command_name: Some("editor".to_string()),
        desktop_id: Some("org.example.Editor.desktop".to_string()),
        window_titles: Vec::new(),
    };
    let game = ProcessIdentity {
        pid: Some(200),
        executable_path: None,
        executable_basename: None,
        command_name: Some("game-bin".to_string()),
        desktop_id: Some("org.example.Game.desktop".to_string()),
        window_titles: Vec::new(),
    };
    let before_usage = context(&config, &database, at_utc(2026, 5, 18, 10, 0));

    assert_eq!(evaluate_app(&editor, &before_usage), Decision::Allow);
    assert_eq!(evaluate_app(&game, &before_usage), Decision::Allow);
    assert!(metered_app_rule_ids_for_process(&editor, &before_usage)
        .expect("allowed app metering should evaluate")
        .is_empty());
    assert_eq!(
        metered_app_rule_ids_for_process(&game, &before_usage)
            .expect("outside app metering should evaluate"),
        vec!["focus-apps".to_string()]
    );

    database
        .insert_app_usage_interval(
            "focus-apps",
            at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc),
            at_utc(2026, 5, 18, 10, 1).with_timezone(&Utc),
        )
        .expect("outside app usage should insert");
    let exhausted = context(&config, &database, at_utc(2026, 5, 18, 10, 1));
    assert!(matches!(
        evaluate_app(&game, &exhausted),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id,
            reason: ControlledBlockReason::AllowanceExhausted,
            ..
        }) if rule_id == "focus-apps"
    ));

    let unlock = request_unlock(
        "game-bin",
        "I need this application to complete a specific scheduled work task".to_string(),
        &exhausted,
    )
    .expect("outside application should be unlockable");
    assert_eq!(unlock.rule_id, "focus-apps");
    let during_unlock = context(&config, &database, at_utc(2026, 5, 18, 10, 2));
    assert_eq!(evaluate_app(&game, &during_unlock), Decision::Allow);
}

#[test]
fn detox_sessions_block_site_rules_until_the_absolute_end_time() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "video"
        name = "Video"
        tier = "scheduled_block"
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
fn tier3_detox_activates_rules_and_allows_manual_unlocks() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "social"
        name = "Social"
        tier = "controlled_access"
        patterns = [
          { kind = "domain", value = "social.example", match_subdomains = false }
        ]

        [[allowances]]
        id = "game-daily"
        daily_minutes = 30

        [[app_rules]]
        id = "game"
        name = "Game"
        tier = "controlled_access"
        allowance_id = "game-daily"
        matchers = [
          { kind = "command_name", value = "game-bin" }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let starts_at = at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc);
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
        Decision::Block(BlockReason::ControlledAccess {
            rule_id,
            reason: ControlledBlockReason::NoAllowance,
            ..
        }) if rule_id == "social"
    ));
    request_unlock(
        "https://social.example/",
        "I need temporary access to complete this scheduled work item".to_string(),
        &during,
    )
    .expect("Tier 3 should allow an unlock while Detox is active");
    assert_eq!(
        evaluate_url("https://social.example/", &during),
        Decision::Allow
    );

    let process = ProcessIdentity {
        pid: Some(1234),
        executable_path: None,
        executable_basename: None,
        command_name: Some("game-bin".to_string()),
        desktop_id: None,
        window_titles: Vec::new(),
    };
    let before_detox_app = context(&config, &database, at_utc(2026, 5, 18, 9, 59));
    assert_eq!(evaluate_app(&process, &before_detox_app), Decision::Allow);

    let during_app_detox = context(&config, &database, at_utc(2026, 5, 18, 10, 1));
    assert_eq!(evaluate_app(&process, &during_app_detox), Decision::Allow);
}

#[test]
fn tier3_detox_uses_and_accounts_for_the_daily_allowance() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "detox-daily"
        daily_minutes = 1

        [[rules]]
        id = "video"
        name = "Video"
        tier = "controlled_access"
        allowance_id = "detox-daily"
        patterns = [
          { kind = "domain", value = "video.example", match_subdomains = true }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");
    let starts_at = at_utc(2026, 5, 18, 10, 0).with_timezone(&Utc);
    database
        .insert_detox_session(&DetoxSession {
            id: "detox-allowance".to_string(),
            name: None,
            starts_at,
            ends_at: starts_at + chrono::Duration::hours(2),
            cancelled_at: None,
            site_rule_ids: vec!["video".to_string()],
            app_rule_ids: Vec::new(),
        })
        .expect("detox session should insert");

    let start_context = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    assert_eq!(
        evaluate_url("https://video.example/", &start_context),
        Decision::Allow
    );
    let visit = record_visit_start("https://video.example/", "tab-detox", &start_context)
        .expect("visit should start");
    record_visit_heartbeat(
        visit.id,
        &context(&config, &database, at_utc(2026, 5, 18, 10, 2)),
    )
    .expect("visit heartbeat should record");
    record_visit_end(
        visit.id,
        &context(&config, &database, at_utc(2026, 5, 18, 10, 2)),
    )
    .expect("visit should end");

    assert!(matches!(
        evaluate_url(
            "https://video.example/",
            &context(&config, &database, at_utc(2026, 5, 18, 10, 2)),
        ),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id,
            reason: ControlledBlockReason::AllowanceExhausted,
            ..
        }) if rule_id == "video"
    ));
}

#[test]
fn tier2_detox_rejects_manual_unlocks() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "social"
        name = "Social"
        tier = "scheduled_block"
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

    let during = context(&config, &database, at_utc(2026, 5, 18, 10, 5));
    let denied = request_unlock(
        "https://social.example/",
        "I need temporary access to complete this specific work item".to_string(),
        &during,
    )
    .expect_err("Tier 2 Detox should reject manual unlock");
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
}

#[test]
fn tier2_blocks_only_while_scheduled_and_cannot_be_unlocked() {
    let config = Config::from_toml_str(
        r#"
        [[schedules]]
        id = "work"

        [[schedules.windows]]
        weekday = "mon"
        start = "09:00"
        end = "17:00"

        [[rules]]
        id = "strict"
        name = "Strict"
        tier = "scheduled_block"
        schedule_ids = ["work"]
        patterns = [{ kind = "domain", value = "strict.example" }]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    assert_eq!(
        evaluate_url(
            "https://strict.example/",
            &context(&config, &database, at_utc(2026, 5, 18, 8, 59)),
        ),
        Decision::Allow
    );
    let active = context(&config, &database, at_utc(2026, 5, 18, 9, 0));
    assert!(matches!(
        evaluate_url("https://strict.example/", &active),
        Decision::Block(BlockReason::ScheduledBlock { rule_id, .. }) if rule_id == "strict"
    ));
    assert!(matches!(
        request_unlock(
            "https://strict.example/",
            "I need temporary access to complete this particular assigned task".to_string(),
            &active,
        ),
        Err(Error::Unlock(UnlockError::TargetIsScheduledBlocked { rule_id }))
            if rule_id == "strict"
    ));
    assert_eq!(
        evaluate_url(
            "https://strict.example/",
            &context(&config, &database, at_utc(2026, 5, 18, 17, 0)),
        ),
        Decision::Allow
    );
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
        Decision::Block(BlockReason::ControlledAccess {
            rule_id,
            reason: ControlledBlockReason::UnlockRequired,
            ..
        }) if rule_id == "detox-target"
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
    record_visit_heartbeat(visit.id, &exhausted_ctx).expect("heartbeat should record");
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

    let policy_site_lists_sql: String = database
        .connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'policy_site_lists'",
            [],
            |row| row.get(0),
        )
        .expect("policy_site_lists schema should query");
    assert!(policy_site_lists_sql.contains("'allowlist'"));

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
fn database_migration_adds_scheduled_block_tier_without_relabeling_existing_rules() {
    let conn = Connection::open_in_memory().expect("database should open");
    conn.execute_batch(
        r#"
        CREATE TABLE policy_site_lists (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'controlled_access')),
            enabled INTEGER NOT NULL DEFAULT 1,
            allowance_id TEXT,
            max_session_minutes INTEGER,
            cooldown_minutes INTEGER,
            max_unlocks_per_hour INTEGER
        );

        CREATE TABLE policy_app_rules (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'controlled_access')),
            enabled INTEGER NOT NULL DEFAULT 1,
            allowance_id TEXT,
            max_session_minutes INTEGER,
            cooldown_minutes INTEGER,
            max_unlocks_per_hour INTEGER
        );

        INSERT INTO policy_site_lists (id, name, tier, enabled)
        VALUES ('existing-flexible', 'Existing flexible', 'controlled_access', 1);
        INSERT INTO policy_app_rules (id, name, tier, enabled)
        VALUES ('existing-app', 'Existing app', 'hard', 1);
        "#,
    )
    .expect("legacy tier schema should initialize");

    migrate_database(&conn).expect("database should migrate");

    conn.execute(
        "INSERT INTO policy_site_lists (id, name, tier, enabled) VALUES (?1, ?2, ?3, 1)",
        ("new-strict", "New strict", "scheduled_block"),
    )
    .expect("scheduled site tier should satisfy migrated constraint");
    conn.execute(
        "INSERT INTO policy_app_rules (id, name, tier, enabled) VALUES (?1, ?2, ?3, 1)",
        ("new-strict-app", "New strict app", "scheduled_block"),
    )
    .expect("scheduled app tier should satisfy migrated constraint");

    let existing_tier: String = conn
        .query_row(
            "SELECT tier FROM policy_site_lists WHERE id = 'existing-flexible'",
            [],
            |row| row.get(0),
        )
        .expect("existing site tier should remain");
    assert_eq!(existing_tier, "controlled_access");

    let existing_site_mode: String = conn
        .query_row(
            "SELECT mode FROM policy_site_lists WHERE id = 'existing-flexible'",
            [],
            |row| row.get(0),
        )
        .expect("existing site mode should default");
    let existing_app_mode: String = conn
        .query_row(
            "SELECT mode FROM policy_app_rules WHERE id = 'existing-app'",
            [],
            |row| row.get(0),
        )
        .expect("existing app mode should default");
    assert_eq!(existing_site_mode, "blocklist");
    assert_eq!(existing_app_mode, "blocklist");

    conn.execute(
        "UPDATE policy_site_lists SET tier = 'hard', enabled = 0, mode = 'allowlist' WHERE id = 'new-strict'",
        [],
    )
    .expect("legacy allowlist state should insert");
    migrate_database(&conn).expect("allowlist state should normalize");
    let (normalized_tier, normalized_enabled): (String, i64) = conn
        .query_row(
            "SELECT tier, enabled FROM policy_site_lists WHERE id = 'new-strict'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("normalized allowlist should query");
    assert_eq!(normalized_tier, "scheduled_block");
    assert_eq!(normalized_enabled, 1);

    conn.execute(
        "INSERT INTO policy_allowances (id, name, daily_minutes) VALUES ('tier3-daily', 'Tier 3 daily', 15)",
        [],
    )
    .expect("Tier 3 allowance should insert");
    conn.execute(
        "UPDATE policy_site_lists SET tier = 'controlled_access', mode = 'allowlist', allowance_id = 'tier3-daily' WHERE id = 'new-strict'",
        [],
    )
    .expect("Tier 3 allowlist should insert");
    migrate_database(&conn).expect("Tier 3 allowlist should survive database normalization");
    let (preserved_tier, preserved_allowance): (String, Option<String>) = conn
        .query_row(
            "SELECT tier, allowance_id FROM policy_site_lists WHERE id = 'new-strict'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("Tier 3 allowlist should query");
    assert_eq!(preserved_tier, "controlled_access");
    assert_eq!(preserved_allowance.as_deref(), Some("tier3-daily"));
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
        id = "allowed-list"
        name = "Allowed List"
        tier = "scheduled_block"
        mode = "allowlist"
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
        mode = "allowlist"
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
    assert_eq!(loaded.rules[0].mode, ListMode::Allowlist);
    assert_eq!(loaded.app_rules[0].mode, ListMode::Allowlist);
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
        id = "allowed-list"
        name = "Allowed List"
        tier = "scheduled_block"
        mode = "allowlist"
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
    assert!(exported.contains("mode = \"allowlist\""));
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
fn site_allowances_charge_only_confirmed_time_inside_schedule_windows() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "daily"
        daily_minutes = 30

        [[schedules]]
        id = "split-workday"

        [[schedules.windows]]
        weekday = "mon"
        start = "09:00"
        end = "10:00"

        [[schedules.windows]]
        weekday = "mon"
        start = "11:00"
        end = "12:00"

        [[rules]]
        id = "scheduled-site"
        name = "Scheduled site"
        tier = "controlled_access"
        schedule_ids = ["split-workday"]
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "scheduled.example", match_subdomains = false }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    let start_ctx = context(&config, &database, at_utc(2026, 5, 18, 9, 45));
    let visit = record_visit_start("https://scheduled.example/", "firefox:7", &start_ctx)
        .expect("visit should start");
    let after_first_window = context(&config, &database, at_utc(2026, 5, 18, 10, 5));
    record_visit_heartbeat(visit.id, &after_first_window).expect("heartbeat should record");

    let second_window_start = context(&config, &database, at_utc(2026, 5, 18, 11, 0));
    assert_eq!(
        evaluate_url("https://scheduled.example/", &second_window_start),
        Decision::Allow
    );

    let exhausted_at = context(&config, &database, at_utc(2026, 5, 18, 11, 15));
    record_visit_heartbeat(visit.id, &exhausted_at).expect("heartbeat should record");
    assert_eq!(
        evaluate_url("https://scheduled.example/", &exhausted_at),
        Decision::Block(BlockReason::ControlledAccess {
            rule_id: "scheduled-site".to_string(),
            rule_name: "Scheduled site".to_string(),
            reason: ControlledBlockReason::AllowanceExhausted,
        })
    );
}

#[test]
fn stale_site_visits_stop_charging_at_the_last_heartbeat() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "daily"
        daily_minutes = 30

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"

        [[rules]]
        id = "stale-site"
        name = "Stale site"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "stale.example", match_subdomains = false }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    let start_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    let visit = record_visit_start("https://stale.example/", "firefox:8", &start_ctx)
        .expect("visit should start");
    let heartbeat_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 10));
    record_visit_heartbeat(visit.id, &heartbeat_ctx).expect("heartbeat should record");

    let after_extension_stops = context(&config, &database, at_utc(2026, 5, 18, 10, 31));
    assert_eq!(
        evaluate_url("https://stale.example/", &after_extension_stops),
        Decision::Allow
    );
}

#[test]
fn site_allowances_reset_at_local_midnight() {
    let config = Config::from_toml_str(
        r#"
        [[allowances]]
        id = "daily"
        daily_minutes = 30

        [[schedules]]
        id = "always"

        [[schedules.windows]]
        weekday = "everyday"
        start = "00:00"
        end = "23:59"

        [[rules]]
        id = "local-day-site"
        name = "Local day site"
        tier = "controlled_access"
        schedule_ids = ["always"]
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "local-day.example", match_subdomains = false }
        ]
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    let start_ctx = context(&config, &database, at_offset(2026, 5, 18, 23, 0, 7_200));
    let visit = record_visit_start("https://local-day.example/", "firefox:9", &start_ctx)
        .expect("visit should start");
    let end_ctx = context(&config, &database, at_offset(2026, 5, 18, 23, 30, 7_200));
    record_visit_end(visit.id, &end_ctx).expect("visit should end");

    assert!(evaluate_url("https://local-day.example/", &end_ctx).is_block());

    let next_local_day = context(&config, &database, at_offset(2026, 5, 19, 0, 5, 7_200));
    assert_eq!(
        evaluate_url("https://local-day.example/", &next_local_day),
        Decision::Allow
    );
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
