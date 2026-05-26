use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use focus_core::{
    evaluate_url, record_visit_end, record_visit_heartbeat, record_visit_start, request_unlock,
    BlockReason, Config, ControlledBlockReason, Database, Decision, Error, EvaluationContext,
    UnlockError,
};

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
        name = "Late hard block"
        tier = "hard"
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
fn unlocks_allow_controlled_access_then_expire_and_enforce_cooldown() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "controlled-no-allowance"
        name = "Controlled without allowance"
        tier = "controlled_access"
        patterns = [
          { kind = "domain", value = "focus.example", match_subdomains = false }
        ]

        [rules.unlock_policy]
        max_session_minutes = 5
        cooldown_minutes = 30
        max_unlocks_per_hour = 2
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

    let too_long = request_unlock(
        "https://focus.example/",
        6,
        "Need access".to_string(),
        &before_unlock,
    )
    .expect_err("oversized unlock should be denied");
    assert!(matches!(
        too_long,
        Error::Unlock(UnlockError::ExceedsMaxSession {
            requested_minutes: 6,
            max_minutes: 5
        })
    ));

    let unlock = request_unlock(
        "https://focus.example/",
        5,
        "Need access for a task".to_string(),
        &before_unlock,
    )
    .expect("unlock should be granted");
    assert_eq!(unlock.rule_id, "controlled-no-allowance");

    let during_unlock = context(&config, &database, at_utc(2026, 5, 18, 10, 1));
    assert_eq!(
        evaluate_url("https://focus.example/", &during_unlock),
        Decision::Allow
    );

    let duplicate = request_unlock(
        "controlled-no-allowance",
        5,
        "Still active".to_string(),
        &during_unlock,
    )
    .expect_err("active unlock should prevent a duplicate");
    assert!(matches!(
        duplicate,
        Error::Unlock(UnlockError::UnlockAlreadyActive { .. })
    ));

    let cooldown_ctx = context(&config, &database, at_utc(2026, 5, 18, 10, 6));
    let cooldown = request_unlock(
        "controlled-no-allowance",
        5,
        "Try again".to_string(),
        &cooldown_ctx,
    )
    .expect_err("cooldown should block after unlock expiry");
    assert!(matches!(
        cooldown,
        Error::Unlock(UnlockError::CooldownActive { .. })
    ));

    assert!(evaluate_url("https://focus.example/", &cooldown_ctx).is_block());
}

#[test]
fn hourly_unlock_quota_blocks_rapid_successive_unlocks() {
    let config = Config::from_toml_str(
        r#"
        [[rules]]
        id = "quota-controlled"
        name = "Quota controlled"
        tier = "controlled_access"
        patterns = [
          { kind = "domain", value = "quota.example", match_subdomains = false }
        ]

        [rules.unlock_policy]
        max_session_minutes = 5
        cooldown_minutes = 0
        max_unlocks_per_hour = 2
        "#,
    )
    .expect("config should parse");
    let database = Database::in_memory().expect("database should initialize");

    let first = context(&config, &database, at_utc(2026, 5, 18, 10, 0));
    request_unlock("quota-controlled", 5, "first".to_string(), &first)
        .expect("first unlock should pass");

    let second = context(&config, &database, at_utc(2026, 5, 18, 10, 5));
    request_unlock("quota-controlled", 5, "second".to_string(), &second)
        .expect("second unlock should pass");

    let third = context(&config, &database, at_utc(2026, 5, 18, 10, 10));
    let denied = request_unlock("quota-controlled", 5, "third".to_string(), &third)
        .expect_err("third unlock in an hour should be denied");
    assert!(matches!(
        denied,
        Error::Unlock(UnlockError::HourlyQuotaExceeded {
            rule_id,
            limit: 2
        }) if rule_id == "quota-controlled"
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
        "rules",
        "rule_patterns",
        "apps",
        "schedules",
        "allowances",
        "unlocks",
        "visits",
        "events",
        "heartbeats",
        "service_state",
        "policy_defaults",
        "policy_allowances",
        "policy_schedules",
        "policy_schedule_windows",
        "policy_site_lists",
        "policy_site_list_patterns",
        "policy_site_list_schedules",
    ] {
        assert!(
            tables.iter().any(|table| table == required),
            "missing required table {required}; present tables: {tables:?}"
        );
    }

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
fn policy_config_roundtrips_through_sqlite() {
    let config = Config::from_toml_str(
        r#"
        [defaults.unlock_policy]
        max_session_minutes = 8
        cooldown_minutes = 20
        max_unlocks_per_hour = 3

        [[allowances]]
        id = "daily"
        name = "Daily allowance"
        daily_minutes = 15

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

        [rules.unlock_policy]
        max_session_minutes = 4
        cooldown_minutes = 5
        max_unlocks_per_hour = 2
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
        allowance_id = "daily"
        patterns = [
          { kind = "domain", value = "visit.example", match_subdomains = false }
        ]
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
