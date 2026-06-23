use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Time(#[from] chrono::ParseError),
    #[error(transparent)]
    Unlock(#[from] UnlockError),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read configuration: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse TOML configuration: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize TOML configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("invalid configuration: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnlockError {
    #[error("unlock target is empty")]
    EmptyTarget,
    #[error("unlock reason is required")]
    EmptyReason,
    #[error("unlock duration must be at least one minute")]
    InvalidDuration,
    #[error("target does not match a configured controlled-access rule: {target}")]
    UnknownTarget { target: String },
    #[error("target is hard-blocked and cannot be unlocked: {rule_id}")]
    TargetIsHardBlocked { rule_id: String },
    #[error("requested unlock duration {requested_minutes} exceeds maximum {max_minutes}")]
    ExceedsMaxSession {
        requested_minutes: u32,
        max_minutes: u32,
    },
    #[error("an unlock is already active for rule {rule_id} until {active_until}")]
    UnlockAlreadyActive {
        rule_id: String,
        active_until: DateTime<Utc>,
    },
    #[error("cooldown is active for rule {rule_id} until {retry_at}")]
    CooldownActive {
        rule_id: String,
        retry_at: DateTime<Utc>,
    },
    #[error("hourly unlock quota exceeded for rule {rule_id}: limit {limit}")]
    HourlyQuotaExceeded { rule_id: String, limit: u32 },
}
