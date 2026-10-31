use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use focus_core::Config;

use crate::error::{DaemonError, Result};

#[derive(Debug, Clone)]
pub struct PolicyRecoveryManager {
    path: PathBuf,
    enforce_immutable: bool,
}

/// Keep the lock inode separate from the snapshot, which is replaced by rename.
/// Closing the descriptor releases the lock, including when a process exits.
pub(crate) struct PolicyRecoveryGuard<'a> {
    manager: &'a PolicyRecoveryManager,
    _lock: File,
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
        self.lock()?.write(config)
    }

    fn parent(&self) -> Result<&Path> {
        let parent = self.path.parent().ok_or_else(|| {
            DaemonError::InvalidRequest(format!(
                "policy recovery path has no parent: {}",
                self.path.display()
            ))
        })?;
        Ok(if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        })
    }

    /// Callers updating SQLite also hold this guard through commit or rollback.
    pub(crate) fn lock(&self) -> Result<PolicyRecoveryGuard<'_>> {
        let parent = self.parent()?;
        fs::create_dir_all(parent)
            .map_err(|error| io_error("create recovery directory", parent, error))?;
        let name = self.path.file_name().ok_or_else(|| {
            DaemonError::InvalidRequest("policy recovery path has no filename".to_string())
        })?;
        let mut lock_name = std::ffi::OsString::from(".");
        lock_name.push(name);
        lock_name.push(".lock");
        let lock_path = parent.join(lock_name);
        // Never unlink this file: another process may still hold its inode.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&lock_path)
            .map_err(|error| io_error("open recovery lock", &lock_path, error))?;
        loop {
            // SAFETY: lock owns this valid descriptor until the guard is dropped.
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
                break;
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(io_error("lock policy recovery", &lock_path, error));
            }
        }
        Ok(PolicyRecoveryGuard {
            manager: self,
            _lock: lock,
        })
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

impl PolicyRecoveryGuard<'_> {
    pub(crate) fn write(&self, config: &Config) -> Result<()> {
        let contents = config.to_toml_string()?;
        let destination = self.manager.path();
        let parent = self.manager.parent()?;
        // Stage and sync a private, uniquely owned file before clearing +i.
        // NamedTempFile removes only this write's file on an error.
        let mut staged = tempfile::Builder::new()
            .prefix(".blockuntu-policy-")
            .suffix(".tmp")
            .tempfile_in(parent)
            .map_err(|error| io_error("stage recovery snapshot", destination, error))?;
        staged
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| io_error("set recovery permissions", destination, error))?;
        staged
            .write_all(contents.as_bytes())
            .map_err(|error| io_error("write recovery snapshot", destination, error))?;
        staged
            .as_file()
            .sync_all()
            .map_err(|error| io_error("sync recovery snapshot", destination, error))?;

        self.manager.clear_immutable_if_needed()?;
        let replacement = staged
            .persist(destination)
            .map_err(|error| io_error("replace recovery snapshot", destination, error.error));
        let protection = if replacement.is_ok() || destination.exists() {
            self.manager.set_immutable_if_needed()
        } else {
            Ok(())
        };
        if let Err(error) = protection {
            return Err(match replacement {
                Err(write_error) => DaemonError::InvalidRequest(format!(
                    "{write_error}; restoring recovery protection also failed: {error}"
                )),
                Ok(_) => error,
            });
        }
        let file = replacement?;
        file.sync_all()
            .map_err(|error| io_error("sync protected recovery snapshot", destination, error))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error("sync recovery directory", parent, error))?;
        Ok(())
    }
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> DaemonError {
    std::io::Error::new(
        error.kind(),
        format!("{action} at {}: {error}", path.display()),
    )
    .into()
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
    fn writes_leave_foreign_staging_files_intact() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("policy.toml");
        let foreign = temp.path().join(".policy.toml.tmp");
        fs::write(&foreign, "another writer's staged policy").unwrap();
        let manager = PolicyRecoveryManager::new(&path, false);
        manager.write(&Config::default()).unwrap();
        manager.write(&Config::default()).unwrap();
        assert_eq!(
            fs::read_to_string(foreign).unwrap(),
            "another writer's staged policy"
        );
    }

    #[test]
    fn failed_replace_removes_only_the_owned_staging_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("policy.toml");
        fs::create_dir(&path).unwrap(); // Force rename to fail after staging.
        let foreign = temp.path().join(".policy.toml.tmp");
        fs::write(&foreign, "other writer").unwrap();
        let manager = PolicyRecoveryManager::new(&path, false);
        let error = manager.write(&Config::default()).unwrap_err();
        assert!(error.to_string().contains("replace recovery snapshot"));
        assert!(error.to_string().contains(&path.display().to_string()));
        assert!(path.is_dir());
        assert_eq!(fs::read_to_string(foreign).unwrap(), "other writer");
        assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".blockuntu-policy-")));
        // An error must release the lock so a later corrected write can proceed.
        fs::remove_dir(&path).unwrap();
        manager.write(&Config::default()).unwrap();
    }

    #[test]
    fn pr_data_004_recovery_manager_preserves_website_and_application_allowlist_modes() {
        let temp = tempfile::tempdir().unwrap();
        let manager = PolicyRecoveryManager::new(temp.path().join("policy.toml"), false);
        let policy = Config::from_toml_str(
            r#"
            [[rules]]
            id = "sites"
            name = "Sites"
            tier = "controlled_access"
            mode = "allowlist"
            patterns = [{ kind = "domain", value = "work.test", match_subdomains = true }]
            [[app_rules]]
            id = "apps"
            name = "Apps"
            tier = "scheduled_block"
            mode = "allowlist"
            matchers = [{ kind = "command_name", value = "editor" }]
        "#,
        )
        .unwrap();
        manager.write(&policy).unwrap();
        assert_eq!(manager.load().unwrap().unwrap(), policy);
    }

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
