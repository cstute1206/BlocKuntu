use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use url::Url;

use crate::{
    AppMatcherConfig, AppMatcherKind, AppRuleConfig, BlockReason, Config, ControlledBlockReason,
    Database, Decision, DetoxSession, DetoxTargetKind, Error, EvaluationContext, ProcessIdentity,
    RuleConfig, RulePatternConfig, RulePatternKind, RuleTier, ScheduleConfig, UnlockError,
    UnlockPolicyConfig, UnlockState, VisitState, Weekday,
};

const FIXED_TIER_2_UNLOCK_POLICY: UnlockPolicyConfig = UnlockPolicyConfig {
    max_session_minutes: 2,
    cooldown_minutes: 0,
    max_unlocks_per_hour: 1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ControlledRuleStrictness {
    allowance_minutes: Option<u32>,
    pattern_specificity: u8,
}

impl ControlledRuleStrictness {
    fn is_stricter_than(self, other: Self) -> bool {
        match (self.allowance_minutes, other.allowance_minutes) {
            (None, Some(_)) => return true,
            (Some(_), None) => return false,
            (Some(left), Some(right)) if left != right => return left < right,
            _ => {}
        }

        self.pattern_specificity > other.pattern_specificity
    }
}

#[derive(Debug, Clone, Copy)]
enum ControlledUsage<'a> {
    SiteRule(&'a RuleConfig),
    AppRule,
}

pub struct PolicyEngine<'a> {
    config: &'a Config,
    database: &'a Database,
}

impl<'a> PolicyEngine<'a> {
    pub fn new(config: &'a Config, database: &'a Database) -> Self {
        Self { config, database }
    }

    pub fn evaluate_url(&self, url: &str, context: &EvaluationContext<'_>) -> Decision {
        let parsed = match NormalizedUrl::parse(url) {
            Ok(parsed) => parsed,
            Err(_) => {
                return Decision::Block(BlockReason::InvalidUrl {
                    url: url.to_string(),
                });
            }
        };

        match self.detox_block_for_url(&parsed, context) {
            Ok(Some(reason)) => return Decision::Block(reason),
            Ok(None) => {}
            Err(err) => return runtime_error(err),
        }

        for rule in self.matching_rules(&parsed, RuleTier::Hard) {
            if self.rule_is_active(rule, context) {
                return Decision::Block(BlockReason::HardBlock {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                });
            }
        }

        let mut controlled_block: Option<(Decision, ControlledRuleStrictness)> = None;
        for rule in self.matching_rules(&parsed, RuleTier::ControlledAccess) {
            if !self.rule_is_active(rule, context) {
                continue;
            }

            let decision = self.evaluate_controlled_rule(rule, context);
            if decision.is_block() {
                let strictness = self.controlled_rule_strictness(rule, &parsed);
                let should_replace = controlled_block
                    .as_ref()
                    .map(|(_, current)| strictness.is_stricter_than(*current))
                    .unwrap_or(true);
                if should_replace {
                    controlled_block = Some((decision, strictness));
                }
            }
        }

        controlled_block
            .map(|(decision, _)| decision)
            .unwrap_or(Decision::Allow)
    }

    pub fn evaluate_app(
        &self,
        process: &ProcessIdentity,
        context: &EvaluationContext<'_>,
    ) -> Decision {
        match self.detox_block_for_app(process, context) {
            Ok(Some(reason)) => return Decision::Block(reason),
            Ok(None) => {}
            Err(err) => return runtime_error(err),
        }

        for rule in self.matching_app_rules(process, RuleTier::Hard) {
            if self.app_rule_is_active(rule, context) {
                return Decision::Block(BlockReason::HardBlock {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                });
            }
        }

        let mut controlled_block: Option<(Decision, ControlledRuleStrictness)> = None;
        for rule in self.matching_app_rules(process, RuleTier::ControlledAccess) {
            if !self.app_rule_is_active(rule, context) {
                continue;
            }

            let decision = self.evaluate_controlled_app_rule(rule, context);
            if decision.is_block() {
                let strictness = self.controlled_app_rule_strictness(rule);
                let should_replace = controlled_block
                    .as_ref()
                    .map(|(_, current)| strictness.is_stricter_than(*current))
                    .unwrap_or(true);
                if should_replace {
                    controlled_block = Some((decision, strictness));
                }
            }
        }

        controlled_block
            .map(|(decision, _)| decision)
            .unwrap_or(Decision::Allow)
    }

    pub fn request_unlock(
        &self,
        target: &str,
        minutes: u32,
        reason: String,
        context: &EvaluationContext<'_>,
    ) -> Result<UnlockState, Error> {
        let target = target.trim();
        if target.is_empty() {
            return Err(UnlockError::EmptyTarget.into());
        }
        if minutes == 0 {
            return Err(UnlockError::InvalidDuration.into());
        }

        let reason = reason.trim().to_string();
        if reason.is_empty() {
            return Err(UnlockError::EmptyReason.into());
        }

        let rule = self.resolve_unlock_rule(target, context)?;
        let policy = FIXED_TIER_2_UNLOCK_POLICY;
        let minutes = policy.max_session_minutes;

        let now = context.now_utc();
        let active_unlock = self.database.active_unlock_for_rule(&rule.id, now)?;
        if let Some(active) = active_unlock {
            return Err(UnlockError::UnlockAlreadyActive {
                rule_id: rule.id.clone(),
                active_until: active.expires_at,
            }
            .into());
        }

        if let Some(latest) = self.database.latest_unlock_for_rule(&rule.id)? {
            let retry_at =
                latest.expires_at + Duration::minutes(i64::from(policy.cooldown_minutes));
            if now < retry_at {
                return Err(UnlockError::CooldownActive {
                    rule_id: rule.id.clone(),
                    retry_at,
                }
                .into());
            }
        }

        let unlocks_in_hour = self
            .database
            .count_unlocks_since(&rule.id, now - Duration::hours(1))?;
        if unlocks_in_hour >= policy.max_unlocks_per_hour {
            return Err(UnlockError::HourlyQuotaExceeded {
                rule_id: rule.id.clone(),
                limit: policy.max_unlocks_per_hour,
            }
            .into());
        }

        let unlock = self.database.insert_unlock(
            &rule.target,
            &rule.id,
            minutes,
            &reason,
            now,
            now + Duration::minutes(i64::from(minutes)),
        )?;
        self.database.record_event(
            "unlock_granted",
            Some(&rule.target),
            Some(&format!("rule_id={};minutes={minutes}", rule.id)),
            now,
        )?;
        Ok(unlock)
    }

    pub fn record_visit_start(
        &self,
        url: &str,
        tab_id: &str,
        context: &EvaluationContext<'_>,
    ) -> Result<VisitState, Error> {
        let parsed = NormalizedUrl::parse(url)?;
        let rule_id = self
            .visit_rule_for_url(&parsed, context)
            .map(|rule| rule.id.as_str());
        let target = parsed.host.as_deref().unwrap_or(url);
        self.database
            .insert_visit_start(rule_id, target, url, tab_id, context.now_utc())
    }

    pub fn record_visit_heartbeat(
        &self,
        visit_id: i64,
        context: &EvaluationContext<'_>,
    ) -> Result<(), Error> {
        self.database
            .update_visit_heartbeat(visit_id, context.now_utc())
    }

    pub fn record_visit_end(
        &self,
        visit_id: i64,
        context: &EvaluationContext<'_>,
    ) -> Result<(), Error> {
        self.database.end_visit(visit_id, context.now_utc())
    }

    pub fn metered_app_rule_ids_for_process(
        &self,
        process: &ProcessIdentity,
        context: &EvaluationContext<'_>,
    ) -> Result<Vec<String>, Error> {
        if self.detox_block_for_app(process, context)?.is_some() {
            return Ok(Vec::new());
        }

        if self
            .matching_app_rules(process, RuleTier::Hard)
            .any(|rule| self.app_rule_is_active(rule, context))
        {
            return Ok(Vec::new());
        }

        Ok(self
            .matching_app_rules(process, RuleTier::ControlledAccess)
            .filter(|rule| {
                self.app_rule_is_active(rule, context)
                    && self.app_rule_allowance_minutes(rule).is_some()
            })
            .map(|rule| rule.id.clone())
            .collect())
    }

    fn evaluate_controlled_rule(
        &self,
        rule: &RuleConfig,
        context: &EvaluationContext<'_>,
    ) -> Decision {
        self.evaluate_controlled_rule_fields(
            &rule.id,
            &rule.name,
            rule.allowance_id.as_deref(),
            ControlledUsage::SiteRule(rule),
            context,
        )
    }

    fn evaluate_controlled_app_rule(
        &self,
        rule: &AppRuleConfig,
        context: &EvaluationContext<'_>,
    ) -> Decision {
        self.evaluate_controlled_rule_fields(
            &rule.id,
            &rule.name,
            rule.allowance_id.as_deref(),
            ControlledUsage::AppRule,
            context,
        )
    }

    fn evaluate_controlled_rule_fields(
        &self,
        rule_id: &str,
        rule_name: &str,
        allowance_id: Option<&str>,
        usage: ControlledUsage<'_>,
        context: &EvaluationContext<'_>,
    ) -> Decision {
        let now = context.now_utc();
        let active_unlock = self.database.active_unlock_for_rule(rule_id, now);
        match active_unlock {
            Ok(Some(_)) => return Decision::Allow,
            Ok(None) => {}
            Err(err) => return runtime_error(err),
        }

        let Some(allowance_id) = allowance_id else {
            return Decision::Block(BlockReason::ControlledAccess {
                rule_id: rule_id.to_string(),
                rule_name: rule_name.to_string(),
                reason: ControlledBlockReason::NoAllowance,
            });
        };

        let Some(allowance) = self
            .config
            .allowances
            .iter()
            .find(|allowance| allowance.id == allowance_id)
        else {
            return Decision::Block(BlockReason::ControlledAccess {
                rule_id: rule_id.to_string(),
                rule_name: rule_name.to_string(),
                reason: ControlledBlockReason::NoAllowance,
            });
        };

        let used_seconds = match usage {
            ControlledUsage::SiteRule(rule) => {
                self.used_seconds_for_site_rule_on_day(rule, context.now_utc())
            }
            ControlledUsage::AppRule => self
                .database
                .used_seconds_for_app_rule_on_day(rule_id, context.now_utc()),
        };

        match used_seconds {
            Ok(used_seconds) => {
                if used_seconds < i64::from(allowance.daily_minutes) * 60 {
                    Decision::Allow
                } else {
                    Decision::Block(BlockReason::ControlledAccess {
                        rule_id: rule_id.to_string(),
                        rule_name: rule_name.to_string(),
                        reason: ControlledBlockReason::AllowanceExhausted,
                    })
                }
            }
            Err(err) => runtime_error(err),
        }
    }

    fn resolve_unlock_rule(
        &self,
        target: &str,
        context: &EvaluationContext<'_>,
    ) -> Result<ResolvedUnlockRule, Error> {
        if let Some(rule) = self
            .config
            .app_rules
            .iter()
            .find(|rule| rule.id == target && self.app_rule_is_active(rule, context))
        {
            return unlock_rule_from_app(rule, target);
        }

        if let Some(parsed) = normalize_unlock_url_target(target) {
            let unlock_target = parsed.url_without_fragment.clone();
            for rule in self.matching_rules(&parsed, RuleTier::Hard) {
                if self.rule_is_active(rule, context) {
                    return Err(UnlockError::TargetIsHardBlocked {
                        rule_id: rule.id.clone(),
                    }
                    .into());
                }
            }

            if let Some(rule) = self.controlled_rule_for_unlock(&parsed, context) {
                return Ok(ResolvedUnlockRule {
                    id: rule.id.clone(),
                    target: unlock_target,
                });
            }
        }

        if let Some(rule) = self.config.app_rules.iter().find(|rule| {
            rule.tier == RuleTier::Hard
                && self.app_rule_is_active(rule, context)
                && app_rule_target_matches(rule, target)
        }) {
            return Err(UnlockError::TargetIsHardBlocked {
                rule_id: rule.id.clone(),
            }
            .into());
        }

        for rule in self.config.app_rules.iter().filter(|rule| {
            rule.tier == RuleTier::ControlledAccess && self.app_rule_is_active(rule, context)
        }) {
            if app_rule_target_matches(rule, target) {
                return unlock_rule_from_app(rule, target);
            }
        }

        Err(UnlockError::UnknownTarget {
            target: target.to_string(),
        }
        .into())
    }

    fn controlled_rule_for_unlock<'b>(
        &'b self,
        parsed: &'b NormalizedUrl,
        context: &EvaluationContext<'_>,
    ) -> Option<&'b RuleConfig> {
        let mut active_match: Option<(&RuleConfig, ControlledRuleStrictness)> = None;
        let mut blocking_match: Option<(&RuleConfig, ControlledRuleStrictness)> = None;

        for rule in self.matching_rules(parsed, RuleTier::ControlledAccess) {
            if !self.rule_is_active(rule, context) {
                continue;
            }

            let strictness = self.controlled_rule_strictness(rule, parsed);
            let should_replace_active = active_match
                .as_ref()
                .map(|(_, current)| strictness.is_stricter_than(*current))
                .unwrap_or(true);
            if should_replace_active {
                active_match = Some((rule, strictness));
            }

            if self.evaluate_controlled_rule(rule, context).is_block() {
                let should_replace_blocking = blocking_match
                    .as_ref()
                    .map(|(_, current)| strictness.is_stricter_than(*current))
                    .unwrap_or(true);
                if should_replace_blocking {
                    blocking_match = Some((rule, strictness));
                }
            }
        }

        blocking_match.or(active_match).map(|(rule, _)| rule)
    }

    fn visit_rule_for_url<'b>(
        &'b self,
        parsed: &'b NormalizedUrl,
        context: &EvaluationContext<'_>,
    ) -> Option<&'b RuleConfig> {
        let mut metered_match: Option<(&RuleConfig, ControlledRuleStrictness)> = None;
        let mut active_match: Option<(&RuleConfig, ControlledRuleStrictness)> = None;

        for rule in self.matching_rules(parsed, RuleTier::ControlledAccess) {
            if !self.rule_is_active(rule, context) {
                continue;
            }

            let strictness = self.controlled_rule_strictness(rule, parsed);
            let should_replace_active = active_match
                .as_ref()
                .map(|(_, current)| strictness.is_stricter_than(*current))
                .unwrap_or(true);
            if should_replace_active {
                active_match = Some((rule, strictness));
            }

            if self.rule_allowance_minutes(rule).is_some() {
                let should_replace_metered = metered_match
                    .as_ref()
                    .map(|(_, current)| strictness.is_stricter_than(*current))
                    .unwrap_or(true);
                if should_replace_metered {
                    metered_match = Some((rule, strictness));
                }
            }
        }

        metered_match.or(active_match).map(|(rule, _)| rule)
    }

    fn controlled_rule_strictness(
        &self,
        rule: &RuleConfig,
        parsed: &NormalizedUrl,
    ) -> ControlledRuleStrictness {
        ControlledRuleStrictness {
            allowance_minutes: self.rule_allowance_minutes(rule),
            pattern_specificity: matched_pattern_specificity(rule, parsed),
        }
    }

    fn rule_allowance_minutes(&self, rule: &RuleConfig) -> Option<u32> {
        let allowance_id = rule.allowance_id.as_deref()?;
        self.config
            .allowances
            .iter()
            .find(|allowance| allowance.id == allowance_id)
            .map(|allowance| allowance.daily_minutes)
    }

    fn controlled_app_rule_strictness(&self, rule: &AppRuleConfig) -> ControlledRuleStrictness {
        ControlledRuleStrictness {
            allowance_minutes: self.app_rule_allowance_minutes(rule),
            pattern_specificity: 0,
        }
    }

    fn app_rule_allowance_minutes(&self, rule: &AppRuleConfig) -> Option<u32> {
        let allowance_id = rule.allowance_id.as_deref()?;
        self.config
            .allowances
            .iter()
            .find(|allowance| allowance.id == allowance_id)
            .map(|allowance| allowance.daily_minutes)
    }

    fn used_seconds_for_site_rule_on_day(
        &self,
        rule: &RuleConfig,
        now: DateTime<Utc>,
    ) -> Result<i64, Error> {
        let day_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid")
            .and_utc();
        let day_end = day_start + Duration::days(1);
        let mut used_seconds = 0_i64;

        for visit in self.database.visit_usage_for_day(now)? {
            if visit.rule_id.as_deref() != Some(rule.id.as_str()) {
                let parsed = match NormalizedUrl::parse(&visit.url) {
                    Ok(parsed) => parsed,
                    Err(_) => continue,
                };
                if !rule
                    .patterns
                    .iter()
                    .any(|pattern| pattern_matches(pattern, &parsed))
                {
                    continue;
                }
            }

            let ended_at = visit.ended_at.unwrap_or(now);
            let clamped_start = visit.started_at.max(day_start);
            let clamped_end = ended_at.min(now).min(day_end);
            if clamped_end > clamped_start {
                used_seconds += (clamped_end - clamped_start).num_seconds();
            }
        }

        Ok(used_seconds)
    }

    fn detox_block_for_url(
        &self,
        parsed: &NormalizedUrl,
        context: &EvaluationContext<'_>,
    ) -> Result<Option<BlockReason>, Error> {
        let active_sessions = self.database.active_detox_sessions(context.now_utc())?;
        let mut block: Option<(&DetoxSession, &RuleConfig)> = None;

        for rule in &self.config.rules {
            if !rule
                .patterns
                .iter()
                .any(|pattern| pattern_matches(pattern, parsed))
            {
                continue;
            }

            let Some(session) = active_sessions
                .iter()
                .filter(|session| {
                    session
                        .site_rule_ids
                        .iter()
                        .any(|rule_id| rule_id == &rule.id)
                })
                .max_by_key(|session| session.ends_at.timestamp_micros())
            else {
                continue;
            };

            if block
                .as_ref()
                .map(|(current, _)| {
                    session.ends_at.timestamp_micros() > current.ends_at.timestamp_micros()
                })
                .unwrap_or(true)
            {
                block = Some((session, rule));
            }
        }

        Ok(block.map(|(session, rule)| BlockReason::Detox {
            session_id: session.id.clone(),
            session_name: session.name.clone(),
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            target_kind: DetoxTargetKind::SiteRule,
            ends_at: session.ends_at,
        }))
    }

    fn detox_block_for_app(
        &self,
        process: &ProcessIdentity,
        context: &EvaluationContext<'_>,
    ) -> Result<Option<BlockReason>, Error> {
        let active_sessions = self.database.active_detox_sessions(context.now_utc())?;
        let mut block: Option<(&DetoxSession, &AppRuleConfig)> = None;

        for rule in &self.config.app_rules {
            if !rule
                .matchers
                .iter()
                .any(|matcher| app_matcher_matches(matcher, process))
            {
                continue;
            }

            let Some(session) = active_sessions
                .iter()
                .filter(|session| {
                    session
                        .app_rule_ids
                        .iter()
                        .any(|rule_id| rule_id == &rule.id)
                })
                .max_by_key(|session| session.ends_at.timestamp_micros())
            else {
                continue;
            };

            if block
                .as_ref()
                .map(|(current, _)| {
                    session.ends_at.timestamp_micros() > current.ends_at.timestamp_micros()
                })
                .unwrap_or(true)
            {
                block = Some((session, rule));
            }
        }

        Ok(block.map(|(session, rule)| BlockReason::Detox {
            session_id: session.id.clone(),
            session_name: session.name.clone(),
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            target_kind: DetoxTargetKind::AppRule,
            ends_at: session.ends_at,
        }))
    }

    fn matching_rules<'b>(
        &'b self,
        parsed: &'b NormalizedUrl,
        tier: RuleTier,
    ) -> impl Iterator<Item = &'b RuleConfig> + 'b {
        self.config
            .rules
            .iter()
            .filter(move |rule| rule.tier == tier)
            .filter(move |rule| {
                rule.patterns
                    .iter()
                    .any(|pattern| pattern_matches(pattern, parsed))
            })
    }

    fn matching_app_rules<'b>(
        &'b self,
        process: &'b ProcessIdentity,
        tier: RuleTier,
    ) -> impl Iterator<Item = &'b AppRuleConfig> + 'b {
        self.config
            .app_rules
            .iter()
            .filter(move |rule| rule.tier == tier)
            .filter(move |rule| {
                rule.matchers
                    .iter()
                    .any(|matcher| app_matcher_matches(matcher, process))
            })
    }

    fn rule_is_active(&self, rule: &RuleConfig, context: &EvaluationContext<'_>) -> bool {
        match rule.tier {
            RuleTier::Hard => true,
            RuleTier::ControlledAccess => self.schedule_ids_are_active(&rule.schedule_ids, context),
        }
    }

    fn app_rule_is_active(&self, rule: &AppRuleConfig, context: &EvaluationContext<'_>) -> bool {
        match rule.tier {
            RuleTier::Hard => true,
            RuleTier::ControlledAccess => self.schedule_ids_are_active(&rule.schedule_ids, context),
        }
    }

    fn schedule_ids_are_active(
        &self,
        schedule_ids: &[String],
        context: &EvaluationContext<'_>,
    ) -> bool {
        !schedule_ids.is_empty()
            && schedule_ids.iter().any(|schedule_id| {
                self.config
                    .schedules
                    .iter()
                    .find(|schedule| schedule.id == *schedule_id)
                    .map(|schedule| schedule_is_active(schedule, context))
                    .unwrap_or(true)
            })
    }
}

struct ResolvedUnlockRule {
    id: String,
    target: String,
}

pub fn evaluate_url(url: &str, context: &EvaluationContext<'_>) -> Decision {
    PolicyEngine::new(context.config, context.database).evaluate_url(url, context)
}

pub fn evaluate_app(process: &ProcessIdentity, context: &EvaluationContext<'_>) -> Decision {
    PolicyEngine::new(context.config, context.database).evaluate_app(process, context)
}

pub fn metered_app_rule_ids_for_process(
    process: &ProcessIdentity,
    context: &EvaluationContext<'_>,
) -> Result<Vec<String>, Error> {
    PolicyEngine::new(context.config, context.database)
        .metered_app_rule_ids_for_process(process, context)
}

pub fn request_unlock(
    target: &str,
    minutes: u32,
    reason: String,
    context: &EvaluationContext<'_>,
) -> Result<UnlockState, Error> {
    PolicyEngine::new(context.config, context.database)
        .request_unlock(target, minutes, reason, context)
}

pub fn record_visit_start(
    url: &str,
    tab_id: &str,
    context: &EvaluationContext<'_>,
) -> Result<VisitState, Error> {
    PolicyEngine::new(context.config, context.database).record_visit_start(url, tab_id, context)
}

pub fn record_visit_heartbeat(visit_id: i64, context: &EvaluationContext<'_>) -> Result<(), Error> {
    PolicyEngine::new(context.config, context.database).record_visit_heartbeat(visit_id, context)
}

pub fn record_visit_end(visit_id: i64, context: &EvaluationContext<'_>) -> Result<(), Error> {
    PolicyEngine::new(context.config, context.database).record_visit_end(visit_id, context)
}

fn unlock_rule_from_app(rule: &AppRuleConfig, target: &str) -> Result<ResolvedUnlockRule, Error> {
    match rule.tier {
        RuleTier::Hard => Err(UnlockError::TargetIsHardBlocked {
            rule_id: rule.id.clone(),
        }
        .into()),
        RuleTier::ControlledAccess => Ok(ResolvedUnlockRule {
            id: rule.id.clone(),
            target: target.to_string(),
        }),
    }
}

fn app_rule_target_matches(rule: &AppRuleConfig, target: &str) -> bool {
    rule.matchers.iter().any(|matcher| {
        matcher.value == target
            || matches!(
                matcher.kind,
                AppMatcherKind::ExecutableBasename
                    | AppMatcherKind::CommandName
                    | AppMatcherKind::DesktopId
            ) && matcher.value.eq_ignore_ascii_case(target)
    })
}

fn app_matcher_matches(matcher: &AppMatcherConfig, process: &ProcessIdentity) -> bool {
    match matcher.kind {
        AppMatcherKind::ExecutablePath => process
            .executable_path
            .as_deref()
            .map(|value| value == matcher.value)
            .unwrap_or(false),
        AppMatcherKind::ExecutableBasename => process
            .executable_basename
            .as_deref()
            .map(|value| value == matcher.value)
            .unwrap_or(false),
        AppMatcherKind::CommandName => process
            .command_name
            .as_deref()
            .map(|value| value == matcher.value)
            .unwrap_or(false),
        AppMatcherKind::DesktopId => process
            .desktop_id
            .as_deref()
            .map(|value| value == matcher.value)
            .unwrap_or(false),
        AppMatcherKind::WindowTitleExact => process
            .window_titles
            .iter()
            .any(|title| title == &matcher.value),
        AppMatcherKind::WindowTitleContains => {
            let needle = matcher.value.to_ascii_lowercase();
            process
                .window_titles
                .iter()
                .any(|title| title.to_ascii_lowercase().contains(&needle))
        }
    }
}

fn schedule_is_active(schedule: &ScheduleConfig, context: &EvaluationContext<'_>) -> bool {
    let now = context.now;
    let current_weekday = Weekday::from(now.weekday());
    let current_minute = (now.hour() as u16) * 60 + now.minute() as u16;

    schedule.windows.iter().any(|window| {
        let start = window.start.minutes_after_midnight();
        let end = window.end.minutes_after_midnight();

        if start < end {
            window.weekday.includes(current_weekday)
                && current_minute >= start
                && current_minute < end
        } else {
            (window.weekday.includes(current_weekday) && current_minute >= start)
                || (window.weekday.includes(current_weekday.previous()) && current_minute < end)
        }
    })
}

fn pattern_matches(pattern: &RulePatternConfig, parsed: &NormalizedUrl) -> bool {
    match pattern.kind {
        RulePatternKind::Domain => {
            let Some(host) = parsed.host.as_deref() else {
                return false;
            };
            let value = normalized_pattern_value(&pattern.value);
            host == value || (pattern.match_subdomains && host.ends_with(&format!(".{value}")))
        }
        RulePatternKind::ExactUrl => normalize_url_pattern(&pattern.value)
            .map(|pattern| parsed.url_without_fragment == pattern)
            .unwrap_or(false),
        RulePatternKind::UrlPrefix => normalize_url_pattern(&pattern.value)
            .map(|pattern| parsed.url_without_fragment.starts_with(&pattern))
            .unwrap_or(false),
        RulePatternKind::PathPrefix => path_prefix_matches(&pattern.value, parsed),
    }
}

fn matched_pattern_specificity(rule: &RuleConfig, parsed: &NormalizedUrl) -> u8 {
    rule.patterns
        .iter()
        .filter(|pattern| pattern_matches(pattern, parsed))
        .map(pattern_specificity)
        .max()
        .unwrap_or(0)
}

fn pattern_specificity(pattern: &RulePatternConfig) -> u8 {
    match pattern.kind {
        RulePatternKind::ExactUrl => 4,
        RulePatternKind::PathPrefix | RulePatternKind::UrlPrefix => 3,
        RulePatternKind::Domain if !pattern.match_subdomains => 2,
        RulePatternKind::Domain => 1,
    }
}

fn path_prefix_matches(pattern: &str, parsed: &NormalizedUrl) -> bool {
    if pattern.starts_with('/') {
        return parsed.path.starts_with(pattern);
    }

    let Some((host, path)) = pattern.split_once('/') else {
        return false;
    };
    let Some(parsed_host) = parsed.host.as_deref() else {
        return false;
    };
    parsed_host == normalized_pattern_value(host) && parsed.path.starts_with(&format!("/{path}"))
}

fn normalize_url_pattern(pattern: &str) -> Result<String, url::ParseError> {
    let mut parsed = Url::parse(pattern)?;
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn normalize_unlock_url_target(target: &str) -> Option<NormalizedUrl> {
    if let Ok(parsed) = NormalizedUrl::parse(target) {
        return Some(parsed);
    }

    NormalizedUrl::parse(&format!("https://{target}")).ok()
}

fn normalized_pattern_value(value: &str) -> String {
    value.trim_end_matches('.').to_ascii_lowercase()
}

fn runtime_error(err: Error) -> Decision {
    Decision::Block(BlockReason::RuntimeError {
        message: err.to_string(),
    })
}

#[derive(Debug, Clone)]
struct NormalizedUrl {
    url_without_fragment: String,
    host: Option<String>,
    path: String,
}

impl NormalizedUrl {
    fn parse(value: &str) -> Result<Self, url::ParseError> {
        let mut parsed = Url::parse(value)?;
        parsed.set_fragment(None);
        let host = parsed
            .host_str()
            .map(|host| host.trim_end_matches('.').to_ascii_lowercase());
        let path = parsed.path().to_string();
        Ok(Self {
            url_without_fragment: parsed.to_string(),
            host,
            path,
        })
    }
}
