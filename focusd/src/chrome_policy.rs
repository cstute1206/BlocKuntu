use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::error::Result;

pub const CHROME_WEB_STORE_UPDATE_URL: &str = "https://clients2.google.com/service/update2/crx";

#[derive(Debug, Clone)]
pub struct ChromePolicyManager {
    policy_path: PathBuf,
    extension_id: String,
    update_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromePolicyRepairStatus {
    AlreadyCompliant,
    Repaired,
    SkippedInactive,
    SkippedDisabled,
    SkippedDeferred,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChromePolicyStatus {
    pub path: String,
    pub extension_id: String,
    pub update_url: String,
    pub policy_exists: bool,
    pub valid_json: bool,
    pub compliant: bool,
    pub force_install_configured: bool,
    pub detail: String,
}

impl ChromePolicyManager {
    pub fn new(policy_path: impl Into<PathBuf>, extension_id: impl Into<String>) -> Self {
        Self {
            policy_path: policy_path.into(),
            extension_id: extension_id.into(),
            update_url: CHROME_WEB_STORE_UPDATE_URL.to_string(),
        }
    }

    pub fn expected_policy(&self) -> Value {
        let force_install_entry = self.force_install_entry();

        json!({
            "DeveloperToolsDisabled": true,
            "ExtensionInstallForcelist": [force_install_entry],
            "ExtensionSettings": {
                self.extension_id.clone(): {
                    "installation_mode": "force_installed",
                    "update_url": self.update_url.clone()
                }
            }
        })
    }

    pub fn verify_and_repair(&self) -> Result<ChromePolicyRepairStatus> {
        let expected_policy = self.expected_policy();
        if file_json_equals(&self.policy_path, &expected_policy)? {
            return Ok(ChromePolicyRepairStatus::AlreadyCompliant);
        }

        self.write_policy(&expected_policy)?;
        Ok(ChromePolicyRepairStatus::Repaired)
    }

    pub fn remove_policy(&self) -> Result<ChromePolicyRepairStatus> {
        let removed = match fs::remove_file(&self.policy_path) {
            Ok(()) => true,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
            Err(err) => return Err(err.into()),
        };

        Ok(if removed {
            ChromePolicyRepairStatus::Repaired
        } else {
            ChromePolicyRepairStatus::AlreadyCompliant
        })
    }

    pub fn status(&self) -> ChromePolicyStatus {
        let expected_policy = self.expected_policy();
        let policy_path = self.policy_path.display().to_string();

        let contents = match fs::read(&self.policy_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return ChromePolicyStatus {
                    path: policy_path,
                    extension_id: self.extension_id.clone(),
                    update_url: self.update_url.clone(),
                    policy_exists: false,
                    valid_json: false,
                    compliant: false,
                    force_install_configured: false,
                    detail: "Chrome policy file is missing".to_string(),
                };
            }
            Err(err) => {
                return ChromePolicyStatus {
                    path: policy_path,
                    extension_id: self.extension_id.clone(),
                    update_url: self.update_url.clone(),
                    policy_exists: true,
                    valid_json: false,
                    compliant: false,
                    force_install_configured: false,
                    detail: format!("Chrome policy file cannot be read: {err}"),
                };
            }
        };

        let parsed = match serde_json::from_slice::<Value>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                return ChromePolicyStatus {
                    path: policy_path,
                    extension_id: self.extension_id.clone(),
                    update_url: self.update_url.clone(),
                    policy_exists: true,
                    valid_json: false,
                    compliant: false,
                    force_install_configured: false,
                    detail: format!("Chrome policy file is not valid JSON: {err}"),
                };
            }
        };

        let force_install_configured = self.force_install_configured(&parsed);
        let extension_settings = parsed
            .get("ExtensionSettings")
            .and_then(|settings| settings.get(&self.extension_id));
        let settings_force_installed = extension_settings
            .and_then(|settings| settings.get("installation_mode"))
            .and_then(Value::as_str)
            == Some("force_installed");
        let settings_update_url = extension_settings
            .and_then(|settings| settings.get("update_url"))
            .and_then(Value::as_str)
            == Some(self.update_url.as_str());
        let compliant = parsed == expected_policy;
        let detail = if compliant {
            "Chrome Web Store policy matches expected force-install settings".to_string()
        } else if !force_install_configured {
            "Chrome Web Store force-install entry is missing or points at a different update URL"
                .to_string()
        } else if !settings_force_installed || !settings_update_url {
            "Chrome ExtensionSettings are missing force_installed/update_url settings".to_string()
        } else {
            "Chrome policy differs from expected hardened settings".to_string()
        };

        ChromePolicyStatus {
            path: policy_path,
            extension_id: self.extension_id.clone(),
            update_url: self.update_url.clone(),
            policy_exists: true,
            valid_json: true,
            compliant,
            force_install_configured,
            detail,
        }
    }

    fn force_install_entry(&self) -> String {
        format!("{};{}", self.extension_id, self.update_url)
    }

    fn force_install_configured(&self, policy: &Value) -> bool {
        let expected = self.force_install_entry();
        policy
            .get("ExtensionInstallForcelist")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.as_str() == Some(&expected))
            })
            .unwrap_or(false)
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
        let result = write_json_atomically(&self.policy_path, &temporary_path, policy);
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }
}

fn file_json_equals(path: &Path, expected: &Value) -> Result<bool> {
    match fs::read(path) {
        Ok(contents) => {
            Ok(serde_json::from_slice::<Value>(&contents).ok().as_ref() == Some(expected))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.into()),
    }
}

fn write_json_atomically(policy_path: &Path, temporary_path: &Path, policy: &Value) -> Result<()> {
    let mut file = open_temporary_file(temporary_path)?;
    serde_json::to_writer_pretty(&mut file, policy)?;
    file.write_all(b"\n")?;
    finish_atomic_write(file, temporary_path, policy_path)
}

fn open_temporary_file(path: &Path) -> Result<fs::File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o644))?;
    Ok(file)
}

fn finish_atomic_write(file: fs::File, temporary_path: &Path, path: &Path) -> Result<()> {
    file.sync_all()?;
    drop(file);

    fs::rename(temporary_path, path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644))?;
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("blockuntu-chrome-policy");
    path.with_file_name(format!(".{file_name}.blockuntu.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{ChromePolicyManager, ChromePolicyRepairStatus};

    #[test]
    fn repairs_missing_policy_for_the_chrome_web_store() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let policy_path = temp.path().join("chrome/policies/managed/blockuntu.json");
        let manager = ChromePolicyManager::new(&policy_path, "opfljaancedgklbpnbpjfhdbbhbfpnoc");

        assert_eq!(
            manager.verify_and_repair().expect("repair should succeed"),
            ChromePolicyRepairStatus::Repaired
        );
        assert_eq!(
            manager
                .verify_and_repair()
                .expect("second repair should succeed"),
            ChromePolicyRepairStatus::AlreadyCompliant
        );

        let policy = std::fs::read_to_string(&policy_path).expect("policy should exist");
        let parsed: Value = serde_json::from_str(&policy).expect("policy should parse");
        assert_eq!(
            parsed["ExtensionInstallForcelist"][0],
            "opfljaancedgklbpnbpjfhdbbhbfpnoc;https://clients2.google.com/service/update2/crx"
        );
        assert_eq!(
            parsed["ExtensionSettings"]["opfljaancedgklbpnbpjfhdbbhbfpnoc"]["installation_mode"],
            "force_installed"
        );
        assert_eq!(
            parsed["ExtensionSettings"]["opfljaancedgklbpnbpjfhdbbhbfpnoc"]["update_url"],
            "https://clients2.google.com/service/update2/crx"
        );

        let status = manager.status();
        assert!(status.compliant);
        assert!(status.force_install_configured);
    }
}
