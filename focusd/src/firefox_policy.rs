use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct FirefoxPolicyManager {
    policy_path: PathBuf,
    extension_id: String,
    install_url: String,
    merge_with_existing: bool,
    backup_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MergedPolicyBackup {
    policy_was_missing: bool,
    policy_values: Vec<(String, Option<Value>)>,
    extension_setting: Option<Value>,
    preference_setting: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairStatus {
    AlreadyCompliant,
    Repaired,
    SkippedInactive,
    SkippedDisabled,
    SkippedDeferred,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirefoxPolicyStatus {
    pub path: String,
    pub extension_id: String,
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
        install_url: impl Into<String>,
    ) -> Self {
        Self::for_browser(policy_path, extension_id, install_url)
    }

    pub fn for_browser(
        policy_path: impl Into<PathBuf>,
        extension_id: impl Into<String>,
        install_url: impl Into<String>,
    ) -> Self {
        Self {
            policy_path: policy_path.into(),
            extension_id: extension_id.into(),
            install_url: install_url.into(),
            merge_with_existing: false,
            backup_path: None,
        }
    }

    /// Creates a policy manager for Firefox-family browsers that ship their own
    /// policy document. BlocKuntu changes only its required entries and records
    /// the prior values so uninstall can restore them without discarding the
    /// browser's defaults.
    pub fn merging_existing_policy(
        policy_path: impl Into<PathBuf>,
        extension_id: impl Into<String>,
        install_url: impl Into<String>,
        backup_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            policy_path: policy_path.into(),
            extension_id: extension_id.into(),
            install_url: install_url.into(),
            merge_with_existing: true,
            backup_path: Some(backup_path.into()),
        }
    }

    pub fn policy_path(&self) -> &Path {
        &self.policy_path
    }

    pub fn expected_policy(&self) -> Value {
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
                        "install_url": self.install_url.clone(),
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
        if self.merge_with_existing {
            return self.merge_and_repair(&expected);
        }
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
        if self.merge_with_existing {
            return self.remove_merged_policy();
        }
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
        let path = self.policy_path.display().to_string();

        let contents = match fs::read(&self.policy_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return FirefoxPolicyStatus {
                    path,
                    extension_id: self.extension_id.clone(),
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
        let compliant = if self.merge_with_existing {
            policy_includes_managed_settings(&parsed, &expected, &self.extension_id)
        } else {
            parsed == expected
        };
        let detail = if compliant {
            if self.merge_with_existing {
                "policy contains BlocKuntu's hardened settings and preserves browser defaults"
                    .to_string()
            } else {
                "policy matches expected hardened settings".to_string()
            }
        } else {
            if self.merge_with_existing {
                "policy is missing or changes BlocKuntu's hardened settings".to_string()
            } else {
                "policy differs from expected hardened settings".to_string()
            }
        };

        FirefoxPolicyStatus {
            path,
            extension_id: self.extension_id.clone(),
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

    fn merge_and_repair(&self, expected: &Value) -> Result<RepairStatus> {
        let existing = match fs::read(&self.policy_path) {
            Ok(contents) => Some(serde_json::from_slice::<Value>(&contents).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "cannot merge BlocKuntu policy because {} is not valid JSON: {error}",
                        self.policy_path.display()
                    ),
                )
            })?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };

        if existing.as_ref().is_some_and(|policy| {
            policy_includes_managed_settings(policy, expected, &self.extension_id)
        }) {
            return Ok(RepairStatus::AlreadyCompliant);
        }

        self.ensure_merge_backup(existing.as_ref())?;
        let merged = match existing {
            Some(policy) => merge_managed_settings(policy, expected, &self.extension_id)?,
            None => expected.clone(),
        };
        self.write_policy(&merged)?;
        Ok(RepairStatus::Repaired)
    }

    fn remove_merged_policy(&self) -> Result<RepairStatus> {
        let backup_path = self.backup_path.as_ref().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "merged Firefox policy has no backup path",
            )
        })?;
        let backup = match fs::read(backup_path) {
            Ok(contents) => {
                serde_json::from_slice::<MergedPolicyBackup>(&contents).map_err(|error| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "cannot restore browser policy because {} is not valid JSON: {error}",
                            backup_path.display()
                        ),
                    )
                })?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RepairStatus::AlreadyCompliant);
            }
            Err(error) => return Err(error.into()),
        };

        if backup.policy_was_missing {
            match fs::remove_file(&self.policy_path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        } else {
            let contents = fs::read(&self.policy_path)?;
            let current = serde_json::from_slice::<Value>(&contents).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "cannot restore browser policy because {} is not valid JSON: {error}",
                        self.policy_path.display()
                    ),
                )
            })?;
            let restored = restore_managed_settings(current, &self.extension_id, &backup)?;
            self.write_policy(&restored)?;
        }

        fs::remove_file(backup_path)?;
        Ok(RepairStatus::Repaired)
    }

    fn ensure_merge_backup(&self, existing: Option<&Value>) -> Result<()> {
        let backup_path = self.backup_path.as_ref().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "merged Firefox policy has no backup path",
            )
        })?;
        if backup_path.exists() {
            return Ok(());
        }

        let backup = merged_policy_backup(existing, &self.extension_id)?;
        if let Some(parent) = backup_path.parent() {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
        write_json_atomically(backup_path, &backup, 0o600)
    }
}

const MANAGED_POLICY_KEYS: [&str; 6] = [
    "BlockAboutConfig",
    "BlockAboutProfiles",
    "BlockAboutSupport",
    "DisableDeveloperTools",
    "DisableSafeMode",
    "PrivateBrowsingModeAvailability",
];
const QUARANTINED_DOMAINS_PREFERENCE: &str = "extensions.quarantinedDomains.enabled";

fn policy_includes_managed_settings(policy: &Value, expected: &Value, extension_id: &str) -> bool {
    let Some(policies) = policy.get("policies").and_then(Value::as_object) else {
        return false;
    };
    let Some(expected_policies) = expected.get("policies").and_then(Value::as_object) else {
        return false;
    };

    MANAGED_POLICY_KEYS
        .iter()
        .all(|key| policies.get(*key) == expected_policies.get(*key))
        && policies
            .get("ExtensionSettings")
            .and_then(|settings| settings.get(extension_id))
            == expected_policies
                .get("ExtensionSettings")
                .and_then(|settings| settings.get(extension_id))
        && policies
            .get("Preferences")
            .and_then(|preferences| preferences.get(QUARANTINED_DOMAINS_PREFERENCE))
            == expected_policies
                .get("Preferences")
                .and_then(|preferences| preferences.get(QUARANTINED_DOMAINS_PREFERENCE))
}

fn merge_managed_settings(
    mut policy: Value,
    expected: &Value,
    extension_id: &str,
) -> Result<Value> {
    let policies = policy
        .get_mut("policies")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "browser policy is missing its policies object",
            )
        })?;
    let expected_policies = expected
        .get("policies")
        .and_then(Value::as_object)
        .expect("BlocKuntu policy must contain a policies object");

    for key in MANAGED_POLICY_KEYS {
        policies.insert(
            key.to_string(),
            expected_policies
                .get(key)
                .expect("BlocKuntu policy must contain every managed key")
                .clone(),
        );
    }

    let extension_settings = policies
        .entry("ExtensionSettings".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "browser policy ExtensionSettings is not an object",
            )
        })?;
    extension_settings.insert(
        extension_id.to_string(),
        expected_policies["ExtensionSettings"][extension_id].clone(),
    );

    let preferences = policies
        .entry("Preferences".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "browser policy Preferences is not an object",
            )
        })?;
    preferences.insert(
        QUARANTINED_DOMAINS_PREFERENCE.to_string(),
        expected_policies["Preferences"][QUARANTINED_DOMAINS_PREFERENCE].clone(),
    );

    Ok(policy)
}

fn merged_policy_backup(policy: Option<&Value>, extension_id: &str) -> Result<MergedPolicyBackup> {
    let Some(policy) = policy else {
        return Ok(MergedPolicyBackup {
            policy_was_missing: true,
            policy_values: Vec::new(),
            extension_setting: None,
            preference_setting: None,
        });
    };
    let policies = policy
        .get("policies")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "browser policy is missing its policies object",
            )
        })?;

    Ok(MergedPolicyBackup {
        policy_was_missing: false,
        policy_values: MANAGED_POLICY_KEYS
            .into_iter()
            .map(|key| (key.to_string(), policies.get(key).cloned()))
            .collect(),
        extension_setting: policies
            .get("ExtensionSettings")
            .and_then(|settings| settings.get(extension_id))
            .cloned(),
        preference_setting: policies
            .get("Preferences")
            .and_then(|preferences| preferences.get(QUARANTINED_DOMAINS_PREFERENCE))
            .cloned(),
    })
}

fn restore_managed_settings(
    mut policy: Value,
    extension_id: &str,
    backup: &MergedPolicyBackup,
) -> Result<Value> {
    let policies = policy
        .get_mut("policies")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "browser policy is missing its policies object",
            )
        })?;

    for (key, original) in &backup.policy_values {
        match original {
            Some(value) => {
                policies.insert(key.clone(), value.clone());
            }
            None => {
                policies.remove(key);
            }
        }
    }

    restore_nested_setting(
        policies,
        "ExtensionSettings",
        extension_id,
        backup.extension_setting.as_ref(),
    )?;
    restore_nested_setting(
        policies,
        "Preferences",
        QUARANTINED_DOMAINS_PREFERENCE,
        backup.preference_setting.as_ref(),
    )?;
    Ok(policy)
}

fn restore_nested_setting(
    policies: &mut serde_json::Map<String, Value>,
    parent_key: &str,
    child_key: &str,
    original: Option<&Value>,
) -> Result<()> {
    let Some(parent) = policies.get_mut(parent_key) else {
        return Ok(());
    };
    let parent = parent.as_object_mut().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("browser policy {parent_key} is not an object"),
        )
    })?;
    match original {
        Some(value) => {
            parent.insert(child_key.to_string(), value.clone());
        }
        None => {
            parent.remove(child_key);
        }
    }
    if parent.is_empty() {
        policies.remove(parent_key);
    }
    Ok(())
}

fn write_json_atomically(path: &Path, value: &impl Serialize, mode: u32) -> Result<()> {
    let temporary_path = temporary_path(path);
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary_path)?;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary_path, path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{FirefoxPolicyManager, RepairStatus};
    use serde_json::json;

    #[test]
    fn repairs_missing_policy_and_then_accepts_it() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let policy_path = temp.path().join("firefox/policies/policies.json");
        let manager = FirefoxPolicyManager::new(
            &policy_path,
            "blockuntu@example.local",
            "https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi",
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
        assert!(policy
            .contains("https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi"));

        let status = manager.status();
        assert!(status.compliant);
        assert!(status.private_browsing_enabled);
        assert!(status.private_browsing_available);
    }

    #[test]
    fn merges_and_restores_a_browser_owned_policy() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let policy_path = temp.path().join("librewolf/distribution/policies.json");
        let backup_path = temp.path().join("backups/librewolf.json");
        std::fs::create_dir_all(policy_path.parent().expect("policy parent should exist"))
            .expect("policy parent should be created");
        let original = json!({
            "policies": {
                "DisableDeveloperTools": false,
                "WebsiteFilter": { "Block": ["https://localhost/*"] },
                "ExtensionSettings": {
                    "other@example.local": { "installation_mode": "normal_installed" }
                },
                "Preferences": {
                    "browser.startup.homepage": { "Value": "https://example.invalid" }
                }
            }
        });
        std::fs::write(
            &policy_path,
            serde_json::to_vec_pretty(&original).expect("original policy should serialize"),
        )
        .expect("original policy should write");

        let manager = FirefoxPolicyManager::merging_existing_policy(
            &policy_path,
            "blockuntu@example.local",
            "https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi",
            &backup_path,
        );
        assert_eq!(
            manager.verify_and_repair().expect("merge should work"),
            RepairStatus::Repaired
        );
        assert!(manager.status().compliant);
        assert!(backup_path.exists());

        let merged: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&policy_path).expect("merged policy should be readable"),
        )
        .expect("merged policy should parse");
        assert_eq!(
            merged["policies"]["WebsiteFilter"],
            original["policies"]["WebsiteFilter"]
        );
        assert_eq!(merged["policies"]["DisableDeveloperTools"], true);
        assert!(merged["policies"]["ExtensionSettings"]
            .get("blockuntu@example.local")
            .is_some());

        assert_eq!(
            manager.remove_policy().expect("restore should work"),
            RepairStatus::Repaired
        );
        assert!(!backup_path.exists());
        let restored: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&policy_path).expect("restored policy should be readable"),
        )
        .expect("restored policy should parse");
        assert_eq!(restored, original);
    }
}
