use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use url::Url;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct FirefoxPolicyManager {
    policy_path: PathBuf,
    extension_id: String,
    extension_xpi: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairStatus {
    AlreadyCompliant,
    Repaired,
}

impl FirefoxPolicyManager {
    pub fn new(
        policy_path: impl Into<PathBuf>,
        extension_id: impl Into<String>,
        extension_xpi: impl Into<PathBuf>,
    ) -> Self {
        Self {
            policy_path: policy_path.into(),
            extension_id: extension_id.into(),
            extension_xpi: extension_xpi.into(),
        }
    }

    pub fn policy_path(&self) -> &Path {
        &self.policy_path
    }

    pub fn expected_policy(&self) -> Value {
        let install_url = file_url(&self.extension_xpi);

        json!({
            "policies": {
                "BlockAboutConfig": true,
                "BlockAboutProfiles": true,
                "BlockAboutSupport": true,
                "DisableDeveloperTools": true,
                "DisableSafeMode": true,
                "ExtensionSettings": {
                    self.extension_id.clone(): {
                        "installation_mode": "force_installed",
                        "install_url": install_url,
                        "default_area": "navbar",
                        "private_browsing": true
                    }
                },
                "Preferences": {
                    "extensions.quarantinedDomains.enabled": {
                        "Value": false,
                        "Status": "locked"
                    }
                }
            }
        })
    }

    pub fn verify_and_repair(&self) -> Result<RepairStatus> {
        let expected = self.expected_policy();
        match fs::read(&self.policy_path) {
            Ok(contents) => {
                if serde_json::from_slice::<Value>(&contents).ok() == Some(expected.clone()) {
                    Ok(RepairStatus::AlreadyCompliant)
                } else {
                    self.write_policy(&expected)?;
                    Ok(RepairStatus::Repaired)
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                self.write_policy(&expected)?;
                Ok(RepairStatus::Repaired)
            }
            Err(err) => Err(err.into()),
        }
    }

    fn write_policy(&self, policy: &Value) -> Result<()> {
        let parent = self.policy_path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("policy path has no parent: {}", self.policy_path.display()),
            )
        })?;
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?;

        let temporary_path = temporary_path(&self.policy_path);
        let result = write_policy_atomically(&self.policy_path, &temporary_path, policy);
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }
}

fn write_policy_atomically(
    policy_path: &Path,
    temporary_path: &Path,
    policy: &Value,
) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(temporary_path)?;
    file.set_permissions(fs::Permissions::from_mode(0o644))?;
    serde_json::to_writer_pretty(&mut file, policy)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);

    fs::rename(temporary_path, policy_path)?;
    fs::set_permissions(policy_path, fs::Permissions::from_mode(0o644))?;
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("policies.json");
    path.with_file_name(format!(".{file_name}.blockuntu.{}.tmp", std::process::id()))
}

fn file_url(path: &Path) -> String {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{FirefoxPolicyManager, RepairStatus};

    #[test]
    fn repairs_missing_policy_and_then_accepts_it() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let policy_path = temp.path().join("firefox/policies/policies.json");
        let manager = FirefoxPolicyManager::new(
            &policy_path,
            "blockuntu@example.local",
            "/usr/local/share/blockuntu/BlocKuntu.xpi",
        );

        assert_eq!(
            manager.verify_and_repair().expect("repair should work"),
            RepairStatus::Repaired
        );
        assert_eq!(
            manager
                .verify_and_repair()
                .expect("second check should pass"),
            RepairStatus::AlreadyCompliant
        );

        let policy = std::fs::read_to_string(policy_path).expect("policy should exist");
        assert!(policy.contains("\"force_installed\""));
        assert!(policy.contains("\"private_browsing\""));
        assert!(policy.contains("\"DisableDeveloperTools\""));
    }
}
