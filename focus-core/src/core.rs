use chrono::{DateTime, FixedOffset, Local};

use crate::{
    evaluate_url, load_config, request_unlock, validate_config, Config, Database, Decision, Error,
    EvaluationContext, UnlockState,
};

pub struct FocusCore {
    config: Config,
    database: Database,
}

impl FocusCore {
    pub fn new(config: Config, database: Database) -> Result<Self, Error> {
        validate_config(&config)?;
        Ok(Self { config, database })
    }

    pub fn open(
        config_path: impl AsRef<std::path::Path>,
        db_path: impl AsRef<std::path::Path>,
    ) -> Result<Self, Error> {
        let database = Database::open(db_path)?;
        let config = if database.has_policy_config()? {
            database.load_policy_config()?
        } else {
            let config = load_config(config_path)?;
            database.replace_policy_config(&config)?;
            config
        };
        Self::new(config, database)
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn replace_config(&mut self, config: Config) -> Result<(), Error> {
        validate_config(&config)?;
        self.config = config;
        Ok(())
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn evaluate_url(&self, url: &str, now: DateTime<FixedOffset>) -> Decision {
        let context = EvaluationContext::new(&self.config, &self.database, now);
        evaluate_url(url, &context)
    }

    pub fn evaluate_url_now(&self, url: &str) -> Decision {
        self.evaluate_url(url, Local::now().fixed_offset())
    }

    pub fn request_unlock(
        &self,
        target: &str,
        minutes: u32,
        reason: String,
    ) -> Result<UnlockState, Error> {
        self.request_unlock_at(target, minutes, reason, Local::now().fixed_offset())
    }

    pub fn request_unlock_at(
        &self,
        target: &str,
        minutes: u32,
        reason: String,
        now: DateTime<FixedOffset>,
    ) -> Result<UnlockState, Error> {
        let context = EvaluationContext::new(&self.config, &self.database, now);
        request_unlock(target, minutes, reason, &context)
    }
}
