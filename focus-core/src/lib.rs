//! Core policy and runtime-state engine for BlocKuntu.
//!
//! This crate intentionally has no privileged side effects. It does not edit
//! `/etc`, call systemd, scan `/proc`, or bind sockets. Privileged enforcement
//! belongs in `focusd`; this crate owns durable configuration parsing, SQLite
//! runtime state, URL/app policy decisions, and unlock accounting.

mod config;
mod core;
mod db;
mod error;
mod policy;
mod types;

pub use config::{
    load_config, validate_config, AllowanceConfig, AppMatcherConfig, AppMatcherKind, AppRuleConfig,
    Config, RuleConfig, RulePatternConfig, RulePatternKind, RuleTier, ScheduleConfig, ScheduleDay,
    ScheduleWindow, StrictModeConfig, TimeOfDay, Weekday,
};
pub use core::FocusCore;
pub use db::{migrate_database, Database, HeartbeatState};
pub use error::{ConfigError, Error, UnlockError};
pub use policy::{
    evaluate_app, evaluate_url, metered_app_rule_ids_for_process, record_visit_end,
    record_visit_heartbeat, record_visit_start, request_unlock, PolicyEngine,
};
pub use types::{
    BlockReason, ControlledBlockReason, Decision, DetoxSession, DetoxTargetKind, EvaluationContext,
    ProcessIdentity, UnlockState, VisitState,
};
