use crate::error::ConfigError;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use url::Url;

pub fn load_config(path: impl AsRef<Path>) -> Result<Config, ConfigError> {
    let contents = fs::read_to_string(path)?;
    Config::from_toml_str(&contents)
}

pub fn validate_config(config: &Config) -> Result<(), ConfigError> {
    config.validate()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    #[serde(default)]
    pub app_rules: Vec<AppRuleConfig>,
    #[serde(default)]
    pub schedules: Vec<ScheduleConfig>,
    #[serde(default)]
    pub allowances: Vec<AllowanceConfig>,
    #[serde(default)]
    pub defaults: DefaultsConfig,
    #[serde(default)]
    pub strict_mode: StrictModeConfig,
}

impl Config {
    pub fn from_toml_str(contents: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(contents)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        ensure_unique_ids("rule", self.rules.iter().map(|rule| rule.id.as_str()))?;
        ensure_unique_ids(
            "app rule",
            self.app_rules.iter().map(|rule| rule.id.as_str()),
        )?;
        ensure_unique_ids(
            "schedule",
            self.schedules.iter().map(|schedule| schedule.id.as_str()),
        )?;
        ensure_unique_ids(
            "allowance",
            self.allowances
                .iter()
                .map(|allowance| allowance.id.as_str()),
        )?;

        let schedule_ids: HashSet<&str> = self
            .schedules
            .iter()
            .map(|schedule| schedule.id.as_str())
            .collect();
        let site_rule_ids: HashSet<&str> = self.rules.iter().map(|rule| rule.id.as_str()).collect();
        let allowance_ids: HashSet<&str> = self
            .allowances
            .iter()
            .map(|allowance| allowance.id.as_str())
            .collect();

        validate_unlock_policy("defaults.unlock_policy", &self.defaults.unlock_policy)?;
        if self.strict_mode.grace_seconds == 0 {
            return Err(ConfigError::Validation(
                "strict_mode.grace_seconds must be greater than zero".to_string(),
            ));
        }

        for schedule in &self.schedules {
            require_identifier("schedule", &schedule.id)?;
            for window in &schedule.windows {
                if window.start == window.end {
                    return Err(ConfigError::Validation(format!(
                        "schedule '{}' contains an empty window at {}",
                        schedule.id, window.start
                    )));
                }
            }
        }

        for allowance in &self.allowances {
            require_identifier("allowance", &allowance.id)?;
        }

        let mut allowance_links: HashMap<&str, &str> = HashMap::new();

        for rule in &self.rules {
            require_identifier("rule", &rule.id)?;
            if rule.name.trim().is_empty() {
                return Err(ConfigError::Validation(format!(
                    "rule '{}' must have a non-empty name",
                    rule.id
                )));
            }
            if rule.patterns.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "rule '{}' must contain at least one pattern",
                    rule.id
                )));
            }

            for pattern in &rule.patterns {
                validate_pattern(&rule.id, pattern)?;
            }

            for schedule_id in &rule.schedule_ids {
                if !schedule_ids.contains(schedule_id.as_str()) {
                    return Err(ConfigError::Validation(format!(
                        "rule '{}' references missing schedule '{}'",
                        rule.id, schedule_id
                    )));
                }
            }

            if let Some(allowance_id) = &rule.allowance_id {
                if !allowance_ids.contains(allowance_id.as_str()) {
                    return Err(ConfigError::Validation(format!(
                        "rule '{}' references missing allowance '{}'",
                        rule.id, allowance_id
                    )));
                }

                if let Some(existing_rule_id) =
                    allowance_links.insert(allowance_id.as_str(), rule.id.as_str())
                {
                    return Err(ConfigError::Validation(format!(
                        "allowance '{}' is already linked to rule '{}' and cannot also be linked to rule '{}'",
                        allowance_id, existing_rule_id, rule.id
                    )));
                }
            }

            match rule.tier {
                RuleTier::Hard => {
                    if rule.allowance_id.is_some() || rule.unlock_policy.is_some() {
                        return Err(ConfigError::Validation(format!(
                            "hard rule '{}' cannot define allowances or unlock policies",
                            rule.id
                        )));
                    }
                }
                RuleTier::ControlledAccess => {
                    validate_unlock_policy(
                        &format!("rule '{}'.unlock_policy", rule.id),
                        &rule.effective_unlock_policy(&self.defaults),
                    )?;
                }
            }
        }

        for app_rule in &self.app_rules {
            require_identifier("app rule", &app_rule.id)?;
            if site_rule_ids.contains(app_rule.id.as_str()) {
                return Err(ConfigError::Validation(format!(
                    "app rule '{}' conflicts with a site rule id",
                    app_rule.id
                )));
            }
            if app_rule.name.trim().is_empty() {
                return Err(ConfigError::Validation(format!(
                    "app rule '{}' must have a non-empty name",
                    app_rule.id
                )));
            }
            if app_rule.matchers.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "app rule '{}' must contain at least one matcher",
                    app_rule.id
                )));
            }

            for matcher in &app_rule.matchers {
                validate_app_matcher(&app_rule.id, matcher)?;
            }

            for schedule_id in &app_rule.schedule_ids {
                if !schedule_ids.contains(schedule_id.as_str()) {
                    return Err(ConfigError::Validation(format!(
                        "app rule '{}' references missing schedule '{}'",
                        app_rule.id, schedule_id
                    )));
                }
            }

            if let Some(allowance_id) = &app_rule.allowance_id {
                if !allowance_ids.contains(allowance_id.as_str()) {
                    return Err(ConfigError::Validation(format!(
                        "app rule '{}' references missing allowance '{}'",
                        app_rule.id, allowance_id
                    )));
                }

                if let Some(existing_rule_id) =
                    allowance_links.insert(allowance_id.as_str(), app_rule.id.as_str())
                {
                    return Err(ConfigError::Validation(format!(
                        "allowance '{}' is already linked to rule '{}' and cannot also be linked to app rule '{}'",
                        allowance_id, existing_rule_id, app_rule.id
                    )));
                }
            }

            match app_rule.tier {
                RuleTier::Hard => {
                    if app_rule.allowance_id.is_some() || app_rule.unlock_policy.is_some() {
                        return Err(ConfigError::Validation(format!(
                            "hard app rule '{}' cannot define allowances or unlock policies",
                            app_rule.id
                        )));
                    }
                }
                RuleTier::ControlledAccess => {
                    validate_unlock_policy(
                        &format!("app rule '{}'.unlock_policy", app_rule.id),
                        &app_rule.effective_unlock_policy(&self.defaults),
                    )?;
                }
            }
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            app_rules: Vec::new(),
            schedules: Vec::new(),
            allowances: Vec::new(),
            defaults: DefaultsConfig::default(),
            strict_mode: StrictModeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefaultsConfig {
    #[serde(default)]
    pub unlock_policy: UnlockPolicyConfig,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            unlock_policy: UnlockPolicyConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrictModeConfig {
    #[serde(default = "default_enabled")]
    pub require_firefox_extension: bool,
    #[serde(default = "default_enabled")]
    pub require_chrome_extension: bool,
    #[serde(default = "default_enabled")]
    pub kill_supported_browser_if_extension_stale: bool,
    #[serde(default = "default_enabled")]
    pub block_unsupported_browsers: bool,
    #[serde(default = "default_strict_grace_seconds")]
    pub grace_seconds: u32,
}

impl Default for StrictModeConfig {
    fn default() -> Self {
        Self {
            require_firefox_extension: true,
            require_chrome_extension: true,
            kill_supported_browser_if_extension_stale: true,
            block_unsupported_browsers: true,
            grace_seconds: default_strict_grace_seconds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleConfig {
    pub id: String,
    pub name: String,
    pub tier: RuleTier,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub patterns: Vec<RulePatternConfig>,
    #[serde(default)]
    pub schedule_ids: Vec<String>,
    #[serde(default)]
    pub allowance_id: Option<String>,
    #[serde(default)]
    pub unlock_policy: Option<UnlockPolicyConfig>,
}

impl RuleConfig {
    pub fn effective_unlock_policy(&self, defaults: &DefaultsConfig) -> UnlockPolicyConfig {
        self.unlock_policy.unwrap_or(defaults.unlock_policy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppRuleConfig {
    pub id: String,
    pub name: String,
    pub tier: RuleTier,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub matchers: Vec<AppMatcherConfig>,
    #[serde(default)]
    pub schedule_ids: Vec<String>,
    #[serde(default)]
    pub allowance_id: Option<String>,
    #[serde(default)]
    pub unlock_policy: Option<UnlockPolicyConfig>,
}

impl AppRuleConfig {
    pub fn effective_unlock_policy(&self, defaults: &DefaultsConfig) -> UnlockPolicyConfig {
        self.unlock_policy.unwrap_or(defaults.unlock_policy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppMatcherConfig {
    pub kind: AppMatcherKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppMatcherKind {
    ExecutablePath,
    ExecutableBasename,
    CommandName,
    DesktopId,
    WindowTitleExact,
    WindowTitleContains,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleTier {
    Hard,
    #[serde(alias = "controlled")]
    ControlledAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RulePatternConfig {
    pub kind: RulePatternKind,
    pub value: String,
    #[serde(default)]
    pub match_subdomains: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RulePatternKind {
    Domain,
    ExactUrl,
    UrlPrefix,
    PathPrefix,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleConfig {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub windows: Vec<ScheduleWindow>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleWindow {
    pub weekday: ScheduleDay,
    pub start: TimeOfDay,
    pub end: TimeOfDay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllowanceConfig {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub daily_minutes: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnlockPolicyConfig {
    #[serde(default = "default_max_session_minutes")]
    pub max_session_minutes: u32,
    #[serde(default = "default_cooldown_minutes")]
    pub cooldown_minutes: u32,
    #[serde(default = "default_max_unlocks_per_hour")]
    pub max_unlocks_per_hour: u32,
}

impl Default for UnlockPolicyConfig {
    fn default() -> Self {
        Self {
            max_session_minutes: default_max_session_minutes(),
            cooldown_minutes: default_cooldown_minutes(),
            max_unlocks_per_hour: default_max_unlocks_per_hour(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleDay {
    Everyday,
    Workdays,
    Weekend,
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl From<chrono::Weekday> for Weekday {
    fn from(value: chrono::Weekday) -> Self {
        match value {
            chrono::Weekday::Mon => Self::Mon,
            chrono::Weekday::Tue => Self::Tue,
            chrono::Weekday::Wed => Self::Wed,
            chrono::Weekday::Thu => Self::Thu,
            chrono::Weekday::Fri => Self::Fri,
            chrono::Weekday::Sat => Self::Sat,
            chrono::Weekday::Sun => Self::Sun,
        }
    }
}

impl ScheduleDay {
    pub fn includes(self, weekday: Weekday) -> bool {
        match self {
            Self::Everyday => true,
            Self::Workdays => matches!(
                weekday,
                Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
            ),
            Self::Weekend => matches!(weekday, Weekday::Sat | Weekday::Sun),
            Self::Mon => weekday == Weekday::Mon,
            Self::Tue => weekday == Weekday::Tue,
            Self::Wed => weekday == Weekday::Wed,
            Self::Thu => weekday == Weekday::Thu,
            Self::Fri => weekday == Weekday::Fri,
            Self::Sat => weekday == Weekday::Sat,
            Self::Sun => weekday == Weekday::Sun,
        }
    }
}

impl Weekday {
    pub fn previous(self) -> Self {
        match self {
            Self::Mon => Self::Sun,
            Self::Tue => Self::Mon,
            Self::Wed => Self::Tue,
            Self::Thu => Self::Wed,
            Self::Fri => Self::Thu,
            Self::Sat => Self::Fri,
            Self::Sun => Self::Sat,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOfDay {
    hour: u8,
    minute: u8,
}

impl TimeOfDay {
    pub fn new(hour: u8, minute: u8) -> Result<Self, String> {
        if hour > 23 || minute > 59 {
            return Err(format!("invalid time of day: {hour:02}:{minute:02}"));
        }

        Ok(Self { hour, minute })
    }

    pub fn minutes_after_midnight(self) -> u16 {
        u16::from(self.hour) * 60 + u16::from(self.minute)
    }

    pub fn hour(self) -> u8 {
        self.hour
    }

    pub fn minute(self) -> u8 {
        self.minute
    }
}

impl FromStr for TimeOfDay {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((hour, minute)) = value.split_once(':') else {
            return Err(format!("time '{value}' must use HH:MM format"));
        };
        if hour.len() != 2 || minute.len() != 2 {
            return Err(format!("time '{value}' must use zero-padded HH:MM format"));
        }

        let hour = hour
            .parse::<u8>()
            .map_err(|_| format!("invalid hour in time '{value}'"))?;
        let minute = minute
            .parse::<u8>()
            .map_err(|_| format!("invalid minute in time '{value}'"))?;

        Self::new(hour, minute)
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:02}:{:02}", self.hour, self.minute)
    }
}

impl Serialize for TimeOfDay {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TimeOfDay {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(de::Error::custom)
    }
}

fn ensure_unique_ids<'a>(
    label: &str,
    ids: impl Iterator<Item = &'a str>,
) -> Result<(), ConfigError> {
    let mut seen = HashSet::new();
    for id in ids {
        require_identifier(label, id)?;
        if !seen.insert(id) {
            return Err(ConfigError::Validation(format!(
                "duplicate {label} id '{id}'"
            )));
        }
    }
    Ok(())
}

fn require_identifier(label: &str, id: &str) -> Result<(), ConfigError> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::Validation(format!(
            "{label} id must not be empty"
        )));
    }
    if trimmed != id {
        return Err(ConfigError::Validation(format!(
            "{label} id '{id}' must not contain leading or trailing whitespace"
        )));
    }
    Ok(())
}

fn validate_pattern(rule_id: &str, pattern: &RulePatternConfig) -> Result<(), ConfigError> {
    let value = pattern.value.trim();
    if value.is_empty() {
        return Err(ConfigError::Validation(format!(
            "rule '{rule_id}' contains an empty pattern"
        )));
    }
    if value != pattern.value {
        return Err(ConfigError::Validation(format!(
            "rule '{rule_id}' contains a pattern with leading or trailing whitespace"
        )));
    }

    match pattern.kind {
        RulePatternKind::Domain => {
            if value.contains('/') || value.contains("://") {
                return Err(ConfigError::Validation(format!(
                    "domain pattern '{value}' in rule '{rule_id}' must contain only a host name"
                )));
            }
            validate_host_like(rule_id, value)?;
        }
        RulePatternKind::ExactUrl | RulePatternKind::UrlPrefix => {
            Url::parse(value).map_err(|err| {
                ConfigError::Validation(format!(
                    "URL pattern '{value}' in rule '{rule_id}' is invalid: {err}"
                ))
            })?;
        }
        RulePatternKind::PathPrefix => {
            if !value.starts_with('/') && !value.contains('/') {
                return Err(ConfigError::Validation(format!(
                    "path_prefix pattern '{value}' in rule '{rule_id}' must start with '/' or use 'host/path'"
                )));
            }
        }
    }

    Ok(())
}

fn validate_app_matcher(rule_id: &str, matcher: &AppMatcherConfig) -> Result<(), ConfigError> {
    let value = matcher.value.trim();
    if value.is_empty() {
        return Err(ConfigError::Validation(format!(
            "app rule '{rule_id}' contains an empty matcher"
        )));
    }
    if value != matcher.value {
        return Err(ConfigError::Validation(format!(
            "app rule '{rule_id}' contains a matcher with leading or trailing whitespace"
        )));
    }

    match matcher.kind {
        AppMatcherKind::ExecutablePath => {
            if !value.starts_with('/') {
                return Err(ConfigError::Validation(format!(
                    "executable_path matcher '{value}' in app rule '{rule_id}' must be an absolute path"
                )));
            }
        }
        AppMatcherKind::ExecutableBasename
        | AppMatcherKind::CommandName
        | AppMatcherKind::DesktopId => {
            if value.contains('/') {
                return Err(ConfigError::Validation(format!(
                    "{:?} matcher '{value}' in app rule '{rule_id}' must not contain '/'",
                    matcher.kind
                )));
            }
        }
        AppMatcherKind::WindowTitleExact | AppMatcherKind::WindowTitleContains => {}
    }

    Ok(())
}

fn validate_host_like(rule_id: &str, value: &str) -> Result<(), ConfigError> {
    if value.starts_with('.') || value.ends_with('.') || value.contains('*') {
        return Err(ConfigError::Validation(format!(
            "domain pattern '{value}' in rule '{rule_id}' is not a valid structural host match"
        )));
    }
    if !value.contains('.') {
        return Err(ConfigError::Validation(format!(
            "domain pattern '{value}' in rule '{rule_id}' must include a registrable suffix"
        )));
    }
    Ok(())
}

fn validate_unlock_policy(label: &str, policy: &UnlockPolicyConfig) -> Result<(), ConfigError> {
    if policy.max_session_minutes == 0 {
        return Err(ConfigError::Validation(format!(
            "{label}.max_session_minutes must be greater than zero"
        )));
    }
    if policy.max_unlocks_per_hour == 0 {
        return Err(ConfigError::Validation(format!(
            "{label}.max_unlocks_per_hour must be greater than zero"
        )));
    }
    Ok(())
}

fn default_enabled() -> bool {
    true
}

fn default_strict_grace_seconds() -> u32 {
    30
}

fn default_max_session_minutes() -> u32 {
    2
}

fn default_cooldown_minutes() -> u32 {
    0
}

fn default_max_unlocks_per_hour() -> u32 {
    1
}
