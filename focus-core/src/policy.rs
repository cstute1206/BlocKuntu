use chrono::{Datelike, Duration, Timelike};
use url::Url;

use crate::{
    AppMatcherConfig, AppMatcherKind, AppRuleConfig, BlockReason, Config, ControlledBlockReason,
    Database, Decision, Error, EvaluationContext, ProcessIdentity, RuleConfig, RulePatternConfig,
    RulePatternKind, RuleTier, ScheduleConfig, UnlockError, UnlockPolicyConfig, UnlockState,
    VisitState, Weekday,
};

const FIXED_TIER_2_UNLOCK_POLICY: UnlockPolicyConfig = UnlockPolicyConfig {
    max_session_minutes: 2,
    cooldown_minutes: 0,
    max_unlocks_per_hour: 1,
};

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

        for rule in self.matching_rules(&parsed, RuleTier::Hard) {
            if self.rule_is_active(rule, context) {
                return Decision::Block(BlockReason::HardBlock {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                });
            }
        }

        for rule in self.matching_rules(&parsed, RuleTier::ControlledAccess) {
            if !self.rule_is_active(rule, context) {
                continue;
            }

            let decision =
                self.evaluate_controlled_rule(rule, &parsed.url_without_fragment, context);
            if decision.is_block() {
                return decision;
            }
        }

        Decision::Allow
    }

    pub fn evaluate_app(
        &self,
        process: &ProcessIdentity,
        context: &EvaluationContext<'_>,
    ) -> Decision {
        for rule in self.matching_app_rules(process, RuleTier::Hard) {
            if self.app_rule_is_active(rule, context) {
                return Decision::Block(BlockReason::HardBlock {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                });
            }
        }

        for rule in self.matching_app_rules(process, RuleTier::ControlledAccess) {
            if !self.app_rule_is_active(rule, context) {
                continue;
            }

            let decision = self.evaluate_controlled_rule_fields(
                &rule.id,
                &rule.name,
                rule.allowance_id.as_deref(),
                None,
                context,
            );
            if decision.is_block() {
                return decision;
            }
        }

        Decision::Allow
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
        let active_unlock = match rule.scope {
            UnlockScope::ExactUrl => {
                self.database
                    .active_unlock_for_rule_and_target(&rule.id, &rule.target, now)?
            }
            UnlockScope::Rule => self.database.active_unlock_for_rule(&rule.id, now)?,
        };
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
            .first_matching_rule(&parsed)
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

    fn evaluate_controlled_rule(
        &self,
        rule: &RuleConfig,
        url_without_fragment: &str,
        context: &EvaluationContext<'_>,
    ) -> Decision {
        self.evaluate_controlled_rule_fields(
            &rule.id,
            &rule.name,
            rule.allowance_id.as_deref(),
            Some(url_without_fragment),
            context,
        )
    }

    fn evaluate_controlled_rule_fields(
        &self,
        rule_id: &str,
        rule_name: &str,
        allowance_id: Option<&str>,
        exact_unlock_target: Option<&str>,
        context: &EvaluationContext<'_>,
    ) -> Decision {
        let now = context.now_utc();
        let active_unlock = match exact_unlock_target {
            Some(target) => self
                .database
                .active_unlock_for_rule_and_target(rule_id, target, now),
            None => self.database.active_unlock_for_rule(rule_id, now),
        };
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

        match self
            .database
            .used_seconds_for_rule_on_day(rule_id, context.now_utc())
        {
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
        if let Some(rule) = self.config.app_rules.iter().find(|rule| {
            rule.enabled && rule.id == target && self.app_rule_is_active(rule, context)
        }) {
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

            if let Some(rule) = self.controlled_rule_for_unlock(&parsed, &unlock_target, context) {
                return Ok(ResolvedUnlockRule {
                    id: rule.id.clone(),
                    target: unlock_target,
                    scope: UnlockScope::ExactUrl,
                });
            }
        }

        if let Some(rule) = self.config.app_rules.iter().find(|rule| {
            rule.enabled
                && rule.tier == RuleTier::Hard
                && self.app_rule_is_active(rule, context)
                && app_rule_target_matches(rule, target)
        }) {
            return Err(UnlockError::TargetIsHardBlocked {
                rule_id: rule.id.clone(),
            }
            .into());
        }

        for rule in self.config.app_rules.iter().filter(|rule| {
            rule.enabled
                && rule.tier == RuleTier::ControlledAccess
                && self.app_rule_is_active(rule, context)
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
        exact_unlock_target: &str,
        context: &EvaluationContext<'_>,
    ) -> Option<&'b RuleConfig> {
        let mut first_active_match = None;

        for rule in self.matching_rules(parsed, RuleTier::ControlledAccess) {
            if !self.rule_is_active(rule, context) {
                continue;
            }

            if first_active_match.is_none() {
                first_active_match = Some(rule);
            }

            if self
                .evaluate_controlled_rule(rule, exact_unlock_target, context)
                .is_block()
            {
                return Some(rule);
            }
        }

        first_active_match
    }

    fn matching_rules<'b>(
        &'b self,
        parsed: &'b NormalizedUrl,
        tier: RuleTier,
    ) -> impl Iterator<Item = &'b RuleConfig> + 'b {
        self.config
            .rules
            .iter()
            .filter(move |rule| rule.enabled && rule.tier == tier)
            .filter(move |rule| {
                rule.patterns
                    .iter()
                    .any(|pattern| pattern_matches(pattern, parsed))
            })
    }

    fn first_matching_rule<'b>(&'b self, parsed: &'b NormalizedUrl) -> Option<&'b RuleConfig> {
        self.config
            .rules
            .iter()
            .filter(|rule| rule.enabled)
            .find(|rule| {
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
            .filter(move |rule| rule.enabled && rule.tier == tier)
            .filter(move |rule| {
                rule.matchers
                    .iter()
                    .any(|matcher| app_matcher_matches(matcher, process))
            })
    }

    fn rule_is_active(&self, rule: &RuleConfig, context: &EvaluationContext<'_>) -> bool {
        self.schedule_ids_are_active(&rule.schedule_ids, context)
    }

    fn app_rule_is_active(&self, rule: &AppRuleConfig, context: &EvaluationContext<'_>) -> bool {
        self.schedule_ids_are_active(&rule.schedule_ids, context)
    }

    fn schedule_ids_are_active(
        &self,
        schedule_ids: &[String],
        context: &EvaluationContext<'_>,
    ) -> bool {
        if schedule_ids.is_empty() {
            return true;
        }

        schedule_ids.iter().any(|schedule_id| {
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
    scope: UnlockScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnlockScope {
    ExactUrl,
    Rule,
}

pub fn evaluate_url(url: &str, context: &EvaluationContext<'_>) -> Decision {
    PolicyEngine::new(context.config, context.database).evaluate_url(url, context)
}

pub fn evaluate_app(process: &ProcessIdentity, context: &EvaluationContext<'_>) -> Decision {
    PolicyEngine::new(context.config, context.database).evaluate_app(process, context)
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
            scope: UnlockScope::Rule,
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
            window.weekday == current_weekday && current_minute >= start && current_minute < end
        } else {
            (window.weekday == current_weekday && current_minute >= start)
                || (window.weekday == current_weekday.previous() && current_minute < end)
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
