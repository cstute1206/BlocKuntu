//! Layer 1 regression cases. Time, policy and process identities are entirely synthetic.
use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use focus_core::{
    evaluate_app, evaluate_url, metered_app_rule_ids_for_process, request_unlock, Config, Database,
    Decision, DetoxSession, EvaluationContext, ProcessIdentity,
};

fn now() -> DateTime<FixedOffset> {
    FixedOffset::east_opt(7200)
        .unwrap()
        .with_ymd_and_hms(2026, 5, 18, 10, 0, 0)
        .unwrap()
}
fn process(name: &str) -> ProcessIdentity {
    ProcessIdentity {
        pid: Some(42),
        command_name: Some(name.into()),
        executable_path: None,
        executable_basename: None,
        desktop_id: None,
        window_titles: vec![],
    }
}
fn config(kind: &str, tier: &str) -> Config {
    let entry = if kind == "rules" {
        "patterns = [{ kind = \"domain\", value = \"work.test\", match_subdomains = false }]"
    } else {
        "matchers = [{ kind = \"command_name\", value = \"editor\" }]"
    };
    Config::from_toml_str(&format!(
        r#"
        [[schedules]]
        id = "work"
        windows = [{{ weekday = "mon", start = "10:00", end = "11:00" }}]
        [[allowances]]
        id = "daily"
        daily_minutes = 1
        [[{kind}]]
        id = "focus"
        name = "Focus"
        mode = "allowlist"
        tier = "{tier}"
        schedule_ids = ["work"]
        {}
        {entry}
    "#,
        if tier == "controlled_access" {
            "allowance_id = \"daily\""
        } else {
            ""
        }
    ))
    .unwrap()
}
fn rejected(kind: &str, ctx: &EvaluationContext<'_>) -> Decision {
    if kind == "rules" {
        evaluate_url("https://outside.test/", ctx)
    } else {
        evaluate_app(&process("game"), ctx)
    }
}

#[test]
fn pr_wal_002_aal_002_empty_allowlists_are_rejected() {
    for kind in ["rules", "app_rules"] {
        let mut policy = config(kind, "scheduled_block");
        if kind == "rules" {
            policy.rules[0].patterns.clear();
        } else {
            policy.app_rules[0].matchers.clear();
        }
        assert!(policy.validate().is_err(), "{kind}");
    }
}

#[test]
fn pr_pol_004_wal_003_004_aal_003_004_exact_schedule_boundaries() {
    for kind in ["rules", "app_rules"] {
        let policy = config(kind, "scheduled_block");
        let db = Database::in_memory().unwrap();
        for (offset, blocked) in [
            (-1, false),
            (0, true),
            (3599, true),
            (3600, false),
            (86400, false),
        ] {
            let ctx = EvaluationContext::new(&policy, &db, now() + Duration::seconds(offset));
            assert_eq!(rejected(kind, &ctx).is_block(), blocked, "{kind} {offset}");
        }
    }
}

#[test]
fn pr_pol_005_aal_005_detox_has_exact_absolute_start_and_end() {
    for kind in ["rules", "app_rules"] {
        let mut policy = config(kind, "scheduled_block");
        if kind == "rules" {
            policy.rules[0].schedule_ids.clear();
        } else {
            policy.app_rules[0].schedule_ids.clear();
        }
        let db = Database::in_memory().unwrap();
        db.insert_detox_session(&DetoxSession {
            id: "detox".into(),
            name: None,
            starts_at: now().with_timezone(&Utc),
            ends_at: (now() + Duration::minutes(1)).with_timezone(&Utc),
            cancelled_at: None,
            site_rule_ids: if kind == "rules" {
                vec!["focus".into()]
            } else {
                vec![]
            },
            app_rule_ids: if kind == "app_rules" {
                vec!["focus".into()]
            } else {
                vec![]
            },
        })
        .unwrap();
        for (offset, blocked) in [(-1, false), (0, true), (59, true), (60, false)] {
            assert_eq!(
                rejected(
                    kind,
                    &EvaluationContext::new(&policy, &db, now() + Duration::seconds(offset))
                )
                .is_block(),
                blocked,
                "{kind} {offset}"
            );
        }
    }
}

#[test]
fn pr_aal_015_016_018_detox_unlock_by_id_or_identity_expires_without_refilling() {
    for target in ["focus", "game"] {
        let mut policy = config("app_rules", "controlled_access");
        policy.app_rules[0].schedule_ids.clear();
        let db = Database::in_memory().unwrap();
        db.insert_detox_session(&DetoxSession {
            id: "detox".into(),
            name: None,
            starts_at: now().with_timezone(&Utc),
            ends_at: (now() + Duration::hours(2)).with_timezone(&Utc),
            cancelled_at: None,
            site_rule_ids: vec![],
            app_rule_ids: vec!["focus".into()],
        })
        .unwrap();
        db.insert_app_usage_interval(
            "focus",
            now().with_timezone(&Utc),
            (now() + Duration::minutes(1)).with_timezone(&Utc),
        )
        .unwrap();
        let start = now() + Duration::minutes(1);
        let ctx = EvaluationContext::new(&policy, &db, start);
        assert!(evaluate_app(&process("game"), &ctx).is_block());
        let unlock = request_unlock(
            target,
            "I need this application to complete a specific work task".into(),
            &ctx,
        )
        .unwrap();
        assert_eq!(unlock.rule_id, "focus");
        for (seconds, blocked) in [(0, false), (119, false), (120, true), (121, true)] {
            assert_eq!(
                evaluate_app(
                    &process("game"),
                    &EvaluationContext::new(&policy, &db, start + Duration::seconds(seconds))
                )
                .is_block(),
                blocked
            );
        }
        assert!(request_unlock(
            target,
            "Another sufficiently long explanation for the same hour".into(),
            &ctx
        )
        .is_err());
    }
}

#[test]
fn pr_aal_009_010_017_wal_012_unlock_does_not_override_another_restriction() {
    for kind in ["rules", "app_rules"] {
        for other_mode in [
            focus_core::ListMode::Allowlist,
            focus_core::ListMode::Blocklist,
        ] {
            let mut policy = config(kind, "controlled_access");
            policy.allowances[0].daily_minutes = 0;
            let db = Database::in_memory().unwrap();
            let ctx = EvaluationContext::new(&policy, &db, now());
            request_unlock(
                "focus",
                "I need temporary access to complete this specific work task".into(),
                &ctx,
            )
            .unwrap();
            assert_eq!(rejected(kind, &ctx), Decision::Allow);
            if kind == "rules" {
                let mut other = policy.rules[0].clone();
                other.id = "other".into();
                other.mode = other_mode;
                if other_mode == focus_core::ListMode::Blocklist {
                    other.patterns[0].value = "outside.test".into();
                }
                policy.rules.push(other);
            } else {
                let mut other = policy.app_rules[0].clone();
                other.id = "other".into();
                other.mode = other_mode;
                if other_mode == focus_core::ListMode::Blocklist {
                    other.matchers[0].value = "game".into();
                }
                policy.app_rules.push(other);
            }
            assert!(
                rejected(kind, &EvaluationContext::new(&policy, &db, now())).is_block(),
                "{kind} {other_mode:?}"
            );
        }
    }
}

#[test]
fn pr_aal_009_allowlists_require_the_intersection_of_process_identities() {
    let mut policy = config("app_rules", "scheduled_block");
    let mut second = policy.app_rules[0].clone();
    second.id = "second".into();
    second.matchers[0].value = "terminal".into();
    policy.app_rules.push(second);
    let db = Database::in_memory().unwrap();
    for name in ["editor", "terminal", "game"] {
        assert!(
            evaluate_app(&process(name), &EvaluationContext::new(&policy, &db, now())).is_block()
        );
    }
    policy.app_rules[1].matchers[0].value = "editor".into();
    assert_eq!(
        evaluate_app(
            &process("editor"),
            &EvaluationContext::new(&policy, &db, now())
        ),
        Decision::Allow
    );
}

#[test]
fn pr_pol_009_aal_007_matchers_use_only_the_evaluated_process_fields() {
    for (kind, value) in [
        ("executable_path", "/opt/editor"),
        ("executable_basename", "editor"),
        ("command_name", "editor"),
        ("desktop_id", "editor.desktop"),
        ("window_title_contains", "Work document"),
    ] {
        let policy = Config::from_toml_str(&format!(
            r#"
            [[app_rules]]
            id = "one"
            name = "One"
            tier = "hard"
            matchers = [{{ kind = "{kind}", value = "{value}" }}]
        "#
        ))
        .unwrap();
        let db = Database::in_memory().unwrap();
        let ctx = EvaluationContext::new(&policy, &db, now());
        let mut matching = process("unrelated");
        match kind {
            "executable_path" => matching.executable_path = Some(value.into()),
            "executable_basename" => matching.executable_basename = Some(value.into()),
            "command_name" => matching.command_name = Some(value.into()),
            "desktop_id" => matching.desktop_id = Some(value.into()),
            _ => matching.window_titles = vec![format!("{value} - Editor")],
        }
        assert!(evaluate_app(&matching, &ctx).is_block(), "{kind}");
        assert_eq!(evaluate_app(&process("unrelated"), &ctx), Decision::Allow);
        if kind != "command_name" {
            assert_eq!(
                evaluate_app(&process(value), &ctx),
                Decision::Allow,
                "no cross-field match for {kind}"
            );
        }
    }
}

#[test]
fn pr_aal_013_014_metering_depends_on_process_identity_not_windows() {
    let policy = config("app_rules", "controlled_access");
    let db = Database::in_memory().unwrap();
    let ctx = EvaluationContext::new(&policy, &db, now());
    assert!(metered_app_rule_ids_for_process(&process("editor"), &ctx)
        .unwrap()
        .is_empty());
    for titles in [vec![], vec!["Background game".into()]] {
        let mut app = process("game");
        app.window_titles = titles;
        assert_eq!(
            metered_app_rule_ids_for_process(&app, &ctx).unwrap(),
            vec!["focus"]
        );
    }
}

#[test]
fn pr_data_004_both_allowlist_modes_survive_sqlite_and_recovery_toml() {
    let mut policy = config("rules", "controlled_access");
    policy.app_rules = config("app_rules", "scheduled_block").app_rules;
    policy.app_rules[0].id = "apps".into();
    let db = Database::in_memory().unwrap();
    db.replace_policy_config(&policy).unwrap();
    let loaded = db.load_policy_config().unwrap();
    assert_eq!(loaded, policy);
    assert_eq!(
        Config::from_toml_str(&toml::to_string(&loaded).unwrap()).unwrap(),
        policy
    );
}

#[test]
fn pr_pol_001_002_each_url_matcher_has_positive_and_near_miss_inputs() {
    for (pattern, yes, no) in [
        (
            r#"kind = "domain", value = "work.test", match_subdomains = false"#,
            "https://work.test/",
            "https://sub.work.test/",
        ),
        (
            r#"kind = "domain", value = "work.test", match_subdomains = true"#,
            "https://sub.work.test/",
            "https://notwork.test/",
        ),
        (
            r#"kind = "exact_url", value = "https://work.test/path""#,
            "https://work.test/path",
            "https://work.test/path/child",
        ),
        (
            r#"kind = "url_prefix", value = "https://work.test/path/""#,
            "https://work.test/path/child",
            "https://work.test/paths/child",
        ),
        (
            r#"kind = "url_contains", value = "needle""#,
            "https://work.test/?q=needle",
            "https://work.test/?q=needl",
        ),
        (
            r#"kind = "path_prefix", value = "work.test/private/""#,
            "https://work.test/private/file",
            "https://work.test/public/file",
        ),
    ] {
        let policy = Config::from_toml_str(&format!(
            r#"
            [[rules]]
            id = "one"
            name = "One"
            tier = "hard"
            patterns = [{{ {pattern} }}]
        "#
        ))
        .unwrap();
        let db = Database::in_memory().unwrap();
        let ctx = EvaluationContext::new(&policy, &db, now());
        assert!(evaluate_url(yes, &ctx).is_block(), "{pattern}");
        assert_eq!(evaluate_url(no, &ctx), Decision::Allow, "{pattern}");
    }
}

#[test]
fn pr_pol_003_disabled_or_unattached_lists_are_inactive() {
    for kind in ["rules", "app_rules"] {
        for tier in ["scheduled_block", "controlled_access"] {
            let mut policy = config(kind, tier);
            if kind == "rules" {
                policy.rules[0].schedule_ids.clear();
            } else {
                policy.app_rules[0].schedule_ids.clear();
            }
            let db = Database::in_memory().unwrap();
            assert_eq!(
                rejected(kind, &EvaluationContext::new(&policy, &db, now())),
                Decision::Allow
            );
        }
    }
    let mut policy = config("rules", "scheduled_block");
    policy.rules[0].mode = focus_core::ListMode::Blocklist;
    policy.rules[0].enabled = false;
    let db = Database::in_memory().unwrap();
    assert_eq!(
        evaluate_url(
            "https://work.test",
            &EvaluationContext::new(&policy, &db, now())
        ),
        Decision::Allow
    );
}

#[test]
fn pr_pol_006_all_three_tiers_obey_precedence_for_sites_and_apps() {
    for apps in [false, true] {
        let mut policy = config(
            if apps { "app_rules" } else { "rules" },
            "controlled_access",
        );
        policy.allowances[0].daily_minutes = 0;
        if apps {
            policy.app_rules[0].mode = focus_core::ListMode::Blocklist;
            let mut scheduled = policy.app_rules[0].clone();
            scheduled.id = "scheduled".into();
            scheduled.tier = focus_core::RuleTier::ScheduledBlock;
            scheduled.allowance_id = None;
            let mut hard = scheduled.clone();
            hard.id = "hard".into();
            hard.tier = focus_core::RuleTier::Hard;
            policy.app_rules.extend([scheduled, hard]);
        } else {
            policy.rules[0].mode = focus_core::ListMode::Blocklist;
            let mut scheduled = policy.rules[0].clone();
            scheduled.id = "scheduled".into();
            scheduled.tier = focus_core::RuleTier::ScheduledBlock;
            scheduled.allowance_id = None;
            let mut hard = scheduled.clone();
            hard.id = "hard".into();
            hard.tier = focus_core::RuleTier::Hard;
            policy.rules.extend([scheduled, hard]);
        }
        let db = Database::in_memory().unwrap();
        for expected in ["hard", "scheduled", "focus"] {
            let ctx = EvaluationContext::new(&policy, &db, now());
            let decision = if apps {
                evaluate_app(&process("editor"), &ctx)
            } else {
                evaluate_url("https://work.test", &ctx)
            };
            let id = match decision {
                Decision::Block(focus_core::BlockReason::HardBlock { rule_id, .. })
                | Decision::Block(focus_core::BlockReason::ScheduledBlock { rule_id, .. })
                | Decision::Block(focus_core::BlockReason::ControlledAccess { rule_id, .. }) => {
                    rule_id
                }
                other => panic!("unexpected {other:?}"),
            };
            assert_eq!(id, expected);
            if apps {
                policy.app_rules.retain(|rule| rule.id != expected);
            } else {
                policy.rules.retain(|rule| rule.id != expected);
            }
        }
    }
}

#[test]
fn pr_aal_016_rejects_strict_tiers_short_reused_reasons_and_hourly_quota() {
    let reason = "I need this application to complete a specific work assignment";
    for tier in [
        focus_core::RuleTier::Hard,
        focus_core::RuleTier::ScheduledBlock,
    ] {
        let mut policy = config("app_rules", "scheduled_block");
        policy.app_rules[0].mode = focus_core::ListMode::Blocklist;
        policy.app_rules[0].tier = tier;
        let db = Database::in_memory().unwrap();
        assert!(request_unlock(
            "editor",
            reason.into(),
            &EvaluationContext::new(&policy, &db, now())
        )
        .is_err());
    }
    let mut policy = config("app_rules", "controlled_access");
    policy.allowances[0].daily_minutes = 0;
    policy.schedules[0].windows[0].end = "23:59".parse().unwrap();
    let db = Database::in_memory().unwrap();
    let ctx = EvaluationContext::new(&policy, &db, now());
    assert!(request_unlock("focus", "1234567890!!!short".into(), &ctx).is_err());
    request_unlock("focus", reason.into(), &ctx).unwrap();
    assert!(request_unlock(
        "focus",
        "A different sufficiently long explanation for this request".into(),
        &ctx
    )
    .is_err());
    let later = EvaluationContext::new(&policy, &db, now() + Duration::minutes(61));
    assert!(request_unlock("focus", reason.to_uppercase(), &later).is_err());
    request_unlock(
        "focus",
        "A different sufficiently long explanation for this request".into(),
        &later,
    )
    .unwrap();
}
