use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use focus_core::Config;

use crate::error::{DaemonError, Result};

#[derive(Debug, Clone)]
pub struct PolicyRecoveryManager {
    path: PathBuf,
    enforce_immutable: bool,
}

impl PolicyRecoveryManager {
    pub fn new(path: impl Into<PathBuf>, enforce_immutable: bool) -> Self {
        Self {
            path: path.into(),
            enforce_immutable,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<Config>> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => match Config::from_toml_str(&contents) {
                Ok(config) => Ok(Some(config)),
                Err(_) => Ok(Some(Config::from_legacy_recovery_toml_str(&contents)?)),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn write(&self, config: &Config) -> Result<()> {
        let contents = config.to_toml_string()?;
        let parent = self.path.parent().ok_or_else(|| {
            DaemonError::InvalidRequest(format!(
                "policy recovery path has no parent: {}",
                self.path.display()
            ))
        })?;
        fs::create_dir_all(parent)?;

        self.clear_immutable_if_needed()?;
        let temp_path = recovery_temp_path(&self.path);
        let result = write_atomically(&temp_path, &self.path, contents.as_bytes());

        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
            let _ = self.set_immutable_if_needed();
            return result;
        }

        self.set_immutable_if_needed()
    }

    fn clear_immutable_if_needed(&self) -> Result<()> {
        if self.enforce_immutable && self.path.exists() {
            self.set_immutable(false)?;
        }
        Ok(())
    }

    fn set_immutable_if_needed(&self) -> Result<()> {
        if self.enforce_immutable {
            self.set_immutable(true)?;
        }
        Ok(())
    }

    fn set_immutable(&self, enabled: bool) -> Result<()> {
        let flag = if enabled { "+i" } else { "-i" };
        let output = Command::new("chattr").arg(flag).arg(&self.path).output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error("chattr", flag, &self.path, output))
        }
    }
}

fn recovery_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("policy-recovery.toml");
    path.with_file_name(format!(".{file_name}.tmp"))
}

fn write_atomically(temp_path: &Path, destination: &Path, contents: &[u8]) -> Result<()> {
    if temp_path.exists() {
        fs::remove_file(temp_path)?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(temp_path, destination)?;

    if let Some(parent) = destination.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn command_error(command: &str, flag: &str, path: &Path, output: Output) -> DaemonError {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    DaemonError::InvalidRequest(format!(
        "{command} {flag} failed for {}{}",
        path.display(),
        if stderr.is_empty() {
            String::new()
        } else {
            format!(": {stderr}")
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_snapshot_roundtrips_and_replaces_atomically() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let path = temp.path().join("policy-recovery.toml");
        let manager = PolicyRecoveryManager::new(&path, false);
        let first = Config::from_toml_str(
            r#"
            [[rules]]
            id = "first"
            name = "First"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "first.example", match_subdomains = true }
            ]
            "#,
        )
        .expect("first config should parse");
        let second = Config::from_toml_str(
            r#"
            [[rules]]
            id = "second"
            name = "Second"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "second.example", match_subdomains = true }
            ]
            "#,
        )
        .expect("second config should parse");

        assert!(manager
            .load()
            .expect("missing snapshot should load")
            .is_none());
        manager.write(&first).expect("first snapshot should write");
        assert_eq!(
            manager.load().expect("first snapshot should load"),
            Some(first)
        );

        manager
            .write(&second)
            .expect("second snapshot should write");
        assert_eq!(
            manager.load().expect("second snapshot should load"),
            Some(second)
        );
        assert_eq!(
            fs::metadata(path)
                .expect("snapshot metadata should exist")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn loads_legacy_recovery_snapshot_without_unlock_policy_fields() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let path = temp.path().join("policy-recovery.toml");
        fs::write(
            &path,
            r#"
            [defaults.unlock_policy]
            max_session_minutes = 8
            cooldown_minutes = 20
            max_unlocks_per_hour = 3

            [[rules]]
            id = "legacy"
            name = "Legacy"
            tier = "controlled_access"
            patterns = [
              { kind = "domain", value = "legacy.example", match_subdomains = true }
            ]

            [rules.unlock_policy]
            max_session_minutes = 4
            cooldown_minutes = 5
            max_unlocks_per_hour = 2
            "#,
        )
        .expect("legacy snapshot should write");
        let manager = PolicyRecoveryManager::new(&path, false);

        let config = manager
            .load()
            .expect("legacy snapshot should load")
            .expect("legacy snapshot should exist");

        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id, "legacy");
        assert!(!config
            .to_toml_string()
            .expect("migrated snapshot should serialize")
            .contains("unlock_policy"));
    }
}
