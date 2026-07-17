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
    #[error("unlock reason must contain at least {minimum} letters; found {actual}")]
    ReasonTooShort { minimum: usize, actual: usize },
    #[error("unlock reason has already been used")]
    ReasonAlreadyUsed,
    #[error("target does not match a configured controlled-access rule: {target}")]
    UnknownTarget { target: String },
    #[error("target is hard-blocked and cannot be unlocked: {rule_id}")]
    TargetIsHardBlocked { rule_id: String },
    #[error("target is Tier 2 scheduled-blocked and cannot be unlocked: {rule_id}")]
    TargetIsScheduledBlocked { rule_id: String },
    #[error("target is covered by active detox session {session_id} until {ends_at}: {rule_id}")]
    TargetInActiveDetox {
        rule_id: String,
        session_id: String,
        ends_at: DateTime<Utc>,
    },
    #[error("an unlock is already active for rule {rule_id} until {active_until}")]
    UnlockAlreadyActive {
        rule_id: String,
        active_until: DateTime<Utc>,
    },
    #[error("the global hourly unlock quota has been used; limit {limit}")]
    HourlyQuotaExceeded { limit: u32 },
}
