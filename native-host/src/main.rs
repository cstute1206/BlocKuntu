use std::io;
use std::time::Duration;

use clap::Parser;
use native_host::bridge::run_bridge;
use native_host::cli::Args;
use native_host::daemon_client::{DaemonClient, DaemonRevivalConfig};

fn main() {
    if let Err(err) = run() {
        eprintln!("blockuntu-native: {err}");
        std::process::exit(1);
    }
}

fn run() -> native_host::error::Result<()> {
    let args = Args::parse();
    let mut daemon_client = match args.tcp_address {
        Some(address) => DaemonClient::new_snap_bridge(
            address,
            args.access_token
                .ok_or(native_host::error::NativeHostError::InvalidSnapBridgeToken)?,
            Duration::from_millis(args.timeout_ms),
        ),
        None => DaemonClient::new(args.socket, Duration::from_millis(args.timeout_ms)),
    };
    if let Some(command) = args.revive_command {
        daemon_client = daemon_client.with_revival(DaemonRevivalConfig::new(
            command,
            Duration::from_millis(args.revive_wait_ms),
            Duration::from_millis(args.revive_retry_interval_ms),
            Duration::from_millis(args.revive_min_interval_ms),
        ));
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();

    run_bridge(&mut input, &mut output, &daemon_client)
}
