use std::io;
use std::time::Duration;

use clap::Parser;
use native_host::bridge::run_bridge;
use native_host::cli::Args;
use native_host::daemon_client::DaemonClient;

fn main() {
    if let Err(err) = run() {
        eprintln!("blockuntu-native: {err}");
        std::process::exit(1);
    }
}

fn run() -> native_host::error::Result<()> {
    let args = Args::parse();
    let daemon_client = DaemonClient::new(args.socket, Duration::from_millis(args.timeout_ms));

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();

    run_bridge(&mut input, &mut output, &daemon_client)
}
