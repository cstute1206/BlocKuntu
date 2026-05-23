use clap::Parser;
use focusd::app::DaemonApp;
use focusd::cli::{Args, Command};
use focusd::error::Result;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("blockuntud: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();
    let app = DaemonApp::load(&args)?;

    match args.command() {
        Command::Serve => app.serve(&args).await,
        Command::Check => app.check(),
        Command::RepairFirefoxPolicy => {
            let status = app.repair_firefox_policy()?;
            println!("Firefox policy: {status:?}");
            Ok(())
        }
        Command::RepairHosts => {
            let status = app.repair_hosts()?;
            println!("hosts: {status:?}");
            Ok(())
        }
    }
}
