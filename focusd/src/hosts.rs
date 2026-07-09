use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use focus_core::{Config, DetoxSession, RuleConfig, RulePatternKind, RuleTier};
use serde::Serialize;

use crate::error::Result;

const BEGIN_MARKER: &str = "# BEGIN BLOCKUNTU MANAGED";
const END_MARKER: &str = "# END BLOCKUNTU MANAGED";

#[derive(Debug, Clone)]
pub struct HostsManager {
    hosts_path: PathBuf,
    enforce_immutable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostsRepairStatus {
    AlreadyCompliant,
    Repaired,
    SkippedInactive,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostsFileStatus {
    pub path: String,
    pub expected_domain_count: usize,
    pub managed_block_present: bool,
    pub managed_block_compliant: bool,
    pub immutable_required: bool,
    pub immutable_state: HostsImmutableState,
    pub immutable_detail: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostsImmutableState {
    Enabled,
    Disabled,
    NotRequired,
    Unknown,
}

impl HostsManager {
    pub fn new(hosts_path: impl Into<PathBuf>) -> Self {
        Self::new_with_immutable(hosts_path, false)
    }

    pub fn new_with_immutable(hosts_path: impl Into<PathBuf>, enforce_immutable: bool) -> Self {
        Self {
            hosts_path: hosts_path.into(),
            enforce_immutable,
        }
    }

    pub fn hosts_path(&self) -> &Path {
        &self.hosts_path
    }

    pub fn verify_and_repair(&self, config: &Config) -> Result<HostsRepairStatus> {
        self.verify_and_repair_with_active_detox(config, &[])
    }

    pub fn verify_and_repair_with_active_detox(
        &self,
        config: &Config,
        active_detox_sessions: &[DetoxSession],
    ) -> Result<HostsRepairStatus> {
        let current = match fs::read_to_string(&self.hosts_path) {
            Ok(current) => current,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err.into()),
        };
        let expected_managed_block =
            render_managed_block_with_active_detox(config, active_detox_sessions);
        let expected = replace_managed_block(&current, &expected_managed_block);
        let mut repaired = false;

        if normalize_line_endings(&current) != normalize_line_endings(&expected) {
            self.clear_immutable_if_needed()?;
            write_hosts_atomically(&self.hosts_path, &expected)?;
            repaired = true;
        }

        if self.enforce_immutable && !self.immutable_enabled()? {
            self.set_immutable(true)?;
            repaired = true;
        }

        if repaired {
            Ok(HostsRepairStatus::Repaired)
        } else {
            Ok(HostsRepairStatus::AlreadyCompliant)
        }
    }

    pub fn remove_managed_block(&self) -> Result<HostsRepairStatus> {
        let current = match fs::read_to_string(&self.hosts_path) {
            Ok(current) => current,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err.into()),
        };
        let expected = remove_managed_block_from(&current);
        let content_changed = normalize_line_endings(&current) != normalize_line_endings(&expected);

        self.clear_immutable_if_needed()?;

        if !content_changed {
            return Ok(HostsRepairStatus::AlreadyCompliant);
        }

        write_hosts_atomically(&self.hosts_path, &expected)?;
        Ok(HostsRepairStatus::Repaired)
    }

    pub fn status(&self, config: &Config) -> HostsFileStatus {
        self.status_with_active_detox(config, &[])
    }

    pub fn status_with_active_detox(
        &self,
        config: &Config,
        active_detox_sessions: &[DetoxSession],
    ) -> HostsFileStatus {
        let current = match fs::read_to_string(&self.hosts_path) {
            Ok(current) => current,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return HostsFileStatus {
                    path: self.hosts_path.display().to_string(),
                    expected_domain_count: managed_domains_with_active_detox(
                        config,
                        active_detox_sessions,
                    )
                    .len(),
                    managed_block_present: false,
                    managed_block_compliant: false,
                    immutable_required: self.enforce_immutable,
                    immutable_state: HostsImmutableState::Unknown,
                    immutable_detail: format!("hosts file cannot be read: {err}"),
                    detail: format!("hosts file cannot be read: {err}"),
                }
            }
        };
        let expected_managed_block =
            render_managed_block_with_active_detox(config, active_detox_sessions);
        let expected = replace_managed_block(&current, &expected_managed_block);
        let managed_block_compliant =
            normalize_line_endings(&current) == normalize_line_endings(&expected);
        let (immutable_state, immutable_detail) = self.immutable_status();
        let detail = if managed_block_compliant {
            "managed hosts block matches Tier 1 and active Detox domain rules".to_string()
        } else {
            "managed hosts block is missing or differs from Tier 1 and active Detox domain rules"
                .to_string()
        };

        HostsFileStatus {
            path: self.hosts_path.display().to_string(),
            expected_domain_count: managed_domains_with_active_detox(config, active_detox_sessions)
                .len(),
            managed_block_present: managed_block_present(&current),
            managed_block_compliant,
            immutable_required: self.enforce_immutable,
            immutable_state,
            immutable_detail,
            detail,
        }
    }

    fn clear_immutable_if_needed(&self) -> Result<()> {
        if self.enforce_immutable && self.hosts_path.exists() {
            self.set_immutable(false)?;
        }
        Ok(())
    }

    fn set_immutable(&self, enabled: bool) -> Result<()> {
        let flag = if enabled { "+i" } else { "-i" };
        let output = Command::new("chattr")
            .arg(flag)
            .arg(&self.hosts_path)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error("chattr", &[flag], &self.hosts_path, output).into())
        }
    }

    fn immutable_enabled(&self) -> Result<bool> {
        let output = Command::new("lsattr")
            .arg("-d")
            .arg(&self.hosts_path)
            .output()?;
        if !output.status.success() {
            return Err(command_error("lsattr", &["-d"], &self.hosts_path, output).into());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let flags = stdout.split_whitespace().next().unwrap_or_default();
        Ok(flags.contains('i'))
    }

    fn immutable_status(&self) -> (HostsImmutableState, String) {
        if !self.enforce_immutable {
            return (
                HostsImmutableState::NotRequired,
                "immutable flag is not required for this hosts path".to_string(),
            );
        }

        match self.immutable_enabled() {
            Ok(true) => (
                HostsImmutableState::Enabled,
                "immutable flag is set".to_string(),
            ),
            Ok(false) => (
                HostsImmutableState::Disabled,
                "immutable flag is missing".to_string(),
            ),
            Err(err) => (HostsImmutableState::Unknown, err.to_string()),
        }
    }
}

pub fn render_managed_block(config: &Config) -> String {
    render_managed_block_with_active_detox(config, &[])
}

pub fn render_managed_block_with_active_detox(
    config: &Config,
    active_detox_sessions: &[DetoxSession],
) -> String {
    let domains = managed_domains_with_active_detox(config, active_detox_sessions);

    let mut block = String::from(BEGIN_MARKER);
    block.push('\n');
    block.push_str("# This block is managed by blockuntud. Manual edits are repaired.\n");
    for domain in domains {
        block.push_str("0.0.0.0 ");
        block.push_str(&domain);
        block.push('\n');
        block.push_str(":: ");
        block.push_str(&domain);
        block.push('\n');
    }
    block.push_str(END_MARKER);
    block.push('\n');
    block
}

fn managed_domains(config: &Config) -> BTreeSet<String> {
    let mut domains = BTreeSet::new();
    for rule in config
        .rules
        .iter()
        .filter(|rule| rule.tier == RuleTier::Hard)
    {
        add_rule_domains(&mut domains, rule);
    }

    domains
}

fn managed_domains_with_active_detox(
    config: &Config,
    active_detox_sessions: &[DetoxSession],
) -> BTreeSet<String> {
    let mut domains = managed_domains(config);

    for session in active_detox_sessions {
        for rule_id in &session.site_rule_ids {
            if let Some(rule) = config.rules.iter().find(|rule| rule.id == *rule_id) {
                add_rule_domains(&mut domains, rule);
            }
        }
    }

    domains
}

fn add_rule_domains(domains: &mut BTreeSet<String>, rule: &RuleConfig) {
    for pattern in rule
        .patterns
        .iter()
        .filter(|pattern| pattern.kind == RulePatternKind::Domain)
    {
        let domain = pattern
            .value
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if !domain.is_empty() {
            domains.insert(domain.clone());
            if pattern.match_subdomains && !domain.starts_with("www.") {
                domains.insert(format!("www.{domain}"));
            }
        }
    }
}

fn replace_managed_block(current: &str, managed_block: &str) -> String {
    let Some(begin) = current.find(BEGIN_MARKER) else {
        let mut output = current.trim_end_matches('\n').to_string();
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(managed_block);
        return output;
    };

    let Some(relative_end) = current[begin..].find(END_MARKER) else {
        let mut output = current[..begin].trim_end_matches('\n').to_string();
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(managed_block);
        return output;
    };

    let end = begin + relative_end + END_MARKER.len();
    let suffix_start = current[end..]
        .find('\n')
        .map(|offset| end + offset + 1)
        .unwrap_or(current.len());

    let mut output = String::new();
    output.push_str(current[..begin].trim_end_matches('\n'));
    if !output.is_empty() {
        output.push_str("\n\n");
    }
    output.push_str(managed_block);
    let suffix = current[suffix_start..].trim_start_matches('\n');
    if !suffix.is_empty() {
        output.push('\n');
        output.push_str(suffix);
    }
    output
}

fn remove_managed_block_from(current: &str) -> String {
    let Some(begin) = current.find(BEGIN_MARKER) else {
        return current.to_string();
    };

    let Some(relative_end) = current[begin..].find(END_MARKER) else {
        return current[..begin].trim_end_matches('\n').to_string();
    };

    let end = begin + relative_end + END_MARKER.len();
    let suffix_start = current[end..]
        .find('\n')
        .map(|offset| end + offset + 1)
        .unwrap_or(current.len());

    let mut output = current[..begin].trim_end_matches('\n').to_string();
    let suffix = current[suffix_start..].trim_start_matches('\n');
    if !suffix.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(suffix);
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

fn managed_block_present(current: &str) -> bool {
    current
        .find(BEGIN_MARKER)
        .is_some_and(|begin| current[begin..].contains(END_MARKER))
}

fn write_hosts_atomically(hosts_path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = hosts_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary_path = temporary_path(hosts_path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_path)?;
        file.set_permissions(fs::Permissions::from_mode(0o644))?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, hosts_path)?;
        fs::set_permissions(hosts_path, fs::Permissions::from_mode(0o644))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }

    result
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("hosts");
    path.with_file_name(format!(".{file_name}.blockuntu.{}.tmp", std::process::id()))
}

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n")
}

fn command_error(command: &str, args: &[&str], path: &Path, output: Output) -> std::io::Error {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = [stdout.trim(), stderr.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let joined_args = args.join(" ");
    let message = if detail.is_empty() {
        format!(
            "{command} {joined_args} {} exited with {}",
            path.display(),
            output.status
        )
    } else {
        format!(
            "{command} {joined_args} {} exited with {}: {detail}",
            path.display(),
            output.status
        )
    };

    std::io::Error::new(std::io::ErrorKind::Other, message)
}

#[cfg(test)]
mod tests {
    use super::{HostsImmutableState, HostsManager, HostsRepairStatus};
    use chrono::{TimeZone, Utc};
    use focus_core::{Config, DetoxSession};

    #[test]
    fn repairs_hosts_managed_block_and_preserves_user_content() {
        let config = Config::from_toml_str(
            r#"
            [[rules]]
            id = "hard"
            name = "Hard"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "instagram.com", match_subdomains = true }
            ]
            "#,
        )
        .expect("config should parse");
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let hosts_path = temp.path().join("hosts");
        std::fs::write(&hosts_path, "127.0.0.1 localhost\n").expect("hosts should write");
        let manager = HostsManager::new(&hosts_path);

        assert_eq!(
            manager
                .verify_and_repair(&config)
                .expect("repair should work"),
            HostsRepairStatus::Repaired
        );
        let contents = std::fs::read_to_string(&hosts_path).expect("hosts should exist");
        assert!(contents.contains("127.0.0.1 localhost"));
        assert!(contents.contains("0.0.0.0 instagram.com"));
        assert!(contents.contains("0.0.0.0 www.instagram.com"));
        assert_eq!(
            manager
                .verify_and_repair(&config)
                .expect("second check should pass"),
            HostsRepairStatus::AlreadyCompliant
        );
    }

    #[test]
    fn removes_managed_block_when_requested() {
        let config = Config::from_toml_str(
            r#"
            [[rules]]
            id = "hard"
            name = "Hard"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "instagram.com", match_subdomains = true }
            ]
            "#,
        )
        .expect("config should parse");
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let hosts_path = temp.path().join("hosts");
        std::fs::write(&hosts_path, "127.0.0.1 localhost\n").expect("hosts should write");
        let manager = HostsManager::new(&hosts_path);

        manager
            .verify_and_repair(&config)
            .expect("repair should work");
        assert_eq!(
            manager.remove_managed_block().expect("removal should work"),
            HostsRepairStatus::Repaired
        );

        let contents = std::fs::read_to_string(&hosts_path).expect("hosts should exist");
        assert!(contents.contains("127.0.0.1 localhost"));
        assert!(!contents.contains("BEGIN BLOCKUNTU MANAGED"));
        assert_eq!(
            manager
                .remove_managed_block()
                .expect("second removal should pass"),
            HostsRepairStatus::AlreadyCompliant
        );
    }

    #[test]
    fn reports_hosts_status_without_requiring_immutable_flag() {
        let config = Config::from_toml_str(
            r#"
            [[rules]]
            id = "hard"
            name = "Hard"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "instagram.com", match_subdomains = true }
            ]
            "#,
        )
        .expect("config should parse");
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let hosts_path = temp.path().join("hosts");
        let manager = HostsManager::new(&hosts_path);
        manager
            .verify_and_repair(&config)
            .expect("repair should work");

        let status = manager.status(&config);

        assert_eq!(status.expected_domain_count, 2);
        assert!(status.managed_block_present);
        assert!(status.managed_block_compliant);
        assert_eq!(status.immutable_state, HostsImmutableState::NotRequired);
    }

    #[test]
    fn includes_active_detox_site_rule_domains_and_removes_them_after_expiry() {
        let config = Config::from_toml_str(
            r#"
            [[rules]]
            id = "hard"
            name = "Hard"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "instagram.com", match_subdomains = true }
            ]

            [[rules]]
            id = "detox-controlled"
            name = "Detox controlled"
            tier = "controlled_access"
            patterns = [
              { kind = "domain", value = "youtube.com", match_subdomains = true },
              { kind = "url_contains", value = "shorts", match_subdomains = false }
            ]
            "#,
        )
        .expect("config should parse");
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let hosts_path = temp.path().join("hosts");
        let manager = HostsManager::new(&hosts_path);
        let detox_session = DetoxSession {
            id: "detox-1".to_string(),
            name: Some("Deep work".to_string()),
            starts_at: Utc
                .with_ymd_and_hms(2026, 5, 24, 20, 0, 0)
                .single()
                .expect("timestamp should be valid"),
            ends_at: Utc
                .with_ymd_and_hms(2026, 5, 24, 21, 0, 0)
                .single()
                .expect("timestamp should be valid"),
            cancelled_at: None,
            site_rule_ids: vec!["detox-controlled".to_string()],
            app_rule_ids: Vec::new(),
        };

        assert_eq!(
            manager
                .verify_and_repair_with_active_detox(&config, &[detox_session])
                .expect("repair should work"),
            HostsRepairStatus::Repaired
        );
        let contents = std::fs::read_to_string(&hosts_path).expect("hosts should exist");
        assert!(contents.contains("0.0.0.0 instagram.com"));
        assert!(contents.contains("0.0.0.0 youtube.com"));
        assert!(contents.contains("0.0.0.0 www.youtube.com"));
        assert!(!contents.contains("shorts"));
        let status = manager.status_with_active_detox(&config, &[]);
        assert_eq!(status.expected_domain_count, 2);

        assert_eq!(
            manager
                .verify_and_repair_with_active_detox(&config, &[])
                .expect("repair should work"),
            HostsRepairStatus::Repaired
        );
        let contents = std::fs::read_to_string(&hosts_path).expect("hosts should exist");
        assert!(contents.contains("0.0.0.0 instagram.com"));
        assert!(!contents.contains("0.0.0.0 youtube.com"));
        assert!(!contents.contains("0.0.0.0 www.youtube.com"));
    }
}
