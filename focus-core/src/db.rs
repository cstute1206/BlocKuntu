use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    AllowanceConfig, AppMatcherConfig, AppMatcherKind, AppRuleConfig, Config, ConfigError,
    DefaultsConfig, Error, RuleConfig, RulePatternConfig, RulePatternKind, RuleTier,
    ScheduleConfig, ScheduleWindow, TimeOfDay, UnlockPolicyConfig, UnlockState, VisitState,
    Weekday,
};

pub struct Database {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatState {
    pub component: String,
    pub last_seen_at: DateTime<Utc>,
    pub details: Option<String>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let conn = Connection::open(path)?;
        migrate_database(&conn)?;
        Ok(Self { conn })
    }

    pub fn in_memory() -> Result<Self, Error> {
        let conn = Connection::open_in_memory()?;
        migrate_database(&conn)?;
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn record_event(
        &self,
        kind: &str,
        target: Option<&str>,
        details: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO events (kind, target, details, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![kind, target, details, format_time(now)],
        )?;
        Ok(self.conn.last_insert_rowid())
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
                (SELECT COUNT(*) FROM policy_allowances)
            "#,
            [],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn load_policy_config(&self) -> Result<Config, Error> {
        let defaults = self.load_policy_defaults()?;
        let allowances = self.load_policy_allowances()?;
        let schedules = self.load_policy_schedules()?;
        let rules = self.load_policy_site_lists()?;
        let app_rules = self.load_policy_app_rules()?;

        let config = Config {
            rules,
            app_rules,
            schedules,
            allowances,
            defaults,
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
            params![
                i64::from(config.defaults.unlock_policy.max_session_minutes),
                i64::from(config.defaults.unlock_policy.cooldown_minutes),
                i64::from(config.defaults.unlock_policy.max_unlocks_per_hour),
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
                        weekday_to_str(window.weekday),
                        window.start.to_string(),
                        window.end.to_string(),
                        position as i64,
                    ],
                )?;
            }
        }

        for rule in &config.rules {
            let unlock_policy = rule.unlock_policy;
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
                    unlock_policy.map(|policy| i64::from(policy.max_session_minutes)),
                    unlock_policy.map(|policy| i64::from(policy.cooldown_minutes)),
                    unlock_policy.map(|policy| i64::from(policy.max_unlocks_per_hour)),
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
            let unlock_policy = rule.unlock_policy;
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
                    unlock_policy.map(|policy| i64::from(policy.max_session_minutes)),
                    unlock_policy.map(|policy| i64::from(policy.cooldown_minutes)),
                    unlock_policy.map(|policy| i64::from(policy.max_unlocks_per_hour)),
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

    fn load_policy_defaults(&self) -> Result<DefaultsConfig, Error> {
        let row = self
            .conn
            .query_row(
                r#"
                SELECT max_session_minutes, cooldown_minutes, max_unlocks_per_hour
                FROM policy_defaults
                WHERE key = 1
                "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((max_session_minutes, cooldown_minutes, max_unlocks_per_hour)) = row else {
            return Ok(DefaultsConfig::default());
        };

        Ok(DefaultsConfig {
            unlock_policy: UnlockPolicyConfig {
                max_session_minutes: to_u32("defaults.max_session_minutes", max_session_minutes)?,
                cooldown_minutes: to_u32("defaults.cooldown_minutes", cooldown_minutes)?,
                max_unlocks_per_hour: to_u32(
                    "defaults.max_unlocks_per_hour",
                    max_unlocks_per_hour,
                )?,
            },
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
                weekday: weekday_from_str(&weekday)?,
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
                allowance_id,
                max_session_minutes,
                cooldown_minutes,
                max_unlocks_per_hour
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
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;

        let mut rules = Vec::new();
        for row in rows {
            let (
                id,
                name,
                tier,
                enabled,
                allowance_id,
                max_session_minutes,
                cooldown_minutes,
                max_unlocks_per_hour,
            ) = row?;
            rules.push(RuleConfig {
                patterns: self.load_policy_site_list_patterns(&id)?,
                schedule_ids: self.load_policy_site_list_schedule_ids(&id)?,
                id,
                name,
                tier: rule_tier_from_str(&tier)?,
                enabled: enabled != 0,
                allowance_id,
                unlock_policy: optional_unlock_policy(
                    max_session_minutes,
                    cooldown_minutes,
                    max_unlocks_per_hour,
                )?,
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
                allowance_id,
                max_session_minutes,
                cooldown_minutes,
                max_unlocks_per_hour
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
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;

        let mut app_rules = Vec::new();
        for row in rows {
            let (
                id,
                name,
                tier,
                enabled,
                allowance_id,
                max_session_minutes,
                cooldown_minutes,
                max_unlocks_per_hour,
            ) = row?;
            app_rules.push(AppRuleConfig {
                matchers: self.load_policy_app_rule_matchers(&id)?,
                schedule_ids: self.load_policy_app_rule_schedule_ids(&id)?,
                id,
                name,
                tier: rule_tier_from_str(&tier)?,
                enabled: enabled != 0,
                allowance_id,
                unlock_policy: optional_unlock_policy(
                    max_session_minutes,
                    cooldown_minutes,
                    max_unlocks_per_hour,
                )?,
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

    pub(crate) fn latest_unlock_for_rule(
        &self,
        rule_id: &str,
    ) -> Result<Option<UnlockState>, Error> {
        self.conn
            .query_row(
                r#"
                SELECT id, target, rule_id, minutes, reason, started_at, expires_at
                FROM unlocks
                WHERE rule_id = ?1
                ORDER BY expires_at DESC
                LIMIT 1
                "#,
                params![rule_id],
                unlock_from_row,
            )
            .optional()
            .map_err(Error::from)
    }

    pub(crate) fn count_unlocks_since(
        &self,
        rule_id: &str,
        since: DateTime<Utc>,
    ) -> Result<u32, Error> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM unlocks WHERE rule_id = ?1 AND started_at >= ?2",
            params![rule_id, format_time(since)],
            |row| row.get(0),
        )?;
        Ok(count.try_into().unwrap_or(u32::MAX))
    }

    pub(crate) fn used_seconds_for_rule_on_day(
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
            FROM visits
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

pub fn migrate_database(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS rules (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            tier TEXT NOT NULL CHECK (tier IN ('hard', 'controlled_access')),
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

        CREATE TABLE IF NOT EXISTS rule_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            value TEXT NOT NULL,
            match_subdomains INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(rule_id) REFERENCES rules(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS apps (
            id TEXT PRIMARY KEY,
            rule_id TEXT,
            name TEXT NOT NULL,
            executable_path TEXT,
            command_name TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            FOREIGN KEY(rule_id) REFERENCES rules(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS schedules (
            id TEXT PRIMARY KEY,
            rule_id TEXT,
            weekday INTEGER,
            start_minute INTEGER,
            end_minute INTEGER,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            FOREIGN KEY(rule_id) REFERENCES rules(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS allowances (
            id TEXT PRIMARY KEY,
            rule_id TEXT,
            daily_minutes INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            FOREIGN KEY(rule_id) REFERENCES rules(id) ON DELETE CASCADE
        );

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

        CREATE TABLE IF NOT EXISTS policy_defaults (
            key INTEGER PRIMARY KEY CHECK (key = 1),
            max_session_minutes INTEGER NOT NULL,
            cooldown_minutes INTEGER NOT NULL,
            max_unlocks_per_hour INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS policy_allowances (
            id TEXT PRIMARY KEY,
            name TEXT,
            daily_minutes INTEGER NOT NULL CHECK (daily_minutes > 0)
        );

        CREATE TABLE IF NOT EXISTS policy_schedules (
            id TEXT PRIMARY KEY,
            name TEXT
        );

        CREATE TABLE IF NOT EXISTS policy_schedule_windows (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            schedule_id TEXT NOT NULL,
            weekday TEXT NOT NULL CHECK (weekday IN ('mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun')),
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
            kind TEXT NOT NULL CHECK (kind IN ('domain', 'exact_url', 'url_prefix', 'path_prefix')),
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

fn optional_unlock_policy(
    max_session_minutes: Option<i64>,
    cooldown_minutes: Option<i64>,
    max_unlocks_per_hour: Option<i64>,
) -> Result<Option<UnlockPolicyConfig>, Error> {
    match (max_session_minutes, cooldown_minutes, max_unlocks_per_hour) {
        (None, None, None) => Ok(None),
        (Some(max_session_minutes), Some(cooldown_minutes), Some(max_unlocks_per_hour)) => {
            Ok(Some(UnlockPolicyConfig {
                max_session_minutes: to_u32(
                    "rule.unlock_policy.max_session_minutes",
                    max_session_minutes,
                )?,
                cooldown_minutes: to_u32("rule.unlock_policy.cooldown_minutes", cooldown_minutes)?,
                max_unlocks_per_hour: to_u32(
                    "rule.unlock_policy.max_unlocks_per_hour",
                    max_unlocks_per_hour,
                )?,
            }))
        }
        _ => Err(ConfigError::Validation(
            "rule unlock policy columns must be all set or all null".to_string(),
        )
        .into()),
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
        RulePatternKind::PathPrefix => "path_prefix",
    }
}

fn pattern_kind_from_str(value: &str) -> Result<RulePatternKind, Error> {
    match value {
        "domain" => Ok(RulePatternKind::Domain),
        "exact_url" => Ok(RulePatternKind::ExactUrl),
        "url_prefix" => Ok(RulePatternKind::UrlPrefix),
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

fn weekday_to_str(value: Weekday) -> &'static str {
    match value {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}

fn weekday_from_str(value: &str) -> Result<Weekday, Error> {
    match value {
        "mon" => Ok(Weekday::Mon),
        "tue" => Ok(Weekday::Tue),
        "wed" => Ok(Weekday::Wed),
        "thu" => Ok(Weekday::Thu),
        "fri" => Ok(Weekday::Fri),
        "sat" => Ok(Weekday::Sat),
        "sun" => Ok(Weekday::Sun),
        _ => Err(ConfigError::Validation(format!("unknown schedule weekday '{value}'")).into()),
    }
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
