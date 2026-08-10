use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub const DEFAULT_CONFIG_PATH: &str = "/etc/blockuntu/config.toml";
pub const DEFAULT_DATABASE_PATH: &str = "/var/lib/blockuntu/blockuntu.sqlite3";
pub const DEFAULT_EVENT_LOG_PATH: &str = "/etc/blockuntu/blockuntu.log";
pub const DEFAULT_POLICY_RECOVERY_PATH: &str = "/etc/blockuntu/policy-recovery.toml";
pub const DEFAULT_SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
pub const DEFAULT_FIREFOX_POLICY_PATH: &str = "/etc/firefox/policies/policies.json";
pub const DEFAULT_EXTENSION_ID: &str = "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}";
pub const DEFAULT_FIREFOX_EXTENSION_INSTALL_URL: &str =
    "https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi";
// LibreWolf's Debian package installs its application directory under
// /usr/share/librewolf. Firefox-family policies are loaded from the
// application's distribution directory, not from the Native Messaging path
// under /usr/lib/librewolf.
pub const DEFAULT_LIBREWOLF_POLICY_PATH: &str = "/usr/share/librewolf/distribution/policies.json";
pub const DEFAULT_LIBREWOLF_POLICY_BACKUP_PATH: &str =
    "/var/lib/blockuntu/browser-policy-backups/librewolf.json";
pub const DEFAULT_LIBREWOLF_EXTENSION_ID: &str = DEFAULT_EXTENSION_ID;
pub const DEFAULT_LIBREWOLF_EXTENSION_INSTALL_URL: &str = DEFAULT_FIREFOX_EXTENSION_INSTALL_URL;
pub const DEFAULT_WATERFOX_POLICY_PATH: &str = "/usr/lib/waterfox/distribution/policies.json";
pub const DEFAULT_WATERFOX_POLICY_BACKUP_PATH: &str =
    "/var/lib/blockuntu/browser-policy-backups/waterfox.json";
pub const DEFAULT_WATERFOX_EXTENSION_ID: &str = DEFAULT_EXTENSION_ID;
pub const DEFAULT_WATERFOX_EXTENSION_INSTALL_URL: &str = DEFAULT_FIREFOX_EXTENSION_INSTALL_URL;
pub const DEFAULT_CHROME_POLICY_PATH: &str = "/etc/opt/chrome/policies/managed/blockuntu.json";
pub const DEFAULT_CHROME_EXTENSION_ID: &str = "opfljaancedgklbpnbpjfhdbbhbfpnoc";
pub const DEFAULT_CHROMIUM_POLICY_PATH: &str = "/etc/chromium/policies/managed/blockuntu.json";
pub const DEFAULT_CHROMIUM_EXTENSION_ID: &str = DEFAULT_CHROME_EXTENSION_ID;
pub const DEFAULT_BRAVE_POLICY_PATH: &str = "/etc/brave/policies/managed/blockuntu.json";
pub const DEFAULT_BRAVE_EXTENSION_ID: &str = DEFAULT_CHROME_EXTENSION_ID;
pub const DEFAULT_OPERA_POLICY_PATH: &str = "/etc/opt/opera/policies/managed/blockuntu.json";
pub const DEFAULT_OPERA_EXTENSION_ID: &str = DEFAULT_CHROME_EXTENSION_ID;
pub const DEFAULT_EDGE_POLICY_PATH: &str = "/etc/opt/edge/policies/managed/blockuntu.json";
pub const DEFAULT_EDGE_EXTENSION_ID: &str = DEFAULT_CHROME_EXTENSION_ID;
pub const DEFAULT_VIVALDI_POLICY_PATH: &str = "/etc/vivaldi/policies/managed/blockuntu.json";
pub const DEFAULT_VIVALDI_EXTENSION_ID: &str = DEFAULT_CHROME_EXTENSION_ID;
pub const DEFAULT_HOSTS_PATH: &str = "/etc/hosts";
pub const DEFAULT_SNAP_NATIVE_BRIDGE_ADDRESS: &str = "127.0.0.1:35173";
pub const DEFAULT_SNAP_NATIVE_BRIDGE_TOKEN_FILE: &str = "/etc/blockuntu/snap-native-bridge-token";

#[derive(Debug, Clone, Parser)]
#[command(name = "blockuntud")]
#[command(about = "BlocKuntu privileged daemon")]
pub struct Args {
    #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
    pub config: PathBuf,
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,
    #[arg(long, default_value = DEFAULT_EVENT_LOG_PATH)]
    pub event_log: PathBuf,
    #[arg(long, default_value = DEFAULT_POLICY_RECOVERY_PATH)]
    pub policy_recovery: PathBuf,
    #[arg(long, conflicts_with = "no_policy_recovery_immutable")]
    pub policy_recovery_immutable: bool,
    #[arg(long, conflicts_with = "policy_recovery_immutable")]
    pub no_policy_recovery_immutable: bool,
    #[arg(long, default_value = DEFAULT_SOCKET_PATH)]
    pub socket: PathBuf,
    #[arg(long, default_value = DEFAULT_FIREFOX_POLICY_PATH)]
    pub firefox_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_EXTENSION_ID)]
    pub extension_id: String,
    #[arg(long, default_value = DEFAULT_FIREFOX_EXTENSION_INSTALL_URL)]
    pub firefox_extension_install_url: String,
    #[arg(long, default_value = DEFAULT_LIBREWOLF_POLICY_PATH)]
    pub librewolf_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_LIBREWOLF_POLICY_BACKUP_PATH)]
    pub librewolf_policy_backup: PathBuf,
    #[arg(long, default_value = DEFAULT_LIBREWOLF_EXTENSION_ID)]
    pub librewolf_extension_id: String,
    #[arg(long, default_value = DEFAULT_LIBREWOLF_EXTENSION_INSTALL_URL)]
    pub librewolf_extension_install_url: String,
    #[arg(long, default_value = DEFAULT_WATERFOX_POLICY_PATH)]
    pub waterfox_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_WATERFOX_POLICY_BACKUP_PATH)]
    pub waterfox_policy_backup: PathBuf,
    #[arg(long, default_value = DEFAULT_WATERFOX_EXTENSION_ID)]
    pub waterfox_extension_id: String,
    #[arg(long, default_value = DEFAULT_WATERFOX_EXTENSION_INSTALL_URL)]
    pub waterfox_extension_install_url: String,
    #[arg(long, default_value = DEFAULT_CHROME_POLICY_PATH)]
    pub chrome_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_CHROME_EXTENSION_ID)]
    pub chrome_extension_id: String,
    #[arg(long, default_value = DEFAULT_CHROMIUM_POLICY_PATH)]
    pub chromium_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_CHROMIUM_EXTENSION_ID)]
    pub chromium_extension_id: String,
    #[arg(long, default_value = DEFAULT_BRAVE_POLICY_PATH)]
    pub brave_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_BRAVE_EXTENSION_ID)]
    pub brave_extension_id: String,
    #[arg(long, default_value = DEFAULT_OPERA_POLICY_PATH)]
    pub opera_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_OPERA_EXTENSION_ID)]
    pub opera_extension_id: String,
    #[arg(long, default_value = DEFAULT_EDGE_POLICY_PATH)]
    pub edge_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_EDGE_EXTENSION_ID)]
    pub edge_extension_id: String,
    #[arg(long, default_value = DEFAULT_VIVALDI_POLICY_PATH)]
    pub vivaldi_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_VIVALDI_EXTENSION_ID)]
    pub vivaldi_extension_id: String,
    #[arg(long, default_value = DEFAULT_HOSTS_PATH)]
    pub hosts: PathBuf,
    #[arg(long, conflicts_with = "no_hosts_immutable")]
    pub hosts_immutable: bool,
    #[arg(long, conflicts_with = "hosts_immutable")]
    pub no_hosts_immutable: bool,
    #[arg(long)]
    pub no_browser_policy_repair: bool,
    #[arg(long)]
    pub no_firefox_policy_repair: bool,
    #[arg(long)]
    pub no_chrome_policy_repair: bool,
    #[arg(long)]
    pub defer_browser_policy_repair_until_heartbeat: bool,
    #[arg(long)]
    pub defer_firefox_policy_repair_until_heartbeat: bool,
    #[arg(long)]
    pub defer_chrome_policy_repair_until_heartbeat: bool,
    #[arg(long)]
    pub snap_native_bridge: bool,
    #[arg(long, default_value = DEFAULT_SNAP_NATIVE_BRIDGE_ADDRESS)]
    pub snap_native_bridge_address: SocketAddr,
    #[arg(long, default_value = DEFAULT_SNAP_NATIVE_BRIDGE_TOKEN_FILE)]
    pub snap_native_bridge_token_file: PathBuf,
    #[arg(long)]
    pub dev_bind_socket: bool,
    #[arg(long, default_value_t = 15)]
    pub extension_heartbeat_timeout_seconds: u64,
    #[arg(long, default_value_t = 30)]
    pub policy_repair_interval_seconds: u64,
    #[arg(long, default_value_t = 10)]
    pub process_scan_interval_seconds: u64,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum Command {
    Serve,
    Check,
    RepairFirefoxPolicy,
    RepairChromePolicy,
    RepairHosts,
}

impl Args {
    pub fn command(&self) -> Command {
        self.command.clone().unwrap_or(Command::Serve)
    }

    pub fn manage_firefox_policy(&self) -> bool {
        !(self.no_browser_policy_repair || self.no_firefox_policy_repair)
    }

    pub fn manage_chrome_policy(&self) -> bool {
        !(self.no_browser_policy_repair || self.no_chrome_policy_repair)
    }

    pub fn defer_firefox_policy_repair_until_heartbeat(&self) -> bool {
        self.defer_browser_policy_repair_until_heartbeat
            || self.defer_firefox_policy_repair_until_heartbeat
    }

    pub fn defer_chrome_policy_repair_until_heartbeat(&self) -> bool {
        self.defer_browser_policy_repair_until_heartbeat
            || self.defer_chrome_policy_repair_until_heartbeat
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Args, DEFAULT_LIBREWOLF_POLICY_PATH};

    #[test]
    fn librewolf_default_policy_uses_its_application_directory() {
        let args = Args::try_parse_from(["blockuntud"]).expect("defaults should parse");

        assert_eq!(
            args.librewolf_policy,
            std::path::PathBuf::from(DEFAULT_LIBREWOLF_POLICY_PATH)
        );
        assert_eq!(
            DEFAULT_LIBREWOLF_POLICY_PATH,
            "/usr/share/librewolf/distribution/policies.json"
        );
    }
}
