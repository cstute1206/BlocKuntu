use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

pub const DEFAULT_SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
pub const DEFAULT_TIMEOUT_MS: u64 = 3000;
pub const DEFAULT_REVIVE_WAIT_MS: u64 = 1500;
pub const DEFAULT_REVIVE_RETRY_INTERVAL_MS: u64 = 100;
pub const DEFAULT_REVIVE_MIN_INTERVAL_MS: u64 = 5000;

#[derive(Debug, Clone, Parser)]
#[command(name = "blockuntu-native")]
#[command(about = "BlocKuntu browser Native Messaging bridge")]
pub struct Args {
    #[arg(long, default_value = DEFAULT_SOCKET_PATH)]
    pub socket: PathBuf,
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS)]
    pub timeout_ms: u64,
    #[arg(
        long,
        help = "Authenticated loopback endpoint used by confined Chromium-family Snaps"
    )]
    pub tcp_address: Option<SocketAddr>,
    #[arg(
        long,
        requires = "tcp_address",
        help = "Authentication token for --tcp-address"
    )]
    pub access_token: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Development-only command to start the daemon after socket connection failures"
    )]
    pub revive_command: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = DEFAULT_REVIVE_WAIT_MS,
        help = "Milliseconds to wait for the daemon after running --revive-command"
    )]
    pub revive_wait_ms: u64,
    #[arg(
        long,
        default_value_t = DEFAULT_REVIVE_RETRY_INTERVAL_MS,
        help = "Milliseconds between revived daemon connection retries"
    )]
    pub revive_retry_interval_ms: u64,
    #[arg(
        long,
        default_value_t = DEFAULT_REVIVE_MIN_INTERVAL_MS,
        help = "Minimum milliseconds between revival command launches"
    )]
    pub revive_min_interval_ms: u64,
    #[arg(
        hide = true,
        trailing_var_arg = true,
        help = "Browser-supplied Native Messaging origin arguments"
    )]
    pub browser_origin_args: Vec<String>,
}
