use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{DateTime, Datelike, Duration, FixedOffset, SecondsFormat, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::{
    AllowanceConfig, AppMatcherConfig, AppMatcherKind, AppRuleConfig, Config, ConfigError,
    DetoxSession, Error, RuleConfig, RulePatternConfig, RulePatternKind, RuleTier, ScheduleConfig,
    ScheduleDay, ScheduleWindow, StrictModeConfig, TimeOfDay, UnlockState, VisitState, Weekday,
};

pub const EVENT_DETAIL_RETENTION_DAYS: i64 = 30;

pub struct Database {
    conn: Connection,
    event_log_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatState {
    pub component: String,
    pub last_seen_at: DateTime<Utc>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleActivityTotal {
    pub schedule_id: String,
    pub total_active_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSummary {
    pub total_events: u64,
    pub event_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationPreferences {
    pub enabled: bool,
    pub website_blocked: bool,
    pub application_blocked: bool,
    pub allowance_warnings: bool,
    pub allowance_warning_minutes: Vec<u32>,
    pub schedule_started: bool,
    pub schedule_ended: bool,
    pub detox_started: bool,
    pub detox_ended: bool,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            website_blocked: true,
            application_blocked: true,
            allowance_warnings: true,
            allowance_warning_minutes: vec![5, 1],
            schedule_started: true,
            schedule_ended: true,
            detox_started: true,
            detox_ended: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationEvent {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VisitUsage {
    pub rule_id: Option<String>,
    pub url: String,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let conn = Connection::open(path)?;
        migrate_database(&conn)?;
        Ok(Self {
            conn,
            event_log_path: None,
        })
    }

    pub fn in_memory() -> Result<Self, Error> {
        let conn = Connection::open_in_memory()?;
        migrate_database(&conn)?;
        Ok(Self {
            conn,
            event_log_path: None,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn set_event_log_path(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o640)
            .open(path)?;
        self.event_log_path = Some(path.to_path_buf());
        Ok(())
    }

    pub fn record_event(
        &self,
        kind: &str,
        target: Option<&str>,
        details: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<i64, Error> {
        let created_at = format_time(now);
        let transaction = self.conn.unchecked_transaction()?;
        let id = insert_event(&transaction, kind, target, details, &created_at)?;
        transaction.commit()?;

        if let Some(path) = &self.event_log_path {
            if let Err(error) = append_event_log(path, &created_at, kind, target, details) {
                eprintln!("BlocKuntu could not append to {}: {error}", path.display());
            }
        }

        Ok(id)
    }

    pub fn record_diagnostic_event(
        &self,
        source_event_id: &str,
        component: &str,
        details: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, Error> {
        let created_at = format_time(now);
        let transaction = self.conn.unchecked_transaction()?;
        let duplicate = transaction
            .query_row(
                "SELECT 1 FROM diagnostic_event_ids WHERE source_event_id = ?1",
                params![source_event_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if duplicate {
            return Ok(false);
        }

        let event_id = insert_event(
            &transaction,
            "runtime_diagnostic",
            Some(component),
            Some(details),
            &created_at,
        )?;
        transaction.execute(
            r#"
            INSERT INTO diagnostic_event_ids (source_event_id, event_id, created_at)
            VALUES (?1, ?2, ?3)
            "#,
            params![source_event_id, event_id, created_at],
        )?;
        transaction.commit()?;

        if let Some(path) = &self.event_log_path {
            if let Err(error) = append_event_log(
                path,
                &created_at,
                "runtime_diagnostic",
                Some(component),
                Some(details),
            ) {
                eprintln!("BlocKuntu could not append to {}: {error}", path.display());
            }
        }

        Ok(true)
    }

    pub fn event_summary(&self) -> Result<EventSummary, Error> {
        let mut statement = self
            .conn
            .prepare("SELECT kind, total_count FROM event_totals ORDER BY kind")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;
        let mut event_counts = BTreeMap::new();
        for row in rows {
            let (kind, count) = row?;
            event_counts.insert(kind, count);
        }
        let total_events = event_counts.values().copied().sum();
        Ok(EventSummary {
            total_events,
            event_counts,
        })
    }

    pub fn event_log_snapshot(&self) -> Result<String, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT created_at, kind, target, details
            FROM events
            ORDER BY created_at, id
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut snapshot = String::new();
        for row in rows {
            let (created_at, kind, target, details) = row?;
            snapshot.push_str(&format_event_log_line(
                &created_at,
                &kind,
                target.as_deref(),
                details.as_deref(),
            ));
        }
        Ok(snapshot)
    }

    pub fn enforce_event_retention(
        &self,
        now: DateTime<Utc>,
        retention: Duration,
    ) -> Result<usize, Error> {
        let cutoff = format_time(now - retention);
        let deleted = self
            .conn
            .execute("DELETE FROM events WHERE created_at < ?1", params![cutoff])?;

        if let Some(path) = &self.event_log_path {
            let snapshot = self.event_log_snapshot()?;
            rewrite_event_log(path, &snapshot)?;
        }

        Ok(deleted)
    }

    pub fn notification_preferences(&self) -> Result<NotificationPreferences, Error> {
        let stored = self
            .conn
            .query_row(
                r#"
                SELECT
                    enabled,
                    website_blocked,
                    application_blocked,
                    allowance_warnings,
                    allowance_warning_minutes,
                    schedule_started,
                    schedule_ended,
                    detox_started,
                    detox_ended
                FROM notification_preferences
                WHERE id = 1
                "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? != 0,
                        row.get::<_, i64>(1)? != 0,
                        row.get::<_, i64>(2)? != 0,
                        row.get::<_, i64>(3)? != 0,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)? != 0,
                        row.get::<_, i64>(6)? != 0,
                        row.get::<_, i64>(7)? != 0,
                        row.get::<_, i64>(8)? != 0,
                    ))
                },
            )
            .optional()?;

        let Some((
            enabled,
            website_blocked,
            application_blocked,
            allowance_warnings,
            thresholds,
            schedule_started,
            schedule_ended,
            detox_started,
            detox_ended,
        )) = stored
        else {
            return Ok(NotificationPreferences::default());
        };
        let allowance_warning_minutes = thresholds
            .split(',')
            .filter_map(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .collect::<Vec<_>>();

        Ok(NotificationPreferences {
            enabled,
            website_blocked,
            application_blocked,
            allowance_warnings,
            allowance_warning_minutes,
            schedule_started,
            schedule_ended,
            detox_started,
            detox_ended,
        })
    }

    pub fn set_notification_preferences(
        &self,
        preferences: &NotificationPreferences,
    ) -> Result<(), Error> {
        let thresholds = preferences
            .allowance_warning_minutes
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.conn.execute(
            r#"
            INSERT INTO notification_preferences (
                id,
                enabled,
                website_blocked,
                application_blocked,
                allowance_warnings,
                allowance_warning_minutes,
                schedule_started,
                schedule_ended,
                detox_started,
                detox_ended
            )
            VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                enabled = excluded.enabled,
                website_blocked = excluded.website_blocked,
                application_blocked = excluded.application_blocked,
                allowance_warnings = excluded.allowance_warnings,
                allowance_warning_minutes = excluded.allowance_warning_minutes,
                schedule_started = excluded.schedule_started,
                schedule_ended = excluded.schedule_ended,
                detox_started = excluded.detox_started,
                detox_ended = excluded.detox_ended
            "#,
            params![
                preferences.enabled,
                preferences.website_blocked,
                preferences.application_blocked,
                preferences.allowance_warnings,
                thresholds,
                preferences.schedule_started,
                preferences.schedule_ended,
                preferences.detox_started,
                preferences.detox_ended,
            ],
        )?;
        Ok(())
    }

    pub fn discard_disabled_notifications(
        &self,
        preferences: &NotificationPreferences,
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        let delivered_at = format_time(now);
        if !preferences.enabled {
            self.conn.execute(
                r#"
                UPDATE notification_outbox
                SET delivered_at = ?1
                WHERE delivered_at IS NULL
                "#,
                params![delivered_at],
            )?;
            return Ok(());
        }

        let disabled_kinds = [
            (!preferences.website_blocked).then_some("website_blocked"),
            (!preferences.application_blocked).then_some("application_blocked"),
            (!preferences.allowance_warnings).then_some("allowance_warning"),
            (!preferences.schedule_started).then_some("schedule_started"),
            (!preferences.schedule_ended).then_some("schedule_ended"),
            (!preferences.detox_started).then_some("detox_started"),
            (!preferences.detox_ended).then_some("detox_ended"),
        ];
        for kind in disabled_kinds.into_iter().flatten() {
            self.conn.execute(
                r#"
                UPDATE notification_outbox
                SET delivered_at = ?2
                WHERE delivered_at IS NULL AND kind = ?1
                "#,
                params![kind, delivered_at],
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn enqueue_notification(
        &self,
        kind: &str,
        title: &str,
        body: &str,
        dedupe_key: &str,
        now: DateTime<Utc>,
        cooldown: Duration,
        time_to_live: Duration,
    ) -> Result<Option<i64>, Error> {
        let existing = self
            .conn
            .query_row(
                r#"
                SELECT id
                FROM notification_outbox
                WHERE dedupe_key = ?1 AND created_at >= ?2
                ORDER BY id DESC
                LIMIT 1
                "#,
                params![dedupe_key, format_time(now - cooldown)],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if existing.is_some() {
            return Ok(None);
        }

        self.conn.execute(
            r#"
            INSERT INTO notification_outbox (
                kind,
                title,
                body,
                dedupe_key,
                created_at,
                expires_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                kind,
                title,
                body,
                dedupe_key,
                format_time(now),
                format_time(now + time_to_live)
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.record_event(
            "notification_queued",
            Some(&format!("{kind}:{id}")),
            Some(&format!(
                "title={title:?};body={body:?};expires_at={:?}",
                format_time(now + time_to_live)
            )),
            now,
        )?;
        Ok(Some(id))
    }

    pub fn pending_notifications(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<NotificationEvent>, Error> {
        self.conn.execute(
            "DELETE FROM notification_outbox WHERE delivered_at IS NULL AND expires_at <= ?1",
            params![format_time(now)],
        )?;
        self.conn.execute(
            "DELETE FROM notification_outbox WHERE delivered_at IS NOT NULL AND delivered_at <= ?1",
            params![format_time(now - Duration::days(7))],
        )?;
        let mut statement = self.conn.prepare(
            r#"
            SELECT
                notification_outbox.id,
                notification_outbox.kind,
                notification_outbox.title,
                notification_outbox.body,
                notification_outbox.created_at,
                notification_outbox.expires_at
            FROM notification_outbox
            LEFT JOIN notification_delivery_attempts
                ON notification_delivery_attempts.notification_id = notification_outbox.id
            WHERE
                notification_outbox.delivered_at IS NULL
                AND notification_outbox.expires_at > ?1
                AND (
                    notification_delivery_attempts.last_attempt_at IS NULL
                    OR notification_delivery_attempts.last_attempt_at <= ?2
                )
            ORDER BY notification_outbox.id
            LIMIT ?3
            "#,
        )?;
        let rows = statement.query_map(
            params![
                format_time(now),
                format_time(now - Duration::minutes(1)),
                i64::from(limit)
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?;

        let mut events = Vec::new();
        for row in rows {
            let (id, kind, title, body, created_at, expires_at) = row?;
            events.push(NotificationEvent {
                id,
                kind,
                title,
                body,
                created_at: parse_time(&created_at)?,
                expires_at: parse_time(&expires_at)?,
            });
        }
        Ok(events)
    }

    pub fn acknowledge_notifications(&self, ids: &[i64], now: DateTime<Utc>) -> Result<(), Error> {
        let transaction = self.conn.unchecked_transaction()?;
        for id in ids {
            transaction.execute(
                r#"
                UPDATE notification_outbox
                SET delivered_at = ?2
                WHERE id = ?1 AND delivered_at IS NULL
                "#,
                params![id, format_time(now)],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn record_notification_delivery_result(
        &self,
        id: i64,
        delivered: bool,
        detail: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<bool, Error> {
        let notification = self
            .conn
            .query_row(
                r#"
                SELECT kind, title, body
                FROM notification_outbox
                WHERE id = ?1
                "#,
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((kind, title, body)) = notification else {
            return Ok(false);
        };

        let attempt = if delivered {
            let previous_attempts = self
                .conn
                .query_row(
                    r#"
                    SELECT attempt_count
                    FROM notification_delivery_attempts
                    WHERE notification_id = ?1
                    "#,
                    params![id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0);
            self.acknowledge_notifications(&[id], now)?;
            self.conn.execute(
                "DELETE FROM notification_delivery_attempts WHERE notification_id = ?1",
                params![id],
            )?;
            previous_attempts + 1
        } else {
            self.conn.execute(
                r#"
                INSERT INTO notification_delivery_attempts (
                    notification_id,
                    attempt_count,
                    last_attempt_at,
                    last_error
                )
                VALUES (?1, 1, ?2, ?3)
                ON CONFLICT(notification_id) DO UPDATE SET
                    attempt_count = notification_delivery_attempts.attempt_count + 1,
                    last_attempt_at = excluded.last_attempt_at,
                    last_error = excluded.last_error
                "#,
                params![id, format_time(now), detail],
            )?;
            self.conn.query_row(
                r#"
                SELECT attempt_count
                FROM notification_delivery_attempts
                WHERE notification_id = ?1
                "#,
                params![id],
                |row| row.get::<_, i64>(0),
            )?
        };

        self.record_event(
            if delivered {
                "notification_accepted"
            } else {
                "notification_delivery_failed"
            },
            Some(&format!("{kind}:{id}")),
            Some(&format!(
                "attempt={attempt};title={title:?};body={body:?};detail={:?}",
                detail.unwrap_or(if delivered {
                    "accepted by desktop notification service"
                } else {
                    "no error detail supplied"
                })
            )),
            now,
        )?;
        Ok(true)
    }

    pub fn allowance_notification_state(
        &self,
        rule_id: &str,
    ) -> Result<Option<(String, i64)>, Error> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT local_day, remaining_seconds
                FROM notification_allowance_state
                WHERE rule_id = ?1
                "#,
                params![rule_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    pub fn set_allowance_notification_state(
        &self,
        rule_id: &str,
        local_day: &str,
        remaining_seconds: i64,
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        self.conn.execute(
            r#"
            INSERT INTO notification_allowance_state (
                rule_id,
                local_day,
                remaining_seconds,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(rule_id) DO UPDATE SET
                local_day = excluded.local_day,
                remaining_seconds = excluded.remaining_seconds,
                updated_at = excluded.updated_at
            "#,
            params![rule_id, local_day, remaining_seconds, format_time(now)],
        )?;
        Ok(())
    }

    pub fn notification_lifecycle_states(&self, kind: &str) -> Result<Vec<(String, bool)>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT target_id, active
            FROM notification_lifecycle_state
            WHERE kind = ?1
            "#,
        )?;
        let rows = statement.query_map(params![kind], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0))
        })?;
        let mut states = Vec::new();
        for row in rows {
            states.push(row?);
        }
        Ok(states)
    }

    pub fn set_notification_lifecycle_state(
        &self,
        kind: &str,
        target_id: &str,
        active: bool,
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        self.conn.execute(
            r#"
            INSERT INTO notification_lifecycle_state (kind, target_id, active, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(kind, target_id) DO UPDATE SET
                active = excluded.active,
                updated_at = excluded.updated_at
            "#,
            params![kind, target_id, active, format_time(now)],
        )?;
        Ok(())
    }

    pub fn delete_notification_lifecycle_state(
        &self,
        kind: &str,
        target_id: &str,
    ) -> Result<(), Error> {
        self.conn.execute(
            "DELETE FROM notification_lifecycle_state WHERE kind = ?1 AND target_id = ?2",
            params![kind, target_id],
        )?;
        Ok(())
    }

    pub fn sync_schedule_activity_totals(
        &self,
        schedules: &[ScheduleConfig],
        now: DateTime<FixedOffset>,
    ) -> Result<Vec<ScheduleActivityTotal>, Error> {
        let transaction = self.conn.unchecked_transaction()?;
        let now_utc = now.with_timezone(&Utc);
        let accounted_until = format_time(now_utc);
        let mut totals = Vec::with_capacity(schedules.len());

        for schedule in schedules {
            let previous = transaction
                .query_row(
                    r#"
                    SELECT total_active_seconds, accounted_until
                    FROM schedule_activity_totals
                    WHERE schedule_id = ?1
                    "#,
                    params![schedule.id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;

            let total_active_seconds = match previous {
                Some((total, previous_accounted_until)) => {
                    let previous_accounted_until = parse_time(&previous_accounted_until)?;
                    if now_utc <= previous_accounted_until {
                        total
                    } else {
                        let elapsed = schedule_active_seconds_between(
                            schedule,
                            previous_accounted_until.with_timezone(now.offset()),
                            now,
                        );
                        let total = total.saturating_add(elapsed);
                        transaction.execute(
                            r#"
                            UPDATE schedule_activity_totals
                            SET total_active_seconds = ?2, accounted_until = ?3
                            WHERE schedule_id = ?1
                            "#,
                            params![schedule.id, total, accounted_until],
                        )?;
                        total
                    }
                }
                None => {
                    transaction.execute(
                        r#"
                        INSERT INTO schedule_activity_totals (
                            schedule_id,
                            total_active_seconds,
                            accounted_until
                        )
                        VALUES (?1, 0, ?2)
                        "#,
                        params![schedule.id, accounted_until],
                    )?;
                    0
                }
            };

            totals.push(ScheduleActivityTotal {
                schedule_id: schedule.id.clone(),
                total_active_seconds,
            });
        }

        transaction.commit()?;
        Ok(totals)
    }

    pub fn upsert_heartbeat(
        &self,
        component: &str,
        details: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        self.conn.execute(
            r#"
            INSERT INTO heartbeats (component, last_seen_at, details)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(component) DO UPDATE SET
                last_seen_at = excluded.last_seen_at,
                details = excluded.details
            "#,
            params![component, format_time(now), details],
        )?;
        Ok(())
    }

    pub fn heartbeat(&self, component: &str) -> Result<Option<HeartbeatState>, Error> {
        let row = self
            .conn
            .query_row(
                "SELECT component, last_seen_at, details FROM heartbeats WHERE component = ?1",
                params![component],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((component, last_seen_at, details)) = row else {
            return Ok(None);
        };

        Ok(Some(HeartbeatState {
            component,
            last_seen_at: parse_time(&last_seen_at)?,
            details,
        }))
    }

    pub fn has_policy_config(&self) -> Result<bool, Error> {
        let count: i64 = self.conn.query_row(
            r#"
            SELECT
                (SELECT COUNT(*) FROM policy_site_lists) +
                (SELECT COUNT(*) FROM policy_app_rules) +
                (SELECT COUNT(*) FROM policy_schedules) +
                (SELECT COUNT(*) FROM policy_allowances) +
                (SELECT COUNT(*) FROM policy_defaults) +
                (SELECT COUNT(*) FROM policy_strict_mode)
            "#,
            [],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn load_policy_config(&self) -> Result<Config, Error> {
        let allowances = self.load_policy_allowances()?;
        let schedules = self.load_policy_schedules()?;
        let rules = self.load_policy_site_lists()?;
        let app_rules = self.load_policy_app_rules()?;
        let strict_mode = self.load_policy_strict_mode()?;

        let config = Config {
            rules,
            app_rules,
            schedules,
            allowances,
            strict_mode,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn replace_policy_config(&self, config: &Config) -> Result<(), Error> {
        config.validate()?;

        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute_batch(
            r#"
            DELETE FROM policy_site_list_schedules;
            DELETE FROM policy_site_list_patterns;
            DELETE FROM policy_site_lists;
            DELETE FROM policy_app_rule_schedules;
            DELETE FROM policy_app_rule_matchers;
            DELETE FROM policy_app_rules;
            DELETE FROM policy_schedule_windows;
            DELETE FROM policy_schedules;
            DELETE FROM policy_allowances;
            DELETE FROM policy_defaults;
            DELETE FROM policy_strict_mode;
            "#,
        )?;

        transaction.execute(
            r#"
            INSERT INTO policy_defaults (
                key,
                max_session_minutes,
                cooldown_minutes,
                max_unlocks_per_hour
            )
            VALUES (1, ?1, ?2, ?3)
            "#,
            params![2_i64, 0_i64, 1_i64],
        )?;

        transaction.execute(
            r#"
            INSERT INTO policy_strict_mode (
                key,
                require_firefox_extension,
                require_chrome_extension,
                kill_supported_browser_if_extension_stale,
                block_unsupported_browsers,
                grace_seconds
            )
            VALUES (1, ?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                bool_to_i64(config.strict_mode.require_firefox_extension),
                bool_to_i64(config.strict_mode.require_chrome_extension),
                bool_to_i64(config.strict_mode.kill_supported_browser_if_extension_stale),
                bool_to_i64(config.strict_mode.block_unsupported_browsers),
                i64::from(config.strict_mode.grace_seconds),
            ],
        )?;

        for allowance in &config.allowances {
            transaction.execute(
                r#"
                INSERT INTO policy_allowances (id, name, daily_minutes)
                VALUES (?1, ?2, ?3)
                "#,
                params![
                    &allowance.id,
                    allowance.name.as_deref(),
                    i64::from(allowance.daily_minutes)
                ],
            )?;
        }

        for schedule in &config.schedules {
            transaction.execute(
                "INSERT INTO policy_schedules (id, name) VALUES (?1, ?2)",
                params![&schedule.id, schedule.name.as_deref()],
            )?;

            for (position, window) in schedule.windows.iter().enumerate() {
                transaction.execute(
                    r#"
                    INSERT INTO policy_schedule_windows (
                        schedule_id,
                        weekday,
                        start_time,
                        end_time,
                        position
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    "#,
                    params![
                        &schedule.id,
                        schedule_day_to_str(window.weekday),
                        window.start.to_string(),
                        window.end.to_string(),
                        position as i64,
                    ],
                )?;
            }
        }

        for rule in &config.rules {
            transaction.execute(
                r#"
                INSERT INTO policy_site_lists (
                    id,
                    name,
                    tier,
                    enabled,
                    allowance_id,
                    max_session_minutes,
                    cooldown_minutes,
                    max_unlocks_per_hour
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    &rule.id,
                    &rule.name,
                    rule_tier_to_str(rule.tier),
                    if rule.enabled { 1_i64 } else { 0_i64 },
                    rule.allowance_id.as_deref(),
                    Option::<i64>::None,
                    Option::<i64>::None,
                    Option::<i64>::None,
                ],
            )?;

            for (position, pattern) in rule.patterns.iter().enumerate() {
                transaction.execute(
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
                    params![
                        &rule.id,
                        pattern_kind_to_str(pattern.kind),
                        &pattern.value,
                        if pattern.match_subdomains {
                            1_i64
                        } else {
                            0_i64
                        },
                        position as i64,
                    ],
                )?;
            }

            for (position, schedule_id) in rule.schedule_ids.iter().enumerate() {
                transaction.execute(
                    r#"
                    INSERT INTO policy_site_list_schedules (list_id, schedule_id, position)
                    VALUES (?1, ?2, ?3)
                    "#,
                    params![&rule.id, schedule_id, position as i64],
                )?;
            }
        }

        for rule in &config.app_rules {
            transaction.execute(
                r#"
                INSERT INTO policy_app_rules (
                    id,
                    name,
                    tier,
                    enabled,
                    allowance_id,
                    max_session_minutes,
                    cooldown_minutes,
                    max_unlocks_per_hour
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    &rule.id,
                    &rule.name,
                    rule_tier_to_str(rule.tier),
                    if rule.enabled { 1_i64 } else { 0_i64 },
                    rule.allowance_id.as_deref(),
                    Option::<i64>::None,
                    Option::<i64>::None,
                    Option::<i64>::None,
                ],
            )?;

            for (position, matcher) in rule.matchers.iter().enumerate() {
                transaction.execute(
                    r#"
                    INSERT INTO policy_app_rule_matchers (
                        rule_id,
                        kind,
                        value,
                        position
                    )
                    VALUES (?1, ?2, ?3, ?4)
                    "#,
                    params![
                        &rule.id,
                        app_matcher_kind_to_str(matcher.kind),
                        &matcher.value,
                        position as i64,
                    ],
                )?;
            }

            for (position, schedule_id) in rule.schedule_ids.iter().enumerate() {
                transaction.execute(
                    r#"
                    INSERT INTO policy_app_rule_schedules (rule_id, schedule_id, position)
                    VALUES (?1, ?2, ?3)
                    "#,
                    params![&rule.id, schedule_id, position as i64],
                )?;
            }
        }

        transaction.commit()?;
        Ok(())
    }

    fn load_policy_strict_mode(&self) -> Result<StrictModeConfig, Error> {
        let row = self
            .conn
            .query_row(
                r#"
                SELECT
                    require_firefox_extension,
                    require_chrome_extension,
                    kill_supported_browser_if_extension_stale,
                    block_unsupported_browsers,
                    grace_seconds
                FROM policy_strict_mode
                WHERE key = 1
                "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            require_firefox_extension,
            require_chrome_extension,
            kill_supported_browser_if_extension_stale,
            block_unsupported_browsers,
            grace_seconds,
        )) = row
        else {
            return Ok(StrictModeConfig::default());
        };

        Ok(StrictModeConfig {
            require_firefox_extension: require_firefox_extension != 0,
            require_chrome_extension: require_chrome_extension != 0,
            kill_supported_browser_if_extension_stale: kill_supported_browser_if_extension_stale
                != 0,
            block_unsupported_browsers: block_unsupported_browsers != 0,
            grace_seconds: to_u32("strict_mode.grace_seconds", grace_seconds)?,
        })
    }

    fn load_policy_allowances(&self) -> Result<Vec<AllowanceConfig>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, name, daily_minutes
            FROM policy_allowances
            ORDER BY id
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let mut allowances = Vec::new();
        for row in rows {
            let (id, name, daily_minutes) = row?;
            allowances.push(AllowanceConfig {
                id,
                name,
                daily_minutes: to_u32("allowance.daily_minutes", daily_minutes)?,
            });
        }
        Ok(allowances)
    }

    fn load_policy_schedules(&self) -> Result<Vec<ScheduleConfig>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, name
            FROM policy_schedules
            ORDER BY id
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        let mut schedules = Vec::new();
        for row in rows {
            let (id, name) = row?;
            schedules.push(ScheduleConfig {
                windows: self.load_policy_schedule_windows(&id)?,
                id,
                name,
            });
        }
        Ok(schedules)
    }

    fn load_policy_schedule_windows(
        &self,
        schedule_id: &str,
    ) -> Result<Vec<ScheduleWindow>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT weekday, start_time, end_time
            FROM policy_schedule_windows
            WHERE schedule_id = ?1
            ORDER BY position, id
            "#,
        )?;
        let rows = statement.query_map([schedule_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut windows = Vec::new();
        for row in rows {
            let (weekday, start, end) = row?;
            windows.push(ScheduleWindow {
                weekday: schedule_day_from_str(&weekday)?,
                start: TimeOfDay::from_str(&start).map_err(|err| ConfigError::Validation(err))?,
                end: TimeOfDay::from_str(&end).map_err(|err| ConfigError::Validation(err))?,
            });
        }
        Ok(windows)
    }

    fn load_policy_site_lists(&self) -> Result<Vec<RuleConfig>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT
                id,
                name,
                tier,
                enabled,
                allowance_id
            FROM policy_site_lists
            ORDER BY id
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        let mut rules = Vec::new();
        for row in rows {
            let (id, name, tier, enabled, allowance_id) = row?;
            rules.push(RuleConfig {
                patterns: self.load_policy_site_list_patterns(&id)?,
                schedule_ids: self.load_policy_site_list_schedule_ids(&id)?,
                id,
                name,
                tier: rule_tier_from_str(&tier)?,
                enabled: enabled != 0,
                allowance_id,
            });
        }
        Ok(rules)
    }

    fn load_policy_site_list_patterns(
        &self,
        list_id: &str,
    ) -> Result<Vec<RulePatternConfig>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT kind, value, match_subdomains
            FROM policy_site_list_patterns
            WHERE list_id = ?1
            ORDER BY position, id
            "#,
        )?;
        let rows = statement.query_map([list_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let mut patterns = Vec::new();
        for row in rows {
            let (kind, value, match_subdomains) = row?;
            patterns.push(RulePatternConfig {
                kind: pattern_kind_from_str(&kind)?,
                value,
                match_subdomains: match_subdomains != 0,
            });
        }
        Ok(patterns)
    }

    fn load_policy_site_list_schedule_ids(&self, list_id: &str) -> Result<Vec<String>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT schedule_id
            FROM policy_site_list_schedules
            WHERE list_id = ?1
            ORDER BY position, schedule_id
            "#,
        )?;
        let rows = statement.query_map([list_id], |row| row.get::<_, String>(0))?;

        let mut schedule_ids = Vec::new();
        for row in rows {
            schedule_ids.push(row?);
        }
        Ok(schedule_ids)
    }

    fn load_policy_app_rules(&self) -> Result<Vec<AppRuleConfig>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT
                id,
                name,
                tier,
                enabled,
                allowance_id
            FROM policy_app_rules
            ORDER BY id
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        let mut app_rules = Vec::new();
        for row in rows {
            let (id, name, tier, enabled, allowance_id) = row?;
            app_rules.push(AppRuleConfig {
                matchers: self.load_policy_app_rule_matchers(&id)?,
                schedule_ids: self.load_policy_app_rule_schedule_ids(&id)?,
                id,
                name,
                tier: rule_tier_from_str(&tier)?,
                enabled: enabled != 0,
                allowance_id,
            });
        }
        Ok(app_rules)
    }

    fn load_policy_app_rule_matchers(&self, rule_id: &str) -> Result<Vec<AppMatcherConfig>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT kind, value
            FROM policy_app_rule_matchers
            WHERE rule_id = ?1
            ORDER BY position, id
            "#,
        )?;
        let rows = statement.query_map([rule_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut matchers = Vec::new();
        for row in rows {
            let (kind, value) = row?;
            matchers.push(AppMatcherConfig {
                kind: app_matcher_kind_from_str(&kind)?,
                value,
            });
        }
        Ok(matchers)
    }

    fn load_policy_app_rule_schedule_ids(&self, rule_id: &str) -> Result<Vec<String>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT schedule_id
            FROM policy_app_rule_schedules
            WHERE rule_id = ?1
            ORDER BY position, schedule_id
            "#,
        )?;
        let rows = statement.query_map([rule_id], |row| row.get::<_, String>(0))?;

        let mut schedule_ids = Vec::new();
        for row in rows {
            schedule_ids.push(row?);
        }
        Ok(schedule_ids)
    }

    pub fn set_service_state(
        &self,
        key: &str,
        value: &str,
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        self.conn.execute(
            r#"
            INSERT INTO service_state (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            "#,
            params![key, value, format_time(now)],
        )?;
        Ok(())
    }

    pub fn service_state(&self, key: &str) -> Result<Option<String>, Error> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM service_state WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn insert_detox_session(&self, session: &DetoxSession) -> Result<DetoxSession, Error> {
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            r#"
            INSERT INTO detox_sessions (id, name, starts_at, ends_at, cancelled_at, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                &session.id,
                session.name.as_deref(),
                format_time(session.starts_at),
                format_time(session.ends_at),
                session.cancelled_at.map(format_time),
                format_time(session.starts_at),
            ],
        )?;

        for (position, rule_id) in session.site_rule_ids.iter().enumerate() {
            transaction.execute(
                r#"
                INSERT INTO detox_session_site_rules (session_id, rule_id, position)
                VALUES (?1, ?2, ?3)
                "#,
                params![&session.id, rule_id, position as i64],
            )?;
        }

        for (position, rule_id) in session.app_rule_ids.iter().enumerate() {
            transaction.execute(
                r#"
                INSERT INTO detox_session_app_rules (session_id, rule_id, position)
                VALUES (?1, ?2, ?3)
                "#,
                params![&session.id, rule_id, position as i64],
            )?;
        }

        transaction.commit()?;
        Ok(session.clone())
    }

    pub fn active_detox_sessions(&self, now: DateTime<Utc>) -> Result<Vec<DetoxSession>, Error> {
        let now = format_time(now);
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, name, starts_at, ends_at, cancelled_at
            FROM detox_sessions
            WHERE starts_at <= ?1
              AND ends_at > ?1
              AND cancelled_at IS NULL
            ORDER BY ends_at DESC, starts_at DESC, id
            "#,
        )?;
        self.load_detox_sessions_from_statement(&mut statement, params![now])
    }

    pub fn uncancelled_detox_sessions(&self) -> Result<Vec<DetoxSession>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, name, starts_at, ends_at, cancelled_at
            FROM detox_sessions
            WHERE cancelled_at IS NULL
            ORDER BY ends_at DESC, starts_at DESC, id
            "#,
        )?;
        self.load_detox_sessions_from_statement(&mut statement, [])
    }

    pub fn detox_sessions(&self, limit: u32) -> Result<Vec<DetoxSession>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, name, starts_at, ends_at, cancelled_at
            FROM detox_sessions
            ORDER BY starts_at DESC, id DESC
            LIMIT ?1
            "#,
        )?;
        self.load_detox_sessions_from_statement(&mut statement, params![i64::from(limit)])
    }

    pub fn detox_session(&self, id: &str) -> Result<Option<DetoxSession>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, name, starts_at, ends_at, cancelled_at
            FROM detox_sessions
            WHERE id = ?1
            "#,
        )?;
        let mut sessions = self.load_detox_sessions_from_statement(&mut statement, params![id])?;
        Ok(sessions.pop())
    }

    pub(crate) fn detox_intervals_for_site_rule_between(
        &self,
        rule_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT sessions.starts_at, sessions.ends_at, sessions.cancelled_at
            FROM detox_sessions AS sessions
            INNER JOIN detox_session_site_rules AS targets
                ON targets.session_id = sessions.id
            WHERE targets.rule_id = ?1
              AND sessions.starts_at < ?3
              AND sessions.ends_at > ?2
              AND (sessions.cancelled_at IS NULL OR sessions.cancelled_at > ?2)
            ORDER BY sessions.starts_at, sessions.ends_at
            "#,
        )?;
        let rows = statement.query_map(
            params![rule_id, format_time(start), format_time(end)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?;

        let mut intervals = Vec::new();
        for row in rows {
            let (starts_at, ends_at, cancelled_at) = row?;
            let starts_at = parse_time(&starts_at)?;
            let mut ends_at = parse_time(&ends_at)?;
            if let Some(cancelled_at) = cancelled_at {
                ends_at = ends_at.min(parse_time(&cancelled_at)?);
            }
            let overlap_start = starts_at.max(start);
            let overlap_end = ends_at.min(end);
            if overlap_end > overlap_start {
                intervals.push((overlap_start, overlap_end));
            }
        }
        Ok(intervals)
    }

    pub fn cancel_detox_session(
        &self,
        id: &str,
        cancelled_at: DateTime<Utc>,
    ) -> Result<Option<DetoxSession>, Error> {
        self.conn.execute(
            r#"
            UPDATE detox_sessions
            SET cancelled_at = ?2
            WHERE id = ?1 AND cancelled_at IS NULL
            "#,
            params![id, format_time(cancelled_at)],
        )?;
        self.detox_session(id)
    }

    fn load_detox_sessions_from_statement<P>(
        &self,
        statement: &mut rusqlite::Statement<'_>,
        params: P,
    ) -> Result<Vec<DetoxSession>, Error>
    where
        P: rusqlite::Params,
    {
        let rows = statement.query_map(params, detox_session_base_from_row)?;
        let mut sessions = Vec::new();
        for row in rows {
            let mut session = row?;
            session.site_rule_ids =
                self.load_detox_session_rule_ids("detox_session_site_rules", &session.id)?;
            session.app_rule_ids =
                self.load_detox_session_rule_ids("detox_session_app_rules", &session.id)?;
            sessions.push(session);
        }
        Ok(sessions)
    }

    fn load_detox_session_rule_ids(
        &self,
        table: &str,
        session_id: &str,
    ) -> Result<Vec<String>, Error> {
        let sql = format!(
            r#"
            SELECT rule_id
            FROM {table}
            WHERE session_id = ?1
            ORDER BY position, rule_id
            "#
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([session_id], |row| row.get::<_, String>(0))?;
        let mut rule_ids = Vec::new();
        for row in rows {
            rule_ids.push(row?);
        }
        Ok(rule_ids)
    }

    pub(crate) fn insert_unlock(
        &self,
        target: &str,
        rule_id: &str,
        minutes: u32,
        reason: &str,
        started_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<UnlockState, Error> {
        self.conn.execute(
            r#"
            INSERT INTO unlocks (target, rule_id, reason, minutes, started_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                target,
                rule_id,
                reason,
                i64::from(minutes),
                format_time(started_at),
                format_time(expires_at),
            ],
        )?;

        Ok(UnlockState {
            id: self.conn.last_insert_rowid(),
            target: target.to_string(),
            rule_id: rule_id.to_string(),
            minutes,
            reason: reason.to_string(),
            started_at,
            expires_at,
        })
    }

    pub(crate) fn active_unlock_for_rule(
        &self,
        rule_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<UnlockState>, Error> {
        let now = format_time(now);
        self.conn
            .query_row(
                r#"
                SELECT id, target, rule_id, minutes, reason, started_at, expires_at
                FROM unlocks
                WHERE rule_id = ?1 AND expires_at > ?2
                ORDER BY expires_at DESC
                LIMIT 1
                "#,
                params![rule_id, now],
                unlock_from_row,
            )
            .optional()
            .map_err(Error::from)
    }

    pub(crate) fn count_unlocks_since(&self, since: DateTime<Utc>) -> Result<u32, Error> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM unlocks WHERE started_at >= ?1",
            params![format_time(since)],
            |row| row.get(0),
        )?;
        Ok(count.try_into().unwrap_or(u32::MAX))
    }

    pub(crate) fn unlock_reasons(&self) -> Result<Vec<String>, Error> {
        let mut statement = self.conn.prepare("SELECT reason FROM unlocks")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut reasons = Vec::new();
        for row in rows {
            reasons.push(row?);
        }
        Ok(reasons)
    }

    pub(crate) fn used_seconds_for_app_rule_between(
        &self,
        rule_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<i64, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT started_at, ended_at
            FROM app_usage_sessions
            WHERE rule_id = ?1
              AND started_at < ?2
              AND COALESCE(ended_at, ?3) > ?4
            "#,
        )?;

        let rows = statement.query_map(
            params![
                rule_id,
                format_time(period_end),
                format_time(now),
                format_time(period_start)
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )?;

        let mut used_seconds = 0_i64;
        for row in rows {
            let (started_at, ended_at) = row?;
            let started_at = parse_time(&started_at)?;
            let ended_at = match ended_at {
                Some(ended_at) => parse_time(&ended_at)?,
                None => now,
            };

            let clamped_start = started_at.max(period_start);
            let clamped_end = ended_at.min(now).min(period_end);
            if clamped_end > clamped_start {
                used_seconds += (clamped_end - clamped_start).num_seconds();
            }
        }

        Ok(used_seconds)
    }

    pub fn sync_app_usage_sessions(
        &self,
        active_rule_ids: &[String],
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        self.close_duplicate_open_app_usage_sessions()?;

        let active_rule_ids = active_rule_ids
            .iter()
            .map(|rule_id| rule_id.as_str())
            .collect::<HashSet<_>>();
        let open_rule_ids = self.open_app_usage_rule_ids()?;

        for rule_id in &active_rule_ids {
            if open_rule_ids.contains(*rule_id) {
                self.conn.execute(
                    r#"
                    UPDATE app_usage_sessions
                    SET last_seen_at = ?2
                    WHERE rule_id = ?1 AND ended_at IS NULL
                    "#,
                    params![rule_id, format_time(now)],
                )?;
            } else {
                self.conn.execute(
                    r#"
                    INSERT INTO app_usage_sessions (rule_id, started_at, last_seen_at)
                    VALUES (?1, ?2, ?2)
                    "#,
                    params![rule_id, format_time(now)],
                )?;
            }
        }

        for rule_id in open_rule_ids {
            if !active_rule_ids.contains(rule_id.as_str()) {
                self.end_open_app_usage_session(&rule_id, now)?;
            }
        }

        Ok(())
    }

    pub fn end_open_app_usage_sessions(&self, now: DateTime<Utc>) -> Result<(), Error> {
        self.conn.execute(
            r#"
            UPDATE app_usage_sessions
            SET ended_at = ?1, last_seen_at = ?1
            WHERE ended_at IS NULL
            "#,
            params![format_time(now)],
        )?;
        Ok(())
    }

    pub fn insert_app_usage_interval(
        &self,
        rule_id: &str,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
    ) -> Result<i64, Error> {
        self.conn.execute(
            r#"
            INSERT INTO app_usage_sessions (rule_id, started_at, last_seen_at, ended_at)
            VALUES (?1, ?2, ?3, ?3)
            "#,
            params![rule_id, format_time(started_at), format_time(ended_at)],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn open_app_usage_rule_ids(&self) -> Result<HashSet<String>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT DISTINCT rule_id
            FROM app_usage_sessions
            WHERE ended_at IS NULL
            "#,
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;

        let mut rule_ids = HashSet::new();
        for row in rows {
            rule_ids.insert(row?);
        }
        Ok(rule_ids)
    }

    fn end_open_app_usage_session(&self, rule_id: &str, now: DateTime<Utc>) -> Result<(), Error> {
        self.conn.execute(
            r#"
            UPDATE app_usage_sessions
            SET ended_at = ?2, last_seen_at = ?2
            WHERE rule_id = ?1 AND ended_at IS NULL
            "#,
            params![rule_id, format_time(now)],
        )?;
        Ok(())
    }

    fn close_duplicate_open_app_usage_sessions(&self) -> Result<(), Error> {
        self.conn.execute(
            r#"
            UPDATE app_usage_sessions
            SET ended_at = last_seen_at
            WHERE ended_at IS NULL
              AND id NOT IN (
                SELECT MAX(id)
                FROM app_usage_sessions
                WHERE ended_at IS NULL
                GROUP BY rule_id
              )
            "#,
            [],
        )?;
        Ok(())
    }

    pub(crate) fn visit_usage_between(
        &self,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Vec<VisitUsage>, Error> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT rule_id, url, started_at, last_heartbeat_at, ended_at
            FROM visits
            WHERE started_at < ?1
              AND COALESCE(ended_at, last_heartbeat_at, ?2) > ?3
            "#,
        )?;

        let rows = statement.query_map(
            params![
                format_time(period_end),
                format_time(now),
                format_time(period_start)
            ],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )?;

        let mut visits = Vec::new();
        for row in rows {
            let (rule_id, url, started_at, last_heartbeat_at, ended_at) = row?;
            visits.push(VisitUsage {
                rule_id,
                url,
                started_at: parse_time(&started_at)?,
                last_heartbeat_at: parse_time(&last_heartbeat_at)?,
                ended_at: ended_at.map(|value| parse_time(&value)).transpose()?,
            });
        }

        Ok(visits)
    }

    pub(crate) fn insert_visit_start(
        &self,
        rule_id: Option<&str>,
        target: &str,
        url: &str,
        tab_id: &str,
        now: DateTime<Utc>,
    ) -> Result<VisitState, Error> {
        self.conn.execute(
            r#"
            UPDATE visits
            SET ended_at = ?1, last_heartbeat_at = ?1
            WHERE tab_id = ?2 AND ended_at IS NULL
            "#,
            params![format_time(now), tab_id],
        )?;

        self.conn.execute(
            r#"
            INSERT INTO visits (rule_id, target, url, tab_id, started_at, last_heartbeat_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            "#,
            params![rule_id, target, url, tab_id, format_time(now)],
        )?;

        Ok(VisitState {
            id: self.conn.last_insert_rowid(),
            target: target.to_string(),
            rule_id: rule_id.map(ToOwned::to_owned),
            url: url.to_string(),
            tab_id: tab_id.to_string(),
            started_at: now,
            last_heartbeat_at: now,
            ended_at: None,
        })
    }

    pub(crate) fn update_visit_heartbeat(
        &self,
        visit_id: i64,
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        self.conn.execute(
            "UPDATE visits SET last_heartbeat_at = ?1 WHERE id = ?2 AND ended_at IS NULL",
            params![format_time(now), visit_id],
        )?;
        Ok(())
    }

    pub(crate) fn end_visit(&self, visit_id: i64, now: DateTime<Utc>) -> Result<(), Error> {
        self.conn.execute(
            r#"
            UPDATE visits
            SET ended_at = ?1, last_heartbeat_at = ?1
            WHERE id = ?2 AND ended_at IS NULL
            "#,
            params![format_time(now), visit_id],
        )?;
        Ok(())
    }

    pub fn insert_visit_interval(
        &self,
        rule_id: &str,
        target: &str,
        url: &str,
        tab_id: &str,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
    ) -> Result<i64, Error> {
        self.conn.execute(
            r#"
            INSERT INTO visits (
                rule_id,
                target,
                url,
                tab_id,
                started_at,
                last_heartbeat_at,
                ended_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            "#,
            params![
                rule_id,
                target,
                url,
                tab_id,
                format_time(started_at),
                format_time(ended_at)
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
}

fn append_event_log(
    path: &Path,
    created_at: &str,
    kind: &str,
    target: Option<&str>,
    details: Option<&str>,
) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o640)
        .open(path)?;
    file.write_all(format_event_log_line(created_at, kind, target, details).as_bytes())?;
    file.sync_data()
}

fn format_event_log_line(
    created_at: &str,
    kind: &str,
    target: Option<&str>,
    details: Option<&str>,
) -> String {
    format!("{created_at} kind={kind:?} target={target:?} details={details:?}\n")
}

fn rewrite_event_log(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("blockuntu.log");
    let temporary_path =
        path.with_file_name(format!(".{file_name}.{}.retention.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o640)
            .open(&temporary_path)?;
        file.set_permissions(fs::Permissions::from_mode(0o640))?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn insert_event(
    transaction: &Transaction<'_>,
    kind: &str,
    target: Option<&str>,
    details: Option<&str>,
    created_at: &str,
) -> Result<i64, Error> {
    transaction.execute(
        "INSERT INTO events (kind, target, details, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![kind, target, details, created_at],
    )?;
    let id = transaction.last_insert_rowid();
    transaction.execute(
        r#"
        INSERT INTO event_totals (kind, total_count)
        VALUES (?1, 1)
        ON CONFLICT(kind) DO UPDATE SET
            total_count = total_count + 1
        "#,
        params![kind],
    )?;
    Ok(id)
}

pub fn migrate_database(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS unlocks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            target TEXT NOT NULL,
            rule_id TEXT NOT NULL,
            reason TEXT NOT NULL,
            minutes INTEGER NOT NULL,
            started_at TEXT NOT NULL,
            expires_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_unlocks_rule_started
            ON unlocks(rule_id, started_at);
        CREATE INDEX IF NOT EXISTS idx_unlocks_rule_expires
            ON unlocks(rule_id, expires_at);

        CREATE TABLE IF NOT EXISTS visits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id TEXT,
            target TEXT NOT NULL,
            url TEXT NOT NULL,
            tab_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            last_heartbeat_at TEXT NOT NULL,
            ended_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_visits_rule_started
            ON visits(rule_id, started_at);

        CREATE TABLE IF NOT EXISTS app_usage_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            ended_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_app_usage_sessions_rule_started
            ON app_usage_sessions(rule_id, started_at);
        CREATE INDEX IF NOT EXISTS idx_app_usage_sessions_open
            ON app_usage_sessions(rule_id, ended_at);

        CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            target TEXT,
            details TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_events_kind_created
            ON events(kind, created_at);

        CREATE TABLE IF NOT EXISTS event_totals (
            kind TEXT PRIMARY KEY,
            total_count INTEGER NOT NULL DEFAULT 0 CHECK (total_count >= 0)
        );

        CREATE TABLE IF NOT EXISTS diagnostic_event_ids (
            source_event_id TEXT PRIMARY KEY,
            event_id INTEGER NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            FOREIGN KEY(event_id) REFERENCES events(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_diagnostic_event_ids_created
            ON diagnostic_event_ids(created_at);

        CREATE TABLE IF NOT EXISTS notification_preferences (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            enabled INTEGER NOT NULL DEFAULT 1,
            website_blocked INTEGER NOT NULL DEFAULT 1,
            application_blocked INTEGER NOT NULL DEFAULT 1,
            allowance_warnings INTEGER NOT NULL DEFAULT 1,
            allowance_warning_minutes TEXT NOT NULL DEFAULT '5,1',
            schedule_started INTEGER NOT NULL DEFAULT 1,
            schedule_ended INTEGER NOT NULL DEFAULT 1,
            detox_started INTEGER NOT NULL DEFAULT 1,
            detox_ended INTEGER NOT NULL DEFAULT 1
        );

        INSERT OR IGNORE INTO notification_preferences (id) VALUES (1);

        CREATE TABLE IF NOT EXISTS notification_outbox (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            dedupe_key TEXT NOT NULL,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            delivered_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_notification_outbox_pending
            ON notification_outbox(delivered_at, expires_at, id);
        CREATE INDEX IF NOT EXISTS idx_notification_outbox_dedupe
            ON notification_outbox(dedupe_key, created_at);

        CREATE TABLE IF NOT EXISTS notification_delivery_attempts (
            notification_id INTEGER PRIMARY KEY,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            last_attempt_at TEXT NOT NULL,
            last_error TEXT,
            FOREIGN KEY(notification_id) REFERENCES notification_outbox(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS notification_allowance_state (
            rule_id TEXT PRIMARY KEY,
            local_day TEXT NOT NULL,
            remaining_seconds INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS notification_lifecycle_state (
            kind TEXT NOT NULL,
            target_id TEXT NOT NULL,
            active INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(kind, target_id)
        );

        CREATE TABLE IF NOT EXISTS schedule_activity_totals (
            schedule_id TEXT PRIMARY KEY,
            total_active_seconds INTEGER NOT NULL DEFAULT 0 CHECK (total_active_seconds >= 0),
            accounted_until TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS heartbeats (
            component TEXT PRIMARY KEY,
            last_seen_at TEXT NOT NULL,
            details TEXT
        );

        CREATE TABLE IF NOT EXISTS service_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS detox_sessions (
            id TEXT PRIMARY KEY,
            name TEXT,
            starts_at TEXT NOT NULL,
            ends_at TEXT NOT NULL,
            cancelled_at TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_detox_sessions_active
            ON detox_sessions(starts_at, ends_at, cancelled_at);

        CREATE TABLE IF NOT EXISTS detox_session_site_rules (
            session_id TEXT NOT NULL,
            rule_id TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(session_id, rule_id),
            FOREIGN KEY(session_id) REFERENCES detox_sessions(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_detox_session_site_rules_rule
            ON detox_session_site_rules(rule_id);

        CREATE TABLE IF NOT EXISTS detox_session_app_rules (
            session_id TEXT NOT NULL,
            rule_id TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(session_id, rule_id),
            FOREIGN KEY(session_id) REFERENCES detox_sessions(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_detox_session_app_rules_rule
            ON detox_session_app_rules(rule_id);

        CREATE TABLE IF NOT EXISTS policy_defaults (
            key INTEGER PRIMARY KEY CHECK (key = 1),
            max_session_minutes INTEGER NOT NULL,
            cooldown_minutes INTEGER NOT NULL,
            max_unlocks_per_hour INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS policy_strict_mode (
            key INTEGER PRIMARY KEY CHECK (key = 1),
            require_firefox_extension INTEGER NOT NULL,
            require_chrome_extension INTEGER NOT NULL,
            kill_supported_browser_if_extension_stale INTEGER NOT NULL,
            block_unsupported_browsers INTEGER NOT NULL,
            grace_seconds INTEGER NOT NULL CHECK (grace_seconds > 0)
        );

        CREATE TABLE IF NOT EXISTS policy_allowances (
            id TEXT PRIMARY KEY,
            name TEXT,
            daily_minutes INTEGER NOT NULL CHECK (daily_minutes >= 0)
        );

        CREATE TABLE IF NOT EXISTS policy_schedules (
            id TEXT PRIMARY KEY,
            name TEXT
        );

        CREATE TABLE IF NOT EXISTS policy_schedule_windows (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            schedule_id TEXT NOT NULL,
            weekday TEXT NOT NULL CHECK (weekday IN ('everyday', 'workdays', 'weekend', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun')),
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(schedule_id) REFERENCES policy_schedules(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_policy_schedule_windows_schedule
            ON policy_schedule_windows(schedule_id, position);

        CREATE TABLE IF NOT EXISTS policy_site_lists (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'scheduled_block', 'controlled_access')),
            enabled INTEGER NOT NULL DEFAULT 1,
            allowance_id TEXT,
            max_session_minutes INTEGER,
            cooldown_minutes INTEGER,
            max_unlocks_per_hour INTEGER,
            FOREIGN KEY(allowance_id) REFERENCES policy_allowances(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS policy_site_list_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            list_id TEXT NOT NULL,
            kind TEXT NOT NULL CHECK (kind IN ('domain', 'exact_url', 'url_prefix', 'url_contains', 'path_prefix')),
            value TEXT NOT NULL,
            match_subdomains INTEGER NOT NULL DEFAULT 0,
            position INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(list_id) REFERENCES policy_site_lists(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_policy_site_list_patterns_list
            ON policy_site_list_patterns(list_id, position);

        CREATE TABLE IF NOT EXISTS policy_site_list_schedules (
            list_id TEXT NOT NULL,
            schedule_id TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(list_id, schedule_id),
            FOREIGN KEY(list_id) REFERENCES policy_site_lists(id) ON DELETE CASCADE,
            FOREIGN KEY(schedule_id) REFERENCES policy_schedules(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS policy_app_rules (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'scheduled_block', 'controlled_access')),
            enabled INTEGER NOT NULL DEFAULT 1,
            allowance_id TEXT,
            max_session_minutes INTEGER,
            cooldown_minutes INTEGER,
            max_unlocks_per_hour INTEGER,
            FOREIGN KEY(allowance_id) REFERENCES policy_allowances(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS policy_app_rule_matchers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id TEXT NOT NULL,
            kind TEXT NOT NULL CHECK (kind IN (
                'executable_path',
                'executable_basename',
                'command_name',
                'desktop_id',
                'window_title_exact',
                'window_title_contains'
            )),
            value TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(rule_id) REFERENCES policy_app_rules(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_policy_app_rule_matchers_rule
            ON policy_app_rule_matchers(rule_id, position);

        CREATE TABLE IF NOT EXISTS policy_app_rule_schedules (
            rule_id TEXT NOT NULL,
            schedule_id TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(rule_id, schedule_id),
            FOREIGN KEY(rule_id) REFERENCES policy_app_rules(id) ON DELETE CASCADE,
            FOREIGN KEY(schedule_id) REFERENCES policy_schedules(id) ON DELETE CASCADE
        );
        "#,
    )?;
    migrate_policy_allowances_zero_minutes(conn)?;
    migrate_policy_schedule_windows_day_groups(conn)?;
    migrate_policy_site_list_patterns_url_contains(conn)?;
    migrate_policy_rule_tiers(conn)?;
    initialize_event_totals(conn)?;
    Ok(())
}

fn initialize_event_totals(conn: &Connection) -> Result<(), Error> {
    let initialized = conn
        .query_row(
            "SELECT value FROM service_state WHERE key = 'event_totals_initialized'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some();
    if initialized {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        r#"
        INSERT INTO event_totals (kind, total_count)
        SELECT kind, COUNT(*)
        FROM events
        GROUP BY kind
        ON CONFLICT(kind) DO UPDATE SET
            total_count = excluded.total_count
        "#,
        [],
    )?;
    transaction.execute(
        r#"
        INSERT INTO service_state (key, value, updated_at)
        VALUES ('event_totals_initialized', '1', ?1)
        "#,
        params![format_time(Utc::now())],
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_policy_rule_tiers(conn: &Connection) -> Result<(), Error> {
    let site_table_sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'policy_site_lists'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let app_table_sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'policy_app_rules'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if site_table_sql
        .as_deref()
        .is_none_or(|sql| sql.contains("'scheduled_block'"))
        && app_table_sql
            .as_deref()
            .is_none_or(|sql| sql.contains("'scheduled_block'"))
    {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        CREATE TABLE policy_site_lists_new (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'scheduled_block', 'controlled_access')),
            enabled INTEGER NOT NULL DEFAULT 1,
            allowance_id TEXT,
            max_session_minutes INTEGER,
            cooldown_minutes INTEGER,
            max_unlocks_per_hour INTEGER,
            FOREIGN KEY(allowance_id) REFERENCES policy_allowances(id) ON DELETE SET NULL
        );

        INSERT INTO policy_site_lists_new (
            id, name, tier, enabled, allowance_id,
            max_session_minutes, cooldown_minutes, max_unlocks_per_hour
        )
        SELECT
            id, name, tier, enabled, allowance_id,
            max_session_minutes, cooldown_minutes, max_unlocks_per_hour
        FROM policy_site_lists;

        DROP TABLE policy_site_lists;
        ALTER TABLE policy_site_lists_new RENAME TO policy_site_lists;

        CREATE TABLE policy_app_rules_new (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'scheduled_block', 'controlled_access')),
            enabled INTEGER NOT NULL DEFAULT 1,
            allowance_id TEXT,
            max_session_minutes INTEGER,
            cooldown_minutes INTEGER,
            max_unlocks_per_hour INTEGER,
            FOREIGN KEY(allowance_id) REFERENCES policy_allowances(id) ON DELETE SET NULL
        );

        INSERT INTO policy_app_rules_new (
            id, name, tier, enabled, allowance_id,
            max_session_minutes, cooldown_minutes, max_unlocks_per_hour
        )
        SELECT
            id, name, tier, enabled, allowance_id,
            max_session_minutes, cooldown_minutes, max_unlocks_per_hour
        FROM policy_app_rules;

        DROP TABLE policy_app_rules;
        ALTER TABLE policy_app_rules_new RENAME TO policy_app_rules;

        PRAGMA foreign_keys = ON;
        "#,
    )?;

    Ok(())
}

fn migrate_policy_allowances_zero_minutes(conn: &Connection) -> Result<(), Error> {
    let table_sql: Option<String> = conn
        .query_row(
            r#"
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = 'policy_allowances'
            "#,
            [],
            |row| row.get(0),
        )
        .optional()?;

    let Some(table_sql) = table_sql else {
        return Ok(());
    };

    if !table_sql.contains("daily_minutes > 0") {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        CREATE TABLE policy_allowances_new (
            id TEXT PRIMARY KEY,
            name TEXT,
            daily_minutes INTEGER NOT NULL CHECK (daily_minutes >= 0)
        );

        INSERT INTO policy_allowances_new (id, name, daily_minutes)
        SELECT id, name, daily_minutes
        FROM policy_allowances;

        DROP TABLE policy_allowances;

        ALTER TABLE policy_allowances_new RENAME TO policy_allowances;

        PRAGMA foreign_keys = ON;
        "#,
    )?;

    Ok(())
}

fn migrate_policy_schedule_windows_day_groups(conn: &Connection) -> Result<(), Error> {
    let table_sql: Option<String> = conn
        .query_row(
            r#"
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = 'policy_schedule_windows'
            "#,
            [],
            |row| row.get(0),
        )
        .optional()?;

    let Some(table_sql) = table_sql else {
        return Ok(());
    };

    if table_sql.contains("'everyday'") {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        ALTER TABLE policy_schedule_windows RENAME TO policy_schedule_windows_old;

        CREATE TABLE policy_schedule_windows (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            schedule_id TEXT NOT NULL,
            weekday TEXT NOT NULL CHECK (weekday IN ('everyday', 'workdays', 'weekend', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun')),
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(schedule_id) REFERENCES policy_schedules(id) ON DELETE CASCADE
        );

        INSERT INTO policy_schedule_windows (
            id,
            schedule_id,
            weekday,
            start_time,
            end_time,
            position
        )
        SELECT
            id,
            schedule_id,
            weekday,
            start_time,
            end_time,
            position
        FROM policy_schedule_windows_old;

        DROP TABLE policy_schedule_windows_old;

        CREATE INDEX IF NOT EXISTS idx_policy_schedule_windows_schedule
            ON policy_schedule_windows(schedule_id, position);

        PRAGMA foreign_keys = ON;
        "#,
    )?;

    Ok(())
}

fn migrate_policy_site_list_patterns_url_contains(conn: &Connection) -> Result<(), Error> {
    let table_sql: Option<String> = conn
        .query_row(
            r#"
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = 'policy_site_list_patterns'
            "#,
            [],
            |row| row.get(0),
        )
        .optional()?;

    let Some(table_sql) = table_sql else {
        return Ok(());
    };

    if table_sql.contains("'url_contains'") {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        ALTER TABLE policy_site_list_patterns RENAME TO policy_site_list_patterns_old;

        CREATE TABLE policy_site_list_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            list_id TEXT NOT NULL,
            kind TEXT NOT NULL CHECK (kind IN ('domain', 'exact_url', 'url_prefix', 'url_contains', 'path_prefix')),
            value TEXT NOT NULL,
            match_subdomains INTEGER NOT NULL DEFAULT 0,
            position INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(list_id) REFERENCES policy_site_lists(id) ON DELETE CASCADE
        );

        INSERT INTO policy_site_list_patterns (
            id,
            list_id,
            kind,
            value,
            match_subdomains,
            position
        )
        SELECT
            id,
            list_id,
            kind,
            value,
            match_subdomains,
            position
        FROM policy_site_list_patterns_old;

        DROP TABLE policy_site_list_patterns_old;

        CREATE INDEX IF NOT EXISTS idx_policy_site_list_patterns_list
            ON policy_site_list_patterns(list_id, position);

        PRAGMA foreign_keys = ON;
        "#,
    )?;

    Ok(())
}

pub(crate) fn format_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

pub(crate) fn parse_time(value: &str) -> Result<DateTime<Utc>, Error> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn schedule_active_seconds_between(
    schedule: &ScheduleConfig,
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
) -> i64 {
    if end <= start {
        return 0;
    }

    let mut date = start
        .date_naive()
        .pred_opt()
        .unwrap_or_else(|| start.date_naive());
    let final_date = end.date_naive();
    let mut total_seconds = 0_i64;

    while date <= final_date {
        let weekday = Weekday::from(date.weekday());
        let mut intervals = Vec::new();
        for window in &schedule.windows {
            if !window.weekday.includes(weekday) {
                continue;
            }

            let window_start = start
                .offset()
                .with_ymd_and_hms(
                    date.year(),
                    date.month(),
                    date.day(),
                    u32::from(window.start.hour()),
                    u32::from(window.start.minute()),
                    0,
                )
                .single()
                .expect("fixed offsets always resolve local times");
            let window_end_date = if window.start < window.end {
                date
            } else {
                date.succ_opt()
                    .expect("schedule dates remain representable")
            };
            let window_end = start
                .offset()
                .with_ymd_and_hms(
                    window_end_date.year(),
                    window_end_date.month(),
                    window_end_date.day(),
                    u32::from(window.end.hour()),
                    u32::from(window.end.minute()),
                    0,
                )
                .single()
                .expect("fixed offsets always resolve local times");

            let overlap_start = std::cmp::max(window_start, start);
            let overlap_end = std::cmp::min(window_end, end);
            if overlap_end > overlap_start {
                intervals.push((overlap_start, overlap_end));
            }
        }

        intervals.sort_by_key(|(interval_start, _)| *interval_start);
        let mut active_interval: Option<(DateTime<FixedOffset>, DateTime<FixedOffset>)> = None;
        for (interval_start, interval_end) in intervals {
            match active_interval {
                Some((active_start, active_end)) if interval_start <= active_end => {
                    active_interval = Some((active_start, std::cmp::max(active_end, interval_end)));
                }
                Some((active_start, active_end)) => {
                    total_seconds =
                        total_seconds.saturating_add((active_end - active_start).num_seconds());
                    active_interval = Some((interval_start, interval_end));
                }
                None => active_interval = Some((interval_start, interval_end)),
            }
        }
        if let Some((active_start, active_end)) = active_interval {
            total_seconds = total_seconds.saturating_add((active_end - active_start).num_seconds());
        }

        let Some(next_date) = date.succ_opt() else {
            break;
        };
        date = next_date;
    }

    total_seconds
}

fn to_u32(label: &str, value: i64) -> Result<u32, Error> {
    value.try_into().map_err(|_| {
        ConfigError::Validation(format!(
            "{label} value {value} is outside the supported range"
        ))
        .into()
    })
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn rule_tier_to_str(value: RuleTier) -> &'static str {
    match value {
        RuleTier::Hard => "hard",
        RuleTier::ScheduledBlock => "scheduled_block",
        RuleTier::ControlledAccess => "controlled_access",
    }
}

fn rule_tier_from_str(value: &str) -> Result<RuleTier, Error> {
    match value {
        "hard" => Ok(RuleTier::Hard),
        "scheduled_block" => Ok(RuleTier::ScheduledBlock),
        "controlled_access" => Ok(RuleTier::ControlledAccess),
        _ => Err(ConfigError::Validation(format!("unknown rule tier '{value}'")).into()),
    }
}

fn pattern_kind_to_str(value: RulePatternKind) -> &'static str {
    match value {
        RulePatternKind::Domain => "domain",
        RulePatternKind::ExactUrl => "exact_url",
        RulePatternKind::UrlPrefix => "url_prefix",
        RulePatternKind::UrlContains => "url_contains",
        RulePatternKind::PathPrefix => "path_prefix",
    }
}

fn pattern_kind_from_str(value: &str) -> Result<RulePatternKind, Error> {
    match value {
        "domain" => Ok(RulePatternKind::Domain),
        "exact_url" => Ok(RulePatternKind::ExactUrl),
        "url_prefix" => Ok(RulePatternKind::UrlPrefix),
        "url_contains" => Ok(RulePatternKind::UrlContains),
        "path_prefix" => Ok(RulePatternKind::PathPrefix),
        _ => Err(ConfigError::Validation(format!("unknown rule pattern kind '{value}'")).into()),
    }
}

fn app_matcher_kind_to_str(value: AppMatcherKind) -> &'static str {
    match value {
        AppMatcherKind::ExecutablePath => "executable_path",
        AppMatcherKind::ExecutableBasename => "executable_basename",
        AppMatcherKind::CommandName => "command_name",
        AppMatcherKind::DesktopId => "desktop_id",
        AppMatcherKind::WindowTitleExact => "window_title_exact",
        AppMatcherKind::WindowTitleContains => "window_title_contains",
    }
}

fn app_matcher_kind_from_str(value: &str) -> Result<AppMatcherKind, Error> {
    match value {
        "executable_path" => Ok(AppMatcherKind::ExecutablePath),
        "executable_basename" => Ok(AppMatcherKind::ExecutableBasename),
        "command_name" => Ok(AppMatcherKind::CommandName),
        "desktop_id" => Ok(AppMatcherKind::DesktopId),
        "window_title_exact" => Ok(AppMatcherKind::WindowTitleExact),
        "window_title_contains" => Ok(AppMatcherKind::WindowTitleContains),
        _ => Err(ConfigError::Validation(format!("unknown app matcher kind '{value}'")).into()),
    }
}

fn schedule_day_to_str(value: ScheduleDay) -> &'static str {
    match value {
        ScheduleDay::Everyday => "everyday",
        ScheduleDay::Workdays => "workdays",
        ScheduleDay::Weekend => "weekend",
        ScheduleDay::Mon => "mon",
        ScheduleDay::Tue => "tue",
        ScheduleDay::Wed => "wed",
        ScheduleDay::Thu => "thu",
        ScheduleDay::Fri => "fri",
        ScheduleDay::Sat => "sat",
        ScheduleDay::Sun => "sun",
    }
}

fn schedule_day_from_str(value: &str) -> Result<ScheduleDay, Error> {
    match value {
        "everyday" => Ok(ScheduleDay::Everyday),
        "workdays" => Ok(ScheduleDay::Workdays),
        "weekend" => Ok(ScheduleDay::Weekend),
        "mon" => Ok(ScheduleDay::Mon),
        "tue" => Ok(ScheduleDay::Tue),
        "wed" => Ok(ScheduleDay::Wed),
        "thu" => Ok(ScheduleDay::Thu),
        "fri" => Ok(ScheduleDay::Fri),
        "sat" => Ok(ScheduleDay::Sat),
        "sun" => Ok(ScheduleDay::Sun),
        _ => Err(ConfigError::Validation(format!("unknown schedule day '{value}'")).into()),
    }
}

fn detox_session_base_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DetoxSession> {
    let starts_at: String = row.get(2)?;
    let ends_at: String = row.get(3)?;
    let cancelled_at: Option<String> = row.get(4)?;

    let starts_at = DateTime::parse_from_rfc3339(&starts_at)
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(err))
        })?
        .with_timezone(&Utc);
    let ends_at = DateTime::parse_from_rfc3339(&ends_at)
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(err))
        })?
        .with_timezone(&Utc);
    let cancelled_at = match cancelled_at {
        Some(cancelled_at) => Some(
            DateTime::parse_from_rfc3339(&cancelled_at)
                .map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?
                .with_timezone(&Utc),
        ),
        None => None,
    };

    Ok(DetoxSession {
        id: row.get(0)?,
        name: row.get(1)?,
        starts_at,
        ends_at,
        cancelled_at,
        site_rule_ids: Vec::new(),
        app_rule_ids: Vec::new(),
    })
}

fn unlock_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UnlockState> {
    let started_at: String = row.get(5)?;
    let expires_at: String = row.get(6)?;
    let minutes: i64 = row.get(3)?;

    Ok(UnlockState {
        id: row.get(0)?,
        target: row.get(1)?,
        rule_id: row.get(2)?,
        minutes: minutes.try_into().unwrap_or(u32::MAX),
        reason: row.get(4)?,
        started_at: DateTime::parse_from_rfc3339(&started_at)
            .map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?
            .with_timezone(&Utc),
        expires_at: DateTime::parse_from_rfc3339(&expires_at)
            .map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?
            .with_timezone(&Utc),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(value: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(value).expect("timestamp should parse")
    }

    #[test]
    fn records_events_in_plain_text_log() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let log_path = directory.path().join("blockuntu.log");
        let mut database = Database::in_memory().expect("in-memory database");
        database
            .set_event_log_path(&log_path)
            .expect("event log setup");

        database
            .record_event(
                "website_blocked",
                Some("https://example.com"),
                Some("line one\nline two"),
                Utc::now(),
            )
            .expect("event record");

        let contents = std::fs::read_to_string(log_path).expect("event log contents");
        assert!(contents.contains("kind=\"website_blocked\""));
        assert!(contents.contains("target=Some(\"https://example.com\")"));
        assert!(contents.contains("details=Some(\"line one\\nline two\")"));
    }

    #[test]
    fn thirty_day_retention_keeps_all_time_event_totals() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let log_path = directory.path().join("blockuntu.log");
        let mut database = Database::in_memory().expect("in-memory database");
        database
            .set_event_log_path(&log_path)
            .expect("event log setup");
        let now = Utc
            .with_ymd_and_hms(2026, 8, 14, 12, 0, 0)
            .single()
            .expect("timestamp");

        database
            .record_event(
                "website_blocked",
                Some("old.example"),
                None,
                now - Duration::days(31),
            )
            .expect("old event");
        database
            .record_event(
                "website_blocked",
                Some("recent.example"),
                None,
                now - Duration::days(2),
            )
            .expect("recent event");

        assert_eq!(
            database
                .enforce_event_retention(now, Duration::days(30))
                .expect("retention should run"),
            1
        );
        let summary = database.event_summary().expect("summary should load");
        assert_eq!(summary.total_events, 2);
        assert_eq!(summary.event_counts["website_blocked"], 2);

        let snapshot = database.event_log_snapshot().expect("snapshot should load");
        assert!(!snapshot.contains("old.example"));
        assert!(snapshot.contains("recent.example"));
        assert_eq!(
            std::fs::read_to_string(&log_path).expect("retained log should load"),
            snapshot
        );
    }

    #[test]
    fn migration_backfills_existing_events_into_all_time_totals_once() {
        let connection = Connection::open_in_memory().expect("database should open");
        connection
            .execute_batch(
                r#"
                CREATE TABLE events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    kind TEXT NOT NULL,
                    target TEXT,
                    details TEXT,
                    created_at TEXT NOT NULL
                );
                INSERT INTO events (kind, created_at) VALUES
                    ('website_blocked', '2026-07-01T10:00:00Z'),
                    ('website_blocked', '2026-07-02T10:00:00Z'),
                    ('app_blocked', '2026-07-03T10:00:00Z');
                "#,
            )
            .expect("legacy events should be created");
        migrate_database(&connection).expect("migration should succeed");
        migrate_database(&connection).expect("repeat migration should succeed");
        let database = Database {
            conn: connection,
            event_log_path: None,
        };

        let summary = database.event_summary().expect("summary should load");
        assert_eq!(summary.total_events, 3);
        assert_eq!(summary.event_counts["website_blocked"], 2);
        assert_eq!(summary.event_counts["app_blocked"], 1);
    }

    #[test]
    fn notification_preferences_default_and_roundtrip() {
        let database = Database::in_memory().expect("in-memory database");
        assert_eq!(
            database
                .notification_preferences()
                .expect("default preferences"),
            NotificationPreferences::default()
        );

        let preferences = NotificationPreferences {
            enabled: true,
            website_blocked: false,
            application_blocked: true,
            allowance_warnings: true,
            allowance_warning_minutes: vec![10, 5, 1],
            schedule_started: false,
            schedule_ended: true,
            detox_started: true,
            detox_ended: false,
        };
        database
            .set_notification_preferences(&preferences)
            .expect("preferences should save");

        assert_eq!(
            database
                .notification_preferences()
                .expect("saved preferences"),
            preferences
        );
    }

    #[test]
    fn notification_outbox_deduplicates_acknowledges_and_expires() {
        let database = Database::in_memory().expect("in-memory database");
        let now = Utc
            .with_ymd_and_hms(2026, 7, 24, 10, 0, 0)
            .single()
            .expect("timestamp");
        let first = database
            .enqueue_notification(
                "website_blocked",
                "Website blocked",
                "example.com was blocked.",
                "website:example.com",
                now,
                Duration::minutes(1),
                Duration::minutes(2),
            )
            .expect("notification should enqueue");
        assert!(first.is_some());
        assert_eq!(
            database
                .enqueue_notification(
                    "website_blocked",
                    "Website blocked",
                    "example.com was blocked.",
                    "website:example.com",
                    now + Duration::seconds(30),
                    Duration::minutes(1),
                    Duration::minutes(2),
                )
                .expect("duplicate should be handled"),
            None
        );

        let pending = database
            .pending_notifications(now, 20)
            .expect("pending notifications");
        assert_eq!(pending.len(), 1);
        database
            .acknowledge_notifications(&[pending[0].id], now)
            .expect("notification should acknowledge");
        assert!(database
            .pending_notifications(now, 20)
            .expect("pending notifications")
            .is_empty());

        database
            .enqueue_notification(
                "schedule_started",
                "Schedule started",
                "Work is active.",
                "schedule:work",
                now,
                Duration::seconds(0),
                Duration::minutes(1),
            )
            .expect("expiring notification should enqueue");
        assert!(database
            .pending_notifications(now + Duration::minutes(2), 20)
            .expect("expired notifications should clean up")
            .is_empty());
    }

    #[test]
    fn notification_delivery_results_are_logged_and_failures_retry_later() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let log_path = directory.path().join("blockuntu.log");
        let mut database = Database::in_memory().expect("in-memory database");
        database
            .set_event_log_path(&log_path)
            .expect("event log setup");
        let now = Utc
            .with_ymd_and_hms(2026, 7, 25, 10, 0, 0)
            .single()
            .expect("timestamp");
        let id = database
            .enqueue_notification(
                "website_blocked",
                "Website blocked",
                "example.com was blocked.",
                "website:example.com",
                now,
                Duration::minutes(1),
                Duration::minutes(5),
            )
            .expect("notification should enqueue")
            .expect("notification id");

        assert!(database
            .record_notification_delivery_result(
                id,
                false,
                Some("org.freedesktop.Notifications is unavailable"),
                now,
            )
            .expect("failed delivery should record"));
        assert!(database
            .pending_notifications(now + Duration::seconds(30), 20)
            .expect("retry should be delayed")
            .is_empty());
        assert_eq!(
            database
                .pending_notifications(now + Duration::seconds(61), 20)
                .expect("retry should become available")
                .len(),
            1
        );

        assert!(database
            .record_notification_delivery_result(
                id,
                true,
                Some("accepted by desktop notification service"),
                now + Duration::seconds(61),
            )
            .expect("successful delivery should record"));
        assert!(database
            .pending_notifications(now + Duration::seconds(62), 20)
            .expect("delivered notification should be acknowledged")
            .is_empty());

        let contents = std::fs::read_to_string(log_path).expect("event log contents");
        assert!(contents.contains("kind=\"notification_queued\""));
        assert!(contents.contains("kind=\"notification_delivery_failed\""));
        assert!(contents.contains("org.freedesktop.Notifications is unavailable"));
        assert!(contents.contains("kind=\"notification_accepted\""));
        assert!(contents.contains("attempt=2"));
    }

    #[test]
    fn disabling_notification_categories_discards_pending_events() {
        let database = Database::in_memory().expect("in-memory database");
        let now = Utc
            .with_ymd_and_hms(2026, 7, 24, 10, 0, 0)
            .single()
            .expect("timestamp");
        database
            .enqueue_notification(
                "website_blocked",
                "Website blocked",
                "example.com was blocked.",
                "website:example.com",
                now,
                Duration::seconds(0),
                Duration::minutes(2),
            )
            .expect("website notification should enqueue");
        database
            .enqueue_notification(
                "schedule_started",
                "Schedule started",
                "Work is active.",
                "schedule:work",
                now,
                Duration::seconds(0),
                Duration::minutes(2),
            )
            .expect("schedule notification should enqueue");

        let preferences = NotificationPreferences {
            website_blocked: false,
            ..NotificationPreferences::default()
        };
        database
            .discard_disabled_notifications(&preferences, now)
            .expect("disabled notifications should discard");
        let pending = database
            .pending_notifications(now, 20)
            .expect("pending notifications");

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, "schedule_started");
    }

    #[test]
    fn schedule_activity_totals_accumulate_active_time_without_double_counting_overlaps() {
        let database = Database::in_memory().expect("in-memory database");
        let schedule = ScheduleConfig {
            id: "work-hours".to_string(),
            name: Some("Work hours".to_string()),
            windows: vec![
                ScheduleWindow {
                    weekday: ScheduleDay::Workdays,
                    start: TimeOfDay::new(9, 0).unwrap(),
                    end: TimeOfDay::new(17, 0).unwrap(),
                },
                ScheduleWindow {
                    weekday: ScheduleDay::Mon,
                    start: TimeOfDay::new(12, 0).unwrap(),
                    end: TimeOfDay::new(13, 0).unwrap(),
                },
            ],
        };

        database
            .sync_schedule_activity_totals(
                &[schedule.clone()],
                timestamp("2026-07-13T08:00:00+02:00"),
            )
            .expect("initial schedule activity sync");
        let totals = database
            .sync_schedule_activity_totals(&[schedule], timestamp("2026-07-14T10:00:00+02:00"))
            .expect("follow-up schedule activity sync");

        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].schedule_id, "work-hours");
        assert_eq!(totals[0].total_active_seconds, 9 * 60 * 60);
    }

    #[test]
    fn schedule_activity_totals_include_overnight_windows_started_on_the_previous_day() {
        let database = Database::in_memory().expect("in-memory database");
        let schedule = ScheduleConfig {
            id: "late-night".to_string(),
            name: None,
            windows: vec![ScheduleWindow {
                weekday: ScheduleDay::Fri,
                start: TimeOfDay::new(22, 0).unwrap(),
                end: TimeOfDay::new(2, 0).unwrap(),
            }],
        };

        database
            .sync_schedule_activity_totals(
                &[schedule.clone()],
                timestamp("2026-07-17T21:00:00+02:00"),
            )
            .expect("initial schedule activity sync");
        let totals = database
            .sync_schedule_activity_totals(&[schedule], timestamp("2026-07-18T03:00:00+02:00"))
            .expect("follow-up schedule activity sync");

        assert_eq!(totals[0].total_active_seconds, 4 * 60 * 60);
    }
}
