use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub const DEFAULT_CONFIG_PATH: &str = "/etc/blockuntu/config.toml";
pub const DEFAULT_DATABASE_PATH: &str = "/var/lib/blockuntu/blockuntu.sqlite3";
pub const DEFAULT_SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
pub const DEFAULT_FIREFOX_POLICY_PATH: &str = "/etc/firefox/policies/policies.json";
pub const DEFAULT_EXTENSION_ID: &str = "blockuntu@example.local";
pub const DEFAULT_EXTENSION_XPI_PATH: &str = "/usr/local/share/blockuntu/BlocKuntu.xpi";
pub const DEFAULT_HOSTS_PATH: &str = "/etc/hosts";

#[derive(Debug, Clone, Parser)]
#[command(name = "blockuntud")]
#[command(about = "BlocKuntu privileged daemon")]
pub struct Args {
    #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
    pub config: PathBuf,
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,
    #[arg(long, default_value = DEFAULT_SOCKET_PATH)]
    pub socket: PathBuf,
    #[arg(long, default_value = DEFAULT_FIREFOX_POLICY_PATH)]
    pub firefox_policy: PathBuf,
    #[arg(long, default_value = DEFAULT_EXTENSION_ID)]
    pub extension_id: String,
    #[arg(long, default_value = DEFAULT_EXTENSION_XPI_PATH)]
    pub extension_xpi: PathBuf,
    #[arg(long, default_value = DEFAULT_HOSTS_PATH)]
    pub hosts: PathBuf,
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
    RepairHosts,
}

impl Args {
    pub fn command(&self) -> Command {
        self.command.clone().unwrap_or(Command::Serve)
    }
}
