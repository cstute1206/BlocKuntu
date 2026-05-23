use std::path::Path;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{Error, UnlockState, VisitState};

pub struct Database {
    conn: Connection,
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
