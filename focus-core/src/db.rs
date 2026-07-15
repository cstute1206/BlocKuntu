use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    AllowanceConfig, AppMatcherConfig, AppMatcherKind, AppRuleConfig, Config, ConfigError,
    DetoxSession, Error, RuleConfig, RulePatternConfig, RulePatternKind, RuleTier, ScheduleConfig,
    ScheduleDay, ScheduleWindow, StrictModeConfig, TimeOfDay, UnlockState, VisitState,
};

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
pub(crate) struct VisitUsage {
    pub rule_id: Option<String>,
    pub url: String,
    pub started_at: DateTime<Utc>,
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
        self.conn.execute(
            "INSERT INTO events (kind, target, details, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![kind, target, details, created_at],
        )?;
        let id = self.conn.last_insert_rowid();

        if let Some(path) = &self.event_log_path {
            if let Err(error) = append_event_log(path, &created_at, kind, target, details) {
                eprintln!("BlocKuntu could not append to {}: {error}", path.display());
            }
        }

        Ok(id)
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

    pub(crate) fn used_seconds_for_app_rule_on_day(
        &self,
        rule_id: &str,
        now: DateTime<Utc>,
    ) -> Result<i64, Error> {
        let day_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid")
            .and_utc();
        let day_end = day_start + Duration::days(1);

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
                format_time(day_end),
                format_time(now),
                format_time(day_start)
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

            let clamped_start = started_at.max(day_start);
            let clamped_end = ended_at.min(now).min(day_end);
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

    pub(crate) fn visit_usage_for_day(&self, now: DateTime<Utc>) -> Result<Vec<VisitUsage>, Error> {
        let day_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid")
            .and_utc();
        let day_end = day_start + Duration::days(1);

        let mut statement = self.conn.prepare(
            r#"
            SELECT rule_id, url, started_at, ended_at
            FROM visits
            WHERE started_at < ?1
              AND COALESCE(ended_at, ?2) > ?3
            "#,
        )?;

        let rows = statement.query_map(
            params![
                format_time(day_end),
                format_time(now),
                format_time(day_start)
            ],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )?;

        let mut visits = Vec::new();
        for row in rows {
            let (rule_id, url, started_at, ended_at) = row?;
            visits.push(VisitUsage {
                rule_id,
                url,
                started_at: parse_time(&started_at)?,
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
    writeln!(
        file,
        "{created_at} kind={kind:?} target={target:?} details={details:?}"
    )?;
    file.sync_data()
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
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'controlled_access')),
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
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'controlled_access')),
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
        RuleTier::ControlledAccess => "controlled_access",
    }
}

fn rule_tier_from_str(value: &str) -> Result<RuleTier, Error> {
    match value {
        "hard" => Ok(RuleTier::Hard),
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
}
