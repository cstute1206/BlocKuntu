use std::path::PathBuf;

use clap::Parser;

pub const DEFAULT_SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
pub const DEFAULT_TIMEOUT_MS: u64 = 3000;

#[derive(Debug, Clone, Parser)]
#[command(name = "blockuntu-native")]
#[command(about = "BlocKuntu Firefox Native Messaging bridge")]
pub struct Args {
    #[arg(long, default_value = DEFAULT_SOCKET_PATH)]
    pub socket: PathBuf,
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS)]
    pub timeout_ms: u64,
}
