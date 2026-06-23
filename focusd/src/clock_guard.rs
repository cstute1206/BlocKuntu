use std::fs;

use chrono::{DateTime, Duration, FixedOffset, Local, Utc};
use focus_core::Database;
use serde::Serialize;

use crate::error::{DaemonError, Result};

const STATUS_KEY: &str = "clock_guard.status";
const DETAIL_KEY: &str = "clock_guard.detail";
const LAST_WALL_UTC_KEY: &str = "clock_guard.last_wall_utc";
const LAST_BOOT_ID_KEY: &str = "clock_guard.last_boot_id";
const LAST_BOOTTIME_SECONDS_KEY: &str = "clock_guard.last_boottime_seconds";
const STATUS_OK: &str = "ok";
const STATUS_TAMPERED: &str = "tampered";
const BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
const UPTIME_PATH: &str = "/proc/uptime";
const MAX_WALL_DRIFT_SECONDS: i64 = 300;
const MAX_BACKWARD_REBOOT_SECONDS: i64 = 300;

#[derive(Debug, Clone, Serialize)]
pub struct ClockIntegrityStatus {
    pub state: String,
    pub detail: String,
    pub checked_at: DateTime<Utc>,
    pub last_wall_utc: Option<DateTime<Utc>>,
    pub boot_id: String,
    pub boottime_seconds: f64,
}

#[derive(Debug, Clone)]
pub struct GuardedNow {
    pub now: DateTime<FixedOffset>,
    pub integrity: ClockIntegrityStatus,
}

pub fn guarded_now(
    database: &Database,
    client_now: Option<&str>,
    trust_client_now: bool,
) -> Result<GuardedNow> {
    if trust_client_now {
        let now = parse_optional_now(client_now)?;
        return Ok(GuardedNow {
            now,
            integrity: ClockIntegrityStatus {
                state: STATUS_OK.to_string(),
                detail: "Client-supplied test time is trusted for this context.".to_string(),
                checked_at: now.with_timezone(&Utc),
                last_wall_utc: None,
                boot_id: "test".to_string(),
                boottime_seconds: 0.0,
            },
        });
    }

    let now = Local::now().fixed_offset();
    let integrity = update_clock_integrity(database, now.with_timezone(&Utc))?;

    Ok(GuardedNow { now, integrity })
}

pub fn status(database: &Database) -> Result<ClockIntegrityStatus> {
    let now_utc = Utc::now();
    let sample = current_sample(now_utc)?;
    let state = database
        .service_state(STATUS_KEY)?
        .unwrap_or_else(|| STATUS_OK.to_string());
    let detail = database
        .service_state(DETAIL_KEY)?
        .unwrap_or_else(|| "Clock baseline has not detected suspicious movement.".to_string());
    let last_wall_utc = database
        .service_state(LAST_WALL_UTC_KEY)?
        .as_deref()
        .map(parse_utc)
        .transpose()?;

    Ok(ClockIntegrityStatus {
        state,
        detail,
        checked_at: now_utc,
        last_wall_utc,
        boot_id: sample.boot_id,
        boottime_seconds: sample.boottime_seconds,
    })
}

pub fn ensure_trusted(database: &Database) -> Result<()> {
    let current = status(database)?;
    if current.state == STATUS_TAMPERED {
        return Err(clock_tampered_error(&current.detail));
    }
    Ok(())
}

pub fn is_tampered(database: &Database) -> Result<bool> {
    Ok(status(database)?.state == STATUS_TAMPERED)
}

pub fn parse_optional_now(now: Option<&str>) -> Result<DateTime<FixedOffset>> {
    match now {
        Some(now) => DateTime::parse_from_rfc3339(now)
            .map(|parsed| parsed.with_timezone(&Local).fixed_offset())
            .map_err(|err| DaemonError::InvalidRequest(format!("invalid RFC3339 now: {err}"))),
        None => Ok(Local::now().fixed_offset()),
    }
}

fn update_clock_integrity(
    database: &Database,
    now_utc: DateTime<Utc>,
) -> Result<ClockIntegrityStatus> {
    let sample = current_sample(now_utc)?;
    let previous_state = database
        .service_state(STATUS_KEY)?
        .unwrap_or_else(|| STATUS_OK.to_string());
    let previous_detail = database.service_state(DETAIL_KEY)?.unwrap_or_default();
    let previous_wall = database
        .service_state(LAST_WALL_UTC_KEY)?
        .as_deref()
        .map(parse_utc)
        .transpose()?;
    let previous_boot_id = database.service_state(LAST_BOOT_ID_KEY)?;
    let previous_boottime = database
        .service_state(LAST_BOOTTIME_SECONDS_KEY)?
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok());

    let violation = clock_violation(
        &sample,
        previous_wall,
        previous_boot_id.as_deref(),
        previous_boottime,
    );
    let (state, detail) = match (previous_state.as_str(), violation) {
        (STATUS_TAMPERED, Some(violation)) => (STATUS_TAMPERED.to_string(), violation),
        (STATUS_TAMPERED, None) => (
            STATUS_TAMPERED.to_string(),
            if previous_detail.is_empty() {
                "System clock tampering was detected previously.".to_string()
            } else {
                previous_detail
            },
        ),
        (_, Some(violation)) => {
            let _ = database.record_event(
                "clock_tamper_detected",
                Some("system_clock"),
                Some(&violation),
                sample.wall_utc,
            );
            (STATUS_TAMPERED.to_string(), violation)
        }
        _ => (
            STATUS_OK.to_string(),
            "Clock movement matches monotonic system uptime.".to_string(),
        ),
    };

    persist_sample(database, &sample, &state, &detail)?;

    Ok(ClockIntegrityStatus {
        state,
        detail,
        checked_at: sample.wall_utc,
        last_wall_utc: previous_wall,
        boot_id: sample.boot_id,
        boottime_seconds: sample.boottime_seconds,
    })
}

fn clock_violation(
    sample: &ClockSample,
    previous_wall: Option<DateTime<Utc>>,
    previous_boot_id: Option<&str>,
    previous_boottime: Option<f64>,
) -> Option<String> {
    let previous_wall = previous_wall?;
    let previous_boot_id = previous_boot_id?;
    let previous_boottime = previous_boottime?;

    if previous_boot_id != sample.boot_id {
        let backward = (previous_wall - sample.wall_utc).num_seconds();
        if backward > MAX_BACKWARD_REBOOT_SECONDS {
            return Some(format!(
                "System clock moved backwards by {backward}s across reboot."
            ));
        }
        return None;
    }

    let elapsed_seconds = sample.boottime_seconds - previous_boottime;
    if elapsed_seconds < -1.0 {
        return Some("System monotonic uptime moved backwards.".to_string());
    }

    let expected_wall = previous_wall + Duration::milliseconds((elapsed_seconds * 1000.0) as i64);
    let drift_seconds = (sample.wall_utc - expected_wall).num_seconds();
    if drift_seconds.abs() > MAX_WALL_DRIFT_SECONDS {
        return Some(format!(
            "System clock drifted by {drift_seconds}s while monotonic uptime advanced by {:.1}s.",
            elapsed_seconds
        ));
    }

    None
}

fn persist_sample(
    database: &Database,
    sample: &ClockSample,
    state: &str,
    detail: &str,
) -> Result<()> {
    database.set_service_state(STATUS_KEY, state, sample.wall_utc)?;
    database.set_service_state(DETAIL_KEY, detail, sample.wall_utc)?;
    database.set_service_state(
        LAST_WALL_UTC_KEY,
        &sample.wall_utc.to_rfc3339(),
        sample.wall_utc,
    )?;
    database.set_service_state(LAST_BOOT_ID_KEY, &sample.boot_id, sample.wall_utc)?;
    database.set_service_state(
        LAST_BOOTTIME_SECONDS_KEY,
        &format!("{:.3}", sample.boottime_seconds),
        sample.wall_utc,
    )?;
    Ok(())
}

fn clock_tampered_error(detail: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "system clock integrity is tampered: {detail}. Restore the system time before changing privileged state."
    ))
}

struct ClockSample {
    wall_utc: DateTime<Utc>,
    boot_id: String,
    boottime_seconds: f64,
}

fn current_sample(wall_utc: DateTime<Utc>) -> Result<ClockSample> {
    Ok(ClockSample {
        wall_utc,
        boot_id: read_boot_id()?,
        boottime_seconds: read_boottime_seconds()?,
    })
}

fn read_boot_id() -> Result<String> {
    Ok(fs::read_to_string(BOOT_ID_PATH)?.trim().to_string())
}

fn read_boottime_seconds() -> Result<f64> {
    let contents = fs::read_to_string(UPTIME_PATH)?;
    contents
        .split_whitespace()
        .next()
        .ok_or_else(|| DaemonError::InvalidRequest("system uptime is unavailable".to_string()))?
        .parse::<f64>()
        .map_err(|err| DaemonError::InvalidRequest(format!("invalid system uptime: {err}")))
}

fn parse_utc(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|err| {
            DaemonError::InvalidRequest(format!("invalid stored clock timestamp: {err}"))
        })
}
