use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::Result;

pub const CHROME_WEB_STORE_UPDATE_URL: &str = "https://clients2.google.com/service/update2/crx";

/// How BlocKuntu handles private windows in Chromium-family browsers.
///
/// Chromium deliberately keeps extension access to Incognito behind a user-consent
/// toggle. The first two variants preserve that boundary; the third uses a browser
/// URL policy rather than trying to override the toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChromiumIncognitoMode {
    Disabled,
    ManualConsent,
    PolicyUrlBlocking,
}

impl Default for ChromiumIncognitoMode {
    fn default() -> Self {
        Self::ManualConsent
    }
}

#[derive(Debug, Clone)]
pub struct ChromePolicyManager {
    policy_path: PathBuf,
    extension_id: String,
    update_url: String,
    browser_name: String,
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
    pub browser: String,
    pub path: String,
    pub extension_id: String,
    pub update_url: String,
    pub policy_exists: bool,
    pub valid_json: bool,
    pub compliant: bool,
    pub force_install_configured: bool,
    pub incognito_mode: ChromiumIncognitoMode,
    pub incognito_mode_configured: bool,
    pub incognito_url_block_count: usize,
    pub detail: String,
}

impl ChromePolicyManager {
    pub fn new(policy_path: impl Into<PathBuf>, extension_id: impl Into<String>) -> Self {
        Self::for_browser(policy_path, extension_id, "Chrome")
    }

    pub fn for_browser(
        policy_path: impl Into<PathBuf>,
        extension_id: impl Into<String>,
        browser_name: impl Into<String>,
    ) -> Self {
        Self {
            policy_path: policy_path.into(),
            extension_id: extension_id.into(),
            update_url: CHROME_WEB_STORE_UPDATE_URL.to_string(),
            browser_name: browser_name.into(),
        }
    }

    pub fn expected_policy(&self) -> Value {
        self.expected_policy_for(ChromiumIncognitoMode::ManualConsent, &[])
    }

    pub fn expected_policy_for(
        &self,
        incognito_mode: ChromiumIncognitoMode,
        incognito_url_blocklist: &[String],
    ) -> Value {
        let force_install_entry = self.force_install_entry();

        let mut policy = json!({
            "DeveloperToolsDisabled": true,
            "ExtensionInstallForcelist": [force_install_entry],
            "ExtensionSettings": {
                self.extension_id.clone(): {
                    "installation_mode": "force_installed",
                    "update_url": self.update_url.clone()
                }
            }
        });

        let object = policy
            .as_object_mut()
            .expect("Chrome policy root must be an object");
        match incognito_mode {
            ChromiumIncognitoMode::Disabled => {
                object.insert(self.incognito_mode_availability_key().to_string(), json!(1));
            }
            ChromiumIncognitoMode::ManualConsent => {}
            ChromiumIncognitoMode::PolicyUrlBlocking => {
                object.insert(self.incognito_mode_availability_key().to_string(), json!(0));
                object.insert(
                    self.incognito_url_blocklist_key().to_string(),
                    json!(incognito_url_blocklist),
                );
            }
        }

        policy
    }

    pub fn verify_and_repair(&self) -> Result<ChromePolicyRepairStatus> {
        self.verify_and_repair_with(ChromiumIncognitoMode::ManualConsent, &[])
    }

    pub fn verify_and_repair_with(
        &self,
        incognito_mode: ChromiumIncognitoMode,
        incognito_url_blocklist: &[String],
    ) -> Result<ChromePolicyRepairStatus> {
        let expected_policy = self.expected_policy_for(incognito_mode, incognito_url_blocklist);
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
        self.status_with(ChromiumIncognitoMode::ManualConsent, &[])
    }

    pub fn status_with(
        &self,
        incognito_mode: ChromiumIncognitoMode,
        incognito_url_blocklist: &[String],
    ) -> ChromePolicyStatus {
        let expected_policy = self.expected_policy_for(incognito_mode, incognito_url_blocklist);
        let policy_path = self.policy_path.display().to_string();

        let contents = match fs::read(&self.policy_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return ChromePolicyStatus {
                    browser: self.browser_name.clone(),
                    path: policy_path,
                    extension_id: self.extension_id.clone(),
                    update_url: self.update_url.clone(),
                    policy_exists: false,
                    valid_json: false,
                    compliant: false,
                    force_install_configured: false,
                    incognito_mode,
                    incognito_mode_configured: false,
                    incognito_url_block_count: 0,
                    detail: format!("{} policy file is missing", self.browser_name),
                };
            }
            Err(err) => {
                return ChromePolicyStatus {
                    browser: self.browser_name.clone(),
                    path: policy_path,
                    extension_id: self.extension_id.clone(),
                    update_url: self.update_url.clone(),
                    policy_exists: true,
                    valid_json: false,
                    compliant: false,
                    force_install_configured: false,
                    incognito_mode,
                    incognito_mode_configured: false,
                    incognito_url_block_count: 0,
                    detail: format!("{} policy file cannot be read: {err}", self.browser_name),
                };
            }
        };

        let parsed = match serde_json::from_slice::<Value>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                return ChromePolicyStatus {
                    browser: self.browser_name.clone(),
                    path: policy_path,
                    extension_id: self.extension_id.clone(),
                    update_url: self.update_url.clone(),
                    policy_exists: true,
                    valid_json: false,
                    compliant: false,
                    force_install_configured: false,
                    incognito_mode,
                    incognito_mode_configured: false,
                    incognito_url_block_count: 0,
                    detail: format!("{} policy file is not valid JSON: {err}", self.browser_name),
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
        let expected_incognito_urls: Vec<Value> = incognito_url_blocklist
            .iter()
            .cloned()
            .map(Value::String)
            .collect();
        let compliant = parsed == expected_policy;
        let incognito_mode_configured = match incognito_mode {
            ChromiumIncognitoMode::Disabled => {
                parsed
                    .get(self.incognito_mode_availability_key())
                    .and_then(Value::as_i64)
                    == Some(1)
            }
            ChromiumIncognitoMode::ManualConsent => {
                parsed.get(self.incognito_mode_availability_key()).is_none()
                    && parsed.get(self.incognito_url_blocklist_key()).is_none()
            }
            ChromiumIncognitoMode::PolicyUrlBlocking => {
                parsed
                    .get(self.incognito_mode_availability_key())
                    .and_then(Value::as_i64)
                    == Some(0)
                    && parsed
                        .get(self.incognito_url_blocklist_key())
                        .and_then(Value::as_array)
                        .map(|urls| urls == &expected_incognito_urls)
                        .unwrap_or(false)
            }
        };
        let incognito_detail = match incognito_mode {
            ChromiumIncognitoMode::Disabled => "private browsing is disabled by policy".to_string(),
            ChromiumIncognitoMode::ManualConsent => {
                "private browsing remains available; the user must allow the extension there"
                    .to_string()
            }
            ChromiumIncognitoMode::PolicyUrlBlocking => format!(
                "{} active URL pattern(s) are blocked in private browsing by policy",
                incognito_url_blocklist.len()
            ),
        };
        let detail = if compliant {
            format!(
                "{} Chrome Web Store policy matches expected force-install settings; {incognito_detail}",
                self.browser_name,
            )
        } else if !force_install_configured {
            format!(
                "{} Chrome Web Store force-install entry is missing or points at a different update URL",
                self.browser_name
            )
        } else if !settings_force_installed || !settings_update_url {
            format!(
                "{} ExtensionSettings are missing force_installed/update_url settings",
                self.browser_name
            )
        } else {
            format!(
                "{} policy differs from expected hardened settings; {incognito_detail}",
                self.browser_name,
            )
        };

        ChromePolicyStatus {
            browser: self.browser_name.clone(),
            path: policy_path,
            extension_id: self.extension_id.clone(),
            update_url: self.update_url.clone(),
            policy_exists: true,
            valid_json: true,
            compliant,
            force_install_configured,
            incognito_mode,
            incognito_mode_configured,
            incognito_url_block_count: parsed
                .get(self.incognito_url_blocklist_key())
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
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

    fn incognito_mode_availability_key(&self) -> &'static str {
        if self.browser_name == "Microsoft Edge" {
            "InPrivateModeAvailability"
        } else {
            "IncognitoModeAvailability"
        }
    }

    fn incognito_url_blocklist_key(&self) -> &'static str {
        if self.browser_name == "Microsoft Edge" {
            "InPrivateModeUrlBlocklist"
        } else {
            "IncognitoModeUrlBlocklist"
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

    use super::{ChromePolicyManager, ChromePolicyRepairStatus, ChromiumIncognitoMode};

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

    #[test]
    fn writes_each_private_browsing_policy_variant() {
        let chrome = ChromePolicyManager::for_browser("/tmp/chrome.json", "extension", "Chrome");
        let edge =
            ChromePolicyManager::for_browser("/tmp/edge.json", "extension", "Microsoft Edge");
        let urls = vec!["blocked.example".to_string(), ".exact.example".to_string()];

        assert_eq!(
            chrome.expected_policy_for(ChromiumIncognitoMode::Disabled, &[])
                ["IncognitoModeAvailability"],
            1
        );
        let chrome_policy =
            chrome.expected_policy_for(ChromiumIncognitoMode::PolicyUrlBlocking, &urls);
        assert_eq!(chrome_policy["IncognitoModeAvailability"], 0);
        assert_eq!(
            chrome_policy["IncognitoModeUrlBlocklist"],
            serde_json::json!(["blocked.example", ".exact.example"])
        );

        let edge_policy = edge.expected_policy_for(ChromiumIncognitoMode::PolicyUrlBlocking, &urls);
        assert_eq!(edge_policy["InPrivateModeAvailability"], 0);
        assert_eq!(
            edge_policy["InPrivateModeUrlBlocklist"],
            serde_json::json!(["blocked.example", ".exact.example"])
        );
    }
}
