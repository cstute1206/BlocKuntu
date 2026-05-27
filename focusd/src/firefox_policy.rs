use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Serialize;
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
    SkippedStopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirefoxPolicyStatus {
    pub path: String,
    pub extension_id: String,
    pub extension_xpi: String,
    pub extension_xpi_exists: bool,
    pub policy_exists: bool,
    pub valid_json: bool,
    pub compliant: bool,
    pub private_browsing_enabled: bool,
    pub private_browsing_available: bool,
    pub install_url: Option<String>,
    pub detail: String,
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
                "PrivateBrowsingModeAvailability": 0,
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

    pub fn remove_policy(&self) -> Result<RepairStatus> {
        match fs::remove_file(&self.policy_path) {
            Ok(()) => Ok(RepairStatus::Repaired),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Ok(RepairStatus::AlreadyCompliant)
            }
            Err(err) => Err(err.into()),
        }
    }

    pub fn status(&self) -> FirefoxPolicyStatus {
        let expected = self.expected_policy();
        let extension_xpi_exists = self.extension_xpi.exists();
        let path = self.policy_path.display().to_string();
        let extension_xpi = self.extension_xpi.display().to_string();

        let contents = match fs::read(&self.policy_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return FirefoxPolicyStatus {
                    path,
                    extension_id: self.extension_id.clone(),
                    extension_xpi,
                    extension_xpi_exists,
                    policy_exists: false,
                    valid_json: false,
                    compliant: false,
                    private_browsing_enabled: false,
                    private_browsing_available: false,
                    install_url: None,
                    detail: "policy file is missing".to_string(),
                }
            }
            Err(err) => {
                return FirefoxPolicyStatus {
                    path,
                    extension_id: self.extension_id.clone(),
                    extension_xpi,
                    extension_xpi_exists,
                    policy_exists: true,
                    valid_json: false,
                    compliant: false,
                    private_browsing_enabled: false,
                    private_browsing_available: false,
                    install_url: None,
                    detail: format!("policy file cannot be read: {err}"),
                }
            }
        };

        let parsed = match serde_json::from_slice::<Value>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                return FirefoxPolicyStatus {
                    path,
                    extension_id: self.extension_id.clone(),
                    extension_xpi,
                    extension_xpi_exists,
                    policy_exists: true,
                    valid_json: false,
                    compliant: false,
                    private_browsing_enabled: false,
                    private_browsing_available: false,
                    install_url: None,
                    detail: format!("policy file is not valid JSON: {err}"),
                }
            }
        };

        let extension_settings = parsed
            .get("policies")
            .and_then(|policies| policies.get("ExtensionSettings"))
            .and_then(|settings| settings.get(&self.extension_id));
        let private_browsing_enabled = extension_settings
            .and_then(|settings| settings.get("private_browsing"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let install_url = extension_settings
            .and_then(|settings| settings.get("install_url"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let private_browsing_available = parsed
            .get("policies")
            .and_then(|policies| policies.get("PrivateBrowsingModeAvailability"))
            .and_then(Value::as_i64)
            == Some(0);
        let compliant = parsed == expected;
        let detail = if compliant {
            "policy matches expected hardened settings".to_string()
        } else {
            "policy differs from expected hardened settings".to_string()
        };

        FirefoxPolicyStatus {
            path,
            extension_id: self.extension_id.clone(),
            extension_xpi,
            extension_xpi_exists,
            policy_exists: true,
            valid_json: true,
            compliant,
            private_browsing_enabled,
            private_browsing_available,
            install_url,
            detail,
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
        assert!(policy.contains("\"PrivateBrowsingModeAvailability\""));
        assert!(policy.contains("\"DisableDeveloperTools\""));

        let status = manager.status();
        assert!(status.compliant);
        assert!(status.private_browsing_enabled);
        assert!(status.private_browsing_available);
    }
}
