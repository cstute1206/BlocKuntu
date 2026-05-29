use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};
use url::Url;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct ChromePolicyManager {
    policy_path: PathBuf,
    update_manifest_path: PathBuf,
    extension_id: String,
    extension_version: String,
    extension_crx_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromePolicyRepairStatus {
    AlreadyCompliant,
    Repaired,
    SkippedStopped,
    SkippedDisabled,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChromePolicyStatus {
    pub path: String,
    pub update_manifest_path: String,
    pub extension_id: String,
    pub extension_version: String,
    pub extension_crx_url: String,
    pub update_url: String,
    pub policy_exists: bool,
    pub update_manifest_exists: bool,
    pub valid_json: bool,
    pub compliant: bool,
    pub update_manifest_compliant: bool,
    pub force_install_configured: bool,
    pub override_update_url: bool,
    pub detail: String,
}

impl ChromePolicyManager {
    pub fn new(
        policy_path: impl Into<PathBuf>,
        update_manifest_path: impl Into<PathBuf>,
        extension_id: impl Into<String>,
        extension_version: impl Into<String>,
        extension_crx_url: impl Into<String>,
    ) -> Self {
        Self {
            policy_path: policy_path.into(),
            update_manifest_path: update_manifest_path.into(),
            extension_id: extension_id.into(),
            extension_version: extension_version.into(),
            extension_crx_url: normalize_url(extension_crx_url.into()),
        }
    }

    pub fn expected_policy(&self) -> Value {
        let update_url = self.update_url();
        let force_install_entry = self.force_install_entry(&update_url);

        json!({
            "DeveloperToolsDisabled": true,
            "ExtensionInstallForcelist": [force_install_entry],
            "ExtensionSettings": {
                self.extension_id.clone(): {
                    "installation_mode": "force_installed",
                    "update_url": update_url,
                    "override_update_url": true
                }
            }
        })
    }

    pub fn expected_update_manifest(&self) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <gupdate xmlns=\"http://www.google.com/update2/response\" protocol=\"2.0\">\n\
             \u{20} <app appid=\"{}\">\n\
             \u{20}\u{20} <updatecheck codebase=\"{}\" version=\"{}\" />\n\
             \u{20} </app>\n\
             </gupdate>\n",
            xml_escape(&self.extension_id),
            xml_escape(&self.extension_crx_url),
            xml_escape(&self.extension_version)
        )
    }

    pub fn verify_and_repair(&self) -> Result<ChromePolicyRepairStatus> {
        let expected_policy = self.expected_policy();
        let expected_manifest = self.expected_update_manifest();
        let policy_compliant = file_json_equals(&self.policy_path, &expected_policy)?;
        let manifest_compliant = file_text_equals(&self.update_manifest_path, &expected_manifest)?;

        if policy_compliant && manifest_compliant {
            return Ok(ChromePolicyRepairStatus::AlreadyCompliant);
        }

        self.write_update_manifest(&expected_manifest)?;
        self.write_policy(&expected_policy)?;
        Ok(ChromePolicyRepairStatus::Repaired)
    }

    pub fn remove_policy(&self) -> Result<ChromePolicyRepairStatus> {
        let mut removed = false;
        for path in [&self.policy_path, &self.update_manifest_path] {
            match fs::remove_file(path) {
                Ok(()) => removed = true,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }

        Ok(if removed {
            ChromePolicyRepairStatus::Repaired
        } else {
            ChromePolicyRepairStatus::AlreadyCompliant
        })
    }

    pub fn status(&self) -> ChromePolicyStatus {
        let expected_policy = self.expected_policy();
        let expected_manifest = self.expected_update_manifest();
        let update_url = self.update_url();
        let policy_path = self.policy_path.display().to_string();
        let update_manifest_path = self.update_manifest_path.display().to_string();
        let update_manifest_exists = self.update_manifest_path.exists();
        let update_manifest_compliant = fs::read_to_string(&self.update_manifest_path)
            .map(|contents| contents == expected_manifest)
            .unwrap_or(false);

        let contents = match fs::read(&self.policy_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return ChromePolicyStatus {
                    path: policy_path,
                    update_manifest_path,
                    extension_id: self.extension_id.clone(),
                    extension_version: self.extension_version.clone(),
                    extension_crx_url: self.extension_crx_url.clone(),
                    update_url,
                    policy_exists: false,
                    update_manifest_exists,
                    valid_json: false,
                    compliant: false,
                    update_manifest_compliant,
                    force_install_configured: false,
                    override_update_url: false,
                    detail: "Chrome policy file is missing".to_string(),
                };
            }
            Err(err) => {
                return ChromePolicyStatus {
                    path: policy_path,
                    update_manifest_path,
                    extension_id: self.extension_id.clone(),
                    extension_version: self.extension_version.clone(),
                    extension_crx_url: self.extension_crx_url.clone(),
                    update_url,
                    policy_exists: true,
                    update_manifest_exists,
                    valid_json: false,
                    compliant: false,
                    update_manifest_compliant,
                    force_install_configured: false,
                    override_update_url: false,
                    detail: format!("Chrome policy file cannot be read: {err}"),
                };
            }
        };

        let parsed = match serde_json::from_slice::<Value>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                return ChromePolicyStatus {
                    path: policy_path,
                    update_manifest_path,
                    extension_id: self.extension_id.clone(),
                    extension_version: self.extension_version.clone(),
                    extension_crx_url: self.extension_crx_url.clone(),
                    update_url,
                    policy_exists: true,
                    update_manifest_exists,
                    valid_json: false,
                    compliant: false,
                    update_manifest_compliant,
                    force_install_configured: false,
                    override_update_url: false,
                    detail: format!("Chrome policy file is not valid JSON: {err}"),
                };
            }
        };

        let force_install_configured = self.force_install_configured(&parsed, &update_url);
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
            == Some(update_url.as_str());
        let override_update_url = extension_settings
            .and_then(|settings| settings.get("override_update_url"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let compliant = parsed == expected_policy && update_manifest_compliant;
        let detail = if compliant {
            "Chrome policy and local update manifest match expected force-install settings"
                .to_string()
        } else if !force_install_configured {
            "Chrome force-install entry is missing or points at a different update manifest"
                .to_string()
        } else if !settings_force_installed || !settings_update_url || !override_update_url {
            "Chrome ExtensionSettings are missing force_installed/update_url override settings"
                .to_string()
        } else if !update_manifest_compliant {
            "Chrome local update manifest is missing or differs from expected CRX URL/version"
                .to_string()
        } else {
            "Chrome policy differs from expected hardened settings".to_string()
        };

        ChromePolicyStatus {
            path: policy_path,
            update_manifest_path,
            extension_id: self.extension_id.clone(),
            extension_version: self.extension_version.clone(),
            extension_crx_url: self.extension_crx_url.clone(),
            update_url,
            policy_exists: true,
            update_manifest_exists,
            valid_json: true,
            compliant,
            update_manifest_compliant,
            force_install_configured,
            override_update_url,
            detail,
        }
    }

    fn update_url(&self) -> String {
        file_url(&self.update_manifest_path)
    }

    fn force_install_entry(&self, update_url: &str) -> String {
        format!("{};{update_url}", self.extension_id)
    }

    fn force_install_configured(&self, policy: &Value, update_url: &str) -> bool {
        let expected = self.force_install_entry(update_url);
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

    fn write_update_manifest(&self, manifest: &str) -> Result<()> {
        let parent = self.update_manifest_path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Chrome update manifest path has no parent: {}",
                    self.update_manifest_path.display()
                ),
            )
        })?;
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?;

        let temporary_path = temporary_path(&self.update_manifest_path);
        let result = write_text_atomically(&self.update_manifest_path, &temporary_path, manifest);
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

fn file_text_equals(path: &Path, expected: &str) -> Result<bool> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents == expected),
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

fn write_text_atomically(path: &Path, temporary_path: &Path, contents: &str) -> Result<()> {
    let mut file = open_temporary_file(temporary_path)?;
    file.write_all(contents.as_bytes())?;
    finish_atomic_write(file, temporary_path, path)
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

fn file_url(path: &Path) -> String {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.display()))
}

fn normalize_url(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("file://")
    {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{ChromePolicyManager, ChromePolicyRepairStatus};

    #[test]
    fn repairs_missing_policy_and_update_manifest() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let policy_path = temp.path().join("chrome/policies/managed/blockuntu.json");
        let update_manifest_path = temp.path().join("chrome/updates.xml");
        let manager = ChromePolicyManager::new(
            &policy_path,
            &update_manifest_path,
            "odedgejjcdilkoibeljkeohekonmdfea",
            "0.2.1",
            "nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download",
        );

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
        assert!(parsed["ExtensionInstallForcelist"][0]
            .as_str()
            .expect("force list entry should be a string")
            .starts_with("odedgejjcdilkoibeljkeohekonmdfea;file://"));
        assert_eq!(
            parsed["ExtensionSettings"]["odedgejjcdilkoibeljkeohekonmdfea"]["installation_mode"],
            "force_installed"
        );
        assert_eq!(
            parsed["ExtensionSettings"]["odedgejjcdilkoibeljkeohekonmdfea"]["override_update_url"],
            true
        );

        let manifest =
            std::fs::read_to_string(&update_manifest_path).expect("manifest should exist");
        assert!(manifest.contains("odedgejjcdilkoibeljkeohekonmdfea"));
        assert!(
            manifest.contains("https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download")
        );

        let status = manager.status();
        assert!(status.compliant);
        assert!(status.force_install_configured);
        assert!(status.override_update_url);
    }
}
