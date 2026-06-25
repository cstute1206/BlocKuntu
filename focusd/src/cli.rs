use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub const DEFAULT_CONFIG_PATH: &str = "/etc/blockuntu/config.toml";
pub const DEFAULT_DATABASE_PATH: &str = "/var/lib/blockuntu/blockuntu.sqlite3";
pub const DEFAULT_POLICY_RECOVERY_PATH: &str = "/etc/blockuntu/policy-recovery.toml";
pub const DEFAULT_SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
pub const DEFAULT_FIREFOX_POLICY_PATH: &str = "/etc/firefox/policies/policies.json";
pub const DEFAULT_EXTENSION_ID: &str = "blockuntu-poc@example.local";
pub const DEFAULT_EXTENSION_XPI_PATH: &str =
    "/home/christian/Desktop/HostFileModifier/browser-extension-firefox/BlocKuntu-Signed.xpi";
pub const DEFAULT_CHROME_POLICY_PATH: &str = "/etc/opt/chrome/policies/managed/blockuntu.json";
pub const DEFAULT_CHROME_UPDATE_MANIFEST_PATH: &str =
    "/usr/local/share/blockuntu/chrome-extension-updates.xml";
pub const DEFAULT_CHROME_EXTENSION_ID: &str = "odedgejjcdilkoibeljkeohekonmdfea";
pub const DEFAULT_CHROME_EXTENSION_VERSION: &str = "0.2.1";
pub const DEFAULT_CHROME_EXTENSION_CRX_URL: &str =
    "https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download";
pub const DEFAULT_HOSTS_PATH: &str = "/etc/hosts";

#[derive(Debug, Clone, Parser)]
#[command(name = "blockuntud")]
#[command(about = "BlocKuntu privileged daemon")]
pub struct Args {
    #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
    pub config: PathBuf,
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,
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
    #[arg(long, default_value = DEFAULT_EXTENSION_XPI_PATH)]
    pub extension_xpi: PathBuf,
    #[arg(long, default_value = DEFAULT_CHROME_POLICY_PATH)]
    pub chrome_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_CHROME_UPDATE_MANIFEST_PATH)]
    pub chrome_update_manifest: PathBuf,
    #[arg(long, default_value = DEFAULT_CHROME_EXTENSION_ID)]
    pub chrome_extension_id: String,
    #[arg(long, default_value = DEFAULT_CHROME_EXTENSION_VERSION)]
    pub chrome_extension_version: String,
    #[arg(long, default_value = DEFAULT_CHROME_EXTENSION_CRX_URL)]
    pub chrome_extension_crx_url: String,
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
