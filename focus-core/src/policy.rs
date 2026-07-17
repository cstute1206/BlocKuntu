use chrono::{DateTime, Datelike, Duration, FixedOffset, TimeZone, Timelike, Utc};
use url::Url;

use crate::{
    AppMatcherConfig, AppMatcherKind, AppRuleConfig, BlockReason, Config, ControlledBlockReason,
    Database, Decision, DetoxSession, DetoxTargetKind, Error, EvaluationContext, ProcessIdentity,
    RuleConfig, RulePatternConfig, RulePatternKind, RuleTier, ScheduleConfig, UnlockError,
    UnlockState, VisitState, Weekday,
};

const TIER_3_UNLOCK_MINUTES: u32 = 2;
const GLOBAL_UNLOCKS_PER_HOUR: u32 = 1;
const MIN_UNLOCK_REASON_LETTERS: usize = 20;

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

        for rule in self.matching_rules(&parsed, RuleTier::Hard) {
            return Decision::Block(BlockReason::HardBlock {
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
            });
        }

        match self.detox_block_for_url(&parsed, context) {
            Ok(Some(reason)) => return Decision::Block(reason),
            Ok(None) => {}
            Err(err) => return runtime_error(err),
        }

        for rule in self.matching_rules(&parsed, RuleTier::ScheduledBlock) {
            if self.rule_is_active(rule, context) {
                return Decision::Block(BlockReason::ScheduledBlock {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                });
            }
        }

        let mut controlled_block: Option<(Decision, ControlledRuleStrictness)> = None;
        for rule in self.matching_rules(&parsed, RuleTier::ControlledAccess) {
            match self.controlled_rule_is_active(rule, context) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(err) => return runtime_error(err),
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
        for rule in self.matching_app_rules(process, RuleTier::Hard) {
            return Decision::Block(BlockReason::HardBlock {
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
            });
        }

        match self.detox_block_for_app(process, context) {
            Ok(Some(reason)) => return Decision::Block(reason),
            Ok(None) => {}
            Err(err) => return runtime_error(err),
        }

        for rule in self.matching_app_rules(process, RuleTier::ScheduledBlock) {
            if self.app_rule_is_active(rule, context) {
                return Decision::Block(BlockReason::ScheduledBlock {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                });
            }
        }

        let mut controlled_block: Option<(Decision, ControlledRuleStrictness)> = None;
        for rule in self.matching_app_rules(process, RuleTier::ControlledAccess) {
            match self.controlled_app_rule_is_active(rule, context) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(err) => return runtime_error(err),
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

    pub fn site_usage_is_metered(&self, url: &str, context: &EvaluationContext<'_>) -> bool {
        let Ok(parsed) = NormalizedUrl::parse(url) else {
            return false;
        };

        self.visit_rule_for_url(&parsed, context)
            .and_then(|rule| self.rule_allowance_minutes(rule))
            .is_some()
    }

    pub fn request_unlock(
        &self,
        target: &str,
        reason: String,
        context: &EvaluationContext<'_>,
    ) -> Result<UnlockState, Error> {
        let target = target.trim();
        if target.is_empty() {
            return Err(UnlockError::EmptyTarget.into());
        }
        if let Some((rule_id, session_id, ends_at)) =
            self.active_detox_for_unlock_target(target, context)?
        {
            return Err(UnlockError::TargetInActiveDetox {
                rule_id,
                session_id,
                ends_at,
            }
            .into());
        }
        let rule = self.resolve_unlock_rule(target, context)?;

        let reason = clean_unlock_reason(&reason);
        if reason.is_empty() {
            return Err(UnlockError::EmptyReason.into());
        }
        let letter_count = reason
            .chars()
            .filter(|character| character.is_alphabetic())
            .count();
        if letter_count < MIN_UNLOCK_REASON_LETTERS {
            return Err(UnlockError::ReasonTooShort {
                minimum: MIN_UNLOCK_REASON_LETTERS,
                actual: letter_count,
            }
            .into());
        }
        let normalized_reason = normalize_unlock_reason(&reason);
        if self
            .database
            .unlock_reasons()?
            .iter()
            .any(|existing| normalize_unlock_reason(existing) == normalized_reason)
        {
            return Err(UnlockError::ReasonAlreadyUsed.into());
        }

        let minutes = TIER_3_UNLOCK_MINUTES;

        let now = context.now_utc();
        let active_unlock = self.database.active_unlock_for_rule(&rule.id, now)?;
        if let Some(active) = active_unlock {
            return Err(UnlockError::UnlockAlreadyActive {
                rule_id: rule.id.clone(),
                active_until: active.expires_at,
            }
            .into());
        }

        let unlocks_in_hour = self
            .database
            .count_unlocks_since(now - Duration::hours(1))?;
        if unlocks_in_hour >= GLOBAL_UNLOCKS_PER_HOUR {
            return Err(UnlockError::HourlyQuotaExceeded {
                limit: GLOBAL_UNLOCKS_PER_HOUR,
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
        if self.detox_block_for_app(process, context)?.is_some()
            || self
                .matching_app_rules(process, RuleTier::ScheduledBlock)
                .any(|rule| self.app_rule_is_active(rule, context))
        {
            return Ok(Vec::new());
        }

        if self
            .matching_app_rules(process, RuleTier::Hard)
            .any(|rule| self.app_rule_is_active(rule, context))
        {
            return Ok(Vec::new());
        }

        let mut metered = Vec::new();
        for rule in self.matching_app_rules(process, RuleTier::ControlledAccess) {
            if self.controlled_app_rule_is_active(rule, context)?
                && self.app_rule_allowance_minutes(rule).is_some()
            {
                metered.push(rule.id.clone());
            }
        }
        Ok(metered)
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
        if !context.clock_tampered {
            let active_unlock = self.database.active_unlock_for_rule(rule_id, now);
            match active_unlock {
                Ok(Some(_)) => return Decision::Allow,
                Ok(None) => {}
                Err(err) => return runtime_error(err),
            }
        }

        if context.clock_tampered {
            return Decision::Block(BlockReason::ControlledAccess {
                rule_id: rule_id.to_string(),
                rule_name: rule_name.to_string(),
                reason: ControlledBlockReason::UnlockRequired,
            });
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
                self.used_seconds_for_site_rule_on_day(rule, context)
            }
            ControlledUsage::AppRule => {
                let (day_start, day_end) = local_day_bounds(context.now);
                self.database.used_seconds_for_app_rule_between(
                    rule_id,
                    day_start,
                    day_end,
                    context.now_utc(),
                )
            }
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
        if let Some(rule) = self.config.app_rules.iter().find(|rule| rule.id == target) {
            let active = match rule.tier {
                RuleTier::Hard => true,
                RuleTier::ScheduledBlock => self.app_rule_is_active(rule, context),
                RuleTier::ControlledAccess => self.controlled_app_rule_is_active(rule, context)?,
            };
            if active {
                return unlock_rule_from_app(rule, target);
            }
        }

        if let Some(parsed) = normalize_unlock_url_target(target) {
            let unlock_target = parsed.url_without_fragment.clone();
            for rule in self.matching_rules(&parsed, RuleTier::Hard) {
                return Err(UnlockError::TargetIsHardBlocked {
                    rule_id: rule.id.clone(),
                }
                .into());
            }

            for rule in self.matching_rules(&parsed, RuleTier::ScheduledBlock) {
                if self.rule_is_active(rule, context) {
                    return Err(UnlockError::TargetIsScheduledBlocked {
                        rule_id: rule.id.clone(),
                    }
                    .into());
                }
            }

            if let Some(rule) = self.controlled_rule_for_unlock(&parsed, context)? {
                return Ok(ResolvedUnlockRule {
                    id: rule.id.clone(),
                    target: unlock_target,
                });
            }
        }

        if let Some(rule) = self
            .config
            .app_rules
            .iter()
            .find(|rule| rule.tier == RuleTier::Hard && app_rule_target_matches(rule, target))
        {
            return Err(UnlockError::TargetIsHardBlocked {
                rule_id: rule.id.clone(),
            }
            .into());
        }

        if let Some(rule) = self.config.app_rules.iter().find(|rule| {
            rule.tier == RuleTier::ScheduledBlock
                && self.app_rule_is_active(rule, context)
                && app_rule_target_matches(rule, target)
        }) {
            return Err(UnlockError::TargetIsScheduledBlocked {
                rule_id: rule.id.clone(),
            }
            .into());
        }

        for rule in self
            .config
            .app_rules
            .iter()
            .filter(|rule| rule.tier == RuleTier::ControlledAccess)
        {
            if app_rule_target_matches(rule, target)
                && self.controlled_app_rule_is_active(rule, context)?
            {
                return unlock_rule_from_app(rule, target);
            }
        }

        Err(UnlockError::UnknownTarget {
            target: target.to_string(),
        }
        .into())
    }

    fn active_detox_for_unlock_target(
        &self,
        target: &str,
        context: &EvaluationContext<'_>,
    ) -> Result<Option<(String, String, DateTime<Utc>)>, Error> {
        if let Some(parsed) = normalize_unlock_url_target(target) {
            if let Some(BlockReason::Detox {
                rule_id,
                session_id,
                ends_at,
                ..
            }) = self.detox_block_for_url(&parsed, context)?
            {
                return Ok(Some((rule_id, session_id, ends_at)));
            }
        }

        let active_sessions = if context.clock_tampered {
            self.database.uncancelled_detox_sessions()?
        } else {
            self.database.active_detox_sessions(context.now_utc())?
        };
        let matching_rule_ids: Vec<&str> = self
            .config
            .app_rules
            .iter()
            .filter(|rule| rule.tier == RuleTier::ScheduledBlock)
            .filter(|rule| rule.id == target || app_rule_target_matches(rule, target))
            .map(|rule| rule.id.as_str())
            .collect();

        Ok(active_sessions
            .into_iter()
            .filter_map(|session| {
                matching_rule_ids
                    .iter()
                    .find(|rule_id| session.app_rule_ids.iter().any(|id| id == **rule_id))
                    .map(|rule_id| ((*rule_id).to_string(), session.id.clone(), session.ends_at))
            })
            .max_by_key(|(_, _, ends_at)| ends_at.timestamp_micros()))
    }

    fn controlled_rule_for_unlock<'b>(
        &'b self,
        parsed: &'b NormalizedUrl,
        context: &EvaluationContext<'_>,
    ) -> Result<Option<&'b RuleConfig>, Error> {
        let mut active_match: Option<(&RuleConfig, ControlledRuleStrictness)> = None;
        let mut blocking_match: Option<(&RuleConfig, ControlledRuleStrictness)> = None;

        for rule in self.matching_rules(parsed, RuleTier::ControlledAccess) {
            if !self.controlled_rule_is_active(rule, context)? {
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

        Ok(blocking_match.or(active_match).map(|(rule, _)| rule))
    }

    fn visit_rule_for_url<'b>(
        &'b self,
        parsed: &'b NormalizedUrl,
        context: &EvaluationContext<'_>,
    ) -> Option<&'b RuleConfig> {
        let mut metered_match: Option<(&RuleConfig, ControlledRuleStrictness)> = None;
        let mut active_match: Option<(&RuleConfig, ControlledRuleStrictness)> = None;

        for rule in self.matching_rules(parsed, RuleTier::ControlledAccess) {
            if !self
                .controlled_rule_is_active(rule, context)
                .unwrap_or(false)
            {
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
        context: &EvaluationContext<'_>,
    ) -> Result<i64, Error> {
        let (day_start, day_end) = local_day_bounds(context.now);
        let now = context.now_utc();
        let mut used_seconds = 0_i64;

        for visit in self.database.visit_usage_between(day_start, day_end, now)? {
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

            let ended_at = visit.ended_at.unwrap_or(visit.last_heartbeat_at);
            let clamped_start = visit.started_at.max(day_start);
            let clamped_end = ended_at.min(now).min(day_end);
            if clamped_end > clamped_start {
                used_seconds += self.rule_active_seconds_between(
                    rule,
                    clamped_start,
                    clamped_end,
                    *context.now.offset(),
                )?;
            }
        }

        Ok(used_seconds)
    }

    fn rule_active_seconds_between(
        &self,
        rule: &RuleConfig,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        local_offset: FixedOffset,
    ) -> Result<i64, Error> {
        if end <= start {
            return Ok(0);
        }

        let start_utc = start;
        let end_utc = end;
        let start = start_utc.with_timezone(&local_offset);
        let end = end_utc.with_timezone(&local_offset);
        let mut date = start
            .date_naive()
            .pred_opt()
            .unwrap_or_else(|| start.date_naive());
        let final_date = end.date_naive();
        let mut intervals = Vec::new();

        for (detox_start, detox_end) in self
            .database
            .detox_intervals_for_site_rule_between(&rule.id, start_utc, end_utc)?
        {
            intervals.push((
                detox_start.with_timezone(&local_offset),
                detox_end.with_timezone(&local_offset),
            ));
        }

        while date <= final_date {
            let weekday = Weekday::from(date.weekday());
            for schedule in self.config.schedules.iter().filter(|schedule| {
                rule.schedule_ids
                    .iter()
                    .any(|schedule_id| schedule_id == &schedule.id)
            }) {
                for window in &schedule.windows {
                    if !window.weekday.includes(weekday) {
                        continue;
                    }

                    let window_start = local_offset
                        .with_ymd_and_hms(
                            date.year(),
                            date.month(),
                            date.day(),
                            u32::from(window.start.hour()),
                            u32::from(window.start.minute()),
                            0,
                        )
                        .single()
                        .expect("fixed offsets always resolve local times");
                    let window_end_date = if window.start < window.end {
                        date
                    } else {
                        date.succ_opt()
                            .expect("schedule dates remain representable")
                    };
                    let window_end = local_offset
                        .with_ymd_and_hms(
                            window_end_date.year(),
                            window_end_date.month(),
                            window_end_date.day(),
                            u32::from(window.end.hour()),
                            u32::from(window.end.minute()),
                            0,
                        )
                        .single()
                        .expect("fixed offsets always resolve local times");

                    let overlap_start = std::cmp::max(window_start, start);
                    let overlap_end = std::cmp::min(window_end, end);
                    if overlap_end > overlap_start {
                        intervals.push((overlap_start, overlap_end));
                    }
                }
            }

            let Some(next_date) = date.succ_opt() else {
                break;
            };
            date = next_date;
        }

        intervals.sort_by_key(|(interval_start, _)| *interval_start);
        let mut total_seconds = 0_i64;
        let mut active_interval: Option<(DateTime<FixedOffset>, DateTime<FixedOffset>)> = None;
        for (interval_start, interval_end) in intervals {
            match active_interval {
                Some((active_start, active_end)) if interval_start <= active_end => {
                    active_interval = Some((active_start, std::cmp::max(active_end, interval_end)));
                }
                Some((active_start, active_end)) => {
                    total_seconds =
                        total_seconds.saturating_add((active_end - active_start).num_seconds());
                    active_interval = Some((interval_start, interval_end));
                }
                None => active_interval = Some((interval_start, interval_end)),
            }
        }
        if let Some((active_start, active_end)) = active_interval {
            total_seconds = total_seconds.saturating_add((active_end - active_start).num_seconds());
        }

        Ok(total_seconds)
    }

    fn detox_block_for_url(
        &self,
        parsed: &NormalizedUrl,
        context: &EvaluationContext<'_>,
    ) -> Result<Option<BlockReason>, Error> {
        let active_sessions = if context.clock_tampered {
            self.database.uncancelled_detox_sessions()?
        } else {
            self.database.active_detox_sessions(context.now_utc())?
        };
        let mut block: Option<(&DetoxSession, &RuleConfig)> = None;

        for rule in &self.config.rules {
            if rule.tier != RuleTier::ScheduledBlock {
                continue;
            }
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
        let active_sessions = if context.clock_tampered {
            self.database.uncancelled_detox_sessions()?
        } else {
            self.database.active_detox_sessions(context.now_utc())?
        };
        let mut block: Option<(&DetoxSession, &AppRuleConfig)> = None;

        for rule in &self.config.app_rules {
            if rule.tier != RuleTier::ScheduledBlock {
                continue;
            }
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
            RuleTier::ScheduledBlock | RuleTier::ControlledAccess => {
                self.schedule_ids_are_active(&rule.schedule_ids, context)
            }
        }
    }

    fn app_rule_is_active(&self, rule: &AppRuleConfig, context: &EvaluationContext<'_>) -> bool {
        match rule.tier {
            RuleTier::Hard => true,
            RuleTier::ScheduledBlock | RuleTier::ControlledAccess => {
                self.schedule_ids_are_active(&rule.schedule_ids, context)
            }
        }
    }

    fn controlled_rule_is_active(
        &self,
        rule: &RuleConfig,
        context: &EvaluationContext<'_>,
    ) -> Result<bool, Error> {
        Ok(self.rule_is_active(rule, context)
            || active_detox_sessions(self.database, context)?
                .iter()
                .any(|session| session.site_rule_ids.iter().any(|id| id == &rule.id)))
    }

    fn controlled_app_rule_is_active(
        &self,
        rule: &AppRuleConfig,
        context: &EvaluationContext<'_>,
    ) -> Result<bool, Error> {
        Ok(self.app_rule_is_active(rule, context)
            || active_detox_sessions(self.database, context)?
                .iter()
                .any(|session| session.app_rule_ids.iter().any(|id| id == &rule.id)))
    }

    fn schedule_ids_are_active(
        &self,
        schedule_ids: &[String],
        context: &EvaluationContext<'_>,
    ) -> bool {
        schedule_ids_are_active_at(
            schedule_ids,
            self.config,
            context.now,
            context.clock_tampered,
        )
    }
}

fn clean_unlock_reason(reason: &str) -> String {
    reason.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_unlock_reason(reason: &str) -> String {
    clean_unlock_reason(reason).to_lowercase()
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

pub fn site_usage_is_metered(url: &str, context: &EvaluationContext<'_>) -> bool {
    PolicyEngine::new(context.config, context.database).site_usage_is_metered(url, context)
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
    reason: String,
    context: &EvaluationContext<'_>,
) -> Result<UnlockState, Error> {
    PolicyEngine::new(context.config, context.database).request_unlock(target, reason, context)
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

fn local_day_bounds(now: DateTime<FixedOffset>) -> (DateTime<Utc>, DateTime<Utc>) {
    let day_start = now
        .offset()
        .from_local_datetime(
            &now.date_naive()
                .and_hms_opt(0, 0, 0)
                .expect("midnight is valid"),
        )
        .single()
        .expect("fixed offsets always resolve local times");
    let day_end = day_start + Duration::days(1);
    (day_start.with_timezone(&Utc), day_end.with_timezone(&Utc))
}

fn unlock_rule_from_app(rule: &AppRuleConfig, target: &str) -> Result<ResolvedUnlockRule, Error> {
    match rule.tier {
        RuleTier::Hard => Err(UnlockError::TargetIsHardBlocked {
            rule_id: rule.id.clone(),
        }
        .into()),
        RuleTier::ScheduledBlock => Err(UnlockError::TargetIsScheduledBlocked {
            rule_id: rule.id.clone(),
        }
        .into()),
        RuleTier::ControlledAccess => Ok(ResolvedUnlockRule {
            id: rule.id.clone(),
            target: target.to_string(),
        }),
    }
}

fn active_detox_sessions(
    database: &Database,
    context: &EvaluationContext<'_>,
) -> Result<Vec<DetoxSession>, Error> {
    if context.clock_tampered {
        database.uncancelled_detox_sessions()
    } else {
        database.active_detox_sessions(context.now_utc())
    }
}

fn app_rule_target_matches(rule: &AppRuleConfig, target: &str) -> bool {
    rule.matchers
        .iter()
        .any(|matcher| app_matcher_value_matches(matcher.kind, &matcher.value, target))
}

fn app_matcher_matches(matcher: &AppMatcherConfig, process: &ProcessIdentity) -> bool {
    match matcher.kind {
        AppMatcherKind::ExecutablePath => process
            .executable_path
            .as_deref()
            .map(|value| app_matcher_value_matches(matcher.kind, value, &matcher.value))
            .unwrap_or(false),
        AppMatcherKind::ExecutableBasename => process
            .executable_basename
            .as_deref()
            .map(|value| app_matcher_value_matches(matcher.kind, value, &matcher.value))
            .unwrap_or(false),
        AppMatcherKind::CommandName => process
            .command_name
            .as_deref()
            .map(|value| app_matcher_value_matches(matcher.kind, value, &matcher.value))
            .unwrap_or(false),
        AppMatcherKind::DesktopId => process
            .desktop_id
            .as_deref()
            .map(|value| app_matcher_value_matches(matcher.kind, value, &matcher.value))
            .unwrap_or(false),
        AppMatcherKind::WindowTitleExact => process
            .window_titles
            .iter()
            .any(|title| app_matcher_value_matches(matcher.kind, title, &matcher.value)),
        AppMatcherKind::WindowTitleContains => {
            let needle = matcher.value.to_ascii_lowercase();
            process
                .window_titles
                .iter()
                .any(|title| title.to_ascii_lowercase().contains(&needle))
        }
    }
}

fn app_matcher_value_matches(kind: AppMatcherKind, actual: &str, expected: &str) -> bool {
    if kind.matches_case_insensitively() {
        actual.eq_ignore_ascii_case(expected)
    } else {
        actual == expected
    }
}

pub fn schedule_ids_are_active_at(
    schedule_ids: &[String],
    config: &Config,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> bool {
    if clock_tampered {
        return !schedule_ids.is_empty();
    }

    !schedule_ids.is_empty()
        && schedule_ids.iter().any(|schedule_id| {
            config
                .schedules
                .iter()
                .find(|schedule| schedule.id == *schedule_id)
                .map(|schedule| schedule_is_active(schedule, now))
                .unwrap_or(true)
        })
}

fn schedule_is_active(schedule: &ScheduleConfig, now: DateTime<FixedOffset>) -> bool {
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
        RulePatternKind::UrlContains => parsed
            .url_without_fragment
            .to_ascii_lowercase()
            .contains(&pattern.value.to_ascii_lowercase()),
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
        RulePatternKind::UrlContains => 2,
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
