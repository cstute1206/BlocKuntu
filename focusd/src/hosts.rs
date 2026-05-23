use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use focus_core::{Config, RulePatternKind, RuleTier};

use crate::error::Result;

const BEGIN_MARKER: &str = "# BEGIN BLOCKUNTU MANAGED";
const END_MARKER: &str = "# END BLOCKUNTU MANAGED";

#[derive(Debug, Clone)]
pub struct HostsManager {
    hosts_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostsRepairStatus {
    AlreadyCompliant,
    Repaired,
}

impl HostsManager {
    pub fn new(hosts_path: impl Into<PathBuf>) -> Self {
        Self {
            hosts_path: hosts_path.into(),
        }
    }

    pub fn hosts_path(&self) -> &Path {
        &self.hosts_path
    }

    pub fn verify_and_repair(&self, config: &Config) -> Result<HostsRepairStatus> {
        let current = match fs::read_to_string(&self.hosts_path) {
            Ok(current) => current,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err.into()),
        };
        let expected_managed_block = render_managed_block(config);
        let expected = replace_managed_block(&current, &expected_managed_block);

        if normalize_line_endings(&current) == normalize_line_endings(&expected) {
            return Ok(HostsRepairStatus::AlreadyCompliant);
        }

        write_hosts_atomically(&self.hosts_path, &expected)?;
        Ok(HostsRepairStatus::Repaired)
    }
}

pub fn render_managed_block(config: &Config) -> String {
    let mut domains = BTreeSet::new();
    for rule in config
        .rules
        .iter()
        .filter(|rule| rule.enabled && rule.tier == RuleTier::Hard)
    {
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

#[cfg(test)]
mod tests {
    use super::{HostsManager, HostsRepairStatus};
    use focus_core::Config;

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
}
