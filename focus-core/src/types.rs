use chrono::{DateTime, FixedOffset, Local, Utc};

use crate::{Config, Database};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block(BlockReason),
}

impl Decision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn is_block(&self) -> bool {
        matches!(self, Self::Block(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockReason {
    InvalidUrl {
        url: String,
    },
    HardBlock {
        rule_id: String,
        rule_name: String,
    },
    ControlledAccess {
        rule_id: String,
        rule_name: String,
        reason: ControlledBlockReason,
    },
    RuntimeError {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlledBlockReason {
    NoAllowance,
    AllowanceExhausted,
    UnlockRequired,
}

#[derive(Clone)]
pub struct EvaluationContext<'a> {
    pub config: &'a Config,
    pub database: &'a Database,
    pub now: DateTime<FixedOffset>,
}

impl<'a> EvaluationContext<'a> {
    pub fn new(config: &'a Config, database: &'a Database, now: DateTime<FixedOffset>) -> Self {
        Self {
            config,
            database,
            now,
        }
    }

    pub fn local_now(config: &'a Config, database: &'a Database) -> Self {
        Self::new(config, database, Local::now().fixed_offset())
    }

    pub fn now_utc(&self) -> DateTime<Utc> {
        self.now.with_timezone(&Utc)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlockState {
    pub id: i64,
    pub target: String,
    pub rule_id: String,
    pub minutes: u32,
    pub reason: String,
    pub started_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisitState {
    pub id: i64,
    pub target: String,
    pub rule_id: Option<String>,
    pub url: String,
    pub tab_id: String,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: Option<u32>,
    pub executable_path: Option<String>,
    pub command_name: Option<String>,
}
