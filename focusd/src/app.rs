use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use focus_core::{Database, FocusCore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::cli::Args;
use crate::error::{DaemonError, Result};
use crate::firefox_policy::{FirefoxPolicyManager, RepairStatus};
use crate::hosts::{HostsManager, HostsRepairStatus};
use crate::process_scan::{
    enforce_forbidden_processes, scan_procfs, ForbiddenProcess, LinuxSignalKiller,
};
use crate::rpc::{handle_payload, RpcContext};
use crate::socket::listener_from_systemd_or_path;

const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub struct DaemonApp {
    core: Arc<Mutex<FocusCore>>,
    rpc_context: RpcContext,
    firefox_policy: FirefoxPolicyManager,
    hosts: HostsManager,
    process_scan_interval: Duration,
    policy_repair_interval: Duration,
}

impl DaemonApp {
    pub fn load(args: &Args) -> Result<Self> {
        create_parent_dir(&args.database, 0o700)?;

        let config = focus_core::load_config(&args.config)?;
        let database = Database::open(&args.database)?;
        let core = Arc::new(Mutex::new(FocusCore::new(config, database)?));
        let rpc_context = RpcContext::new(core.clone(), args.config.clone());
        let firefox_policy = FirefoxPolicyManager::new(
            &args.firefox_policy,
            &args.extension_id,
            &args.extension_xpi,
        );
        let hosts = HostsManager::new(&args.hosts);

        Ok(Self {
            core,
            rpc_context,
            firefox_policy,
            hosts,
            process_scan_interval: Duration::from_secs(args.process_scan_interval_seconds),
            policy_repair_interval: Duration::from_secs(args.policy_repair_interval_seconds),
        })
    }

    pub fn check(&self) -> Result<()> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        focus_core::validate_config(core.config())?;
        core.database()
            .set_service_state("last_check", "ok", chrono::Utc::now())?;
        Ok(())
    }

    pub fn repair_firefox_policy(&self) -> Result<RepairStatus> {
        self.firefox_policy.verify_and_repair()
    }

    pub fn repair_hosts(&self) -> Result<HostsRepairStatus> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        self.hosts.verify_and_repair(core.config())
    }

    pub async fn serve(self, args: &Args) -> Result<()> {
        self.repair_firefox_policy()?;
        self.repair_hosts()?;
        self.spawn_repair_loop();
        self.spawn_process_scan_loop();

        let listener = listener_from_systemd_or_path(&args.socket, args.dev_bind_socket)?;
        self.accept_loop(listener).await
    }

    fn spawn_repair_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(app.policy_repair_interval);
            loop {
                interval.tick().await;
                if let Err(err) = app.repair_firefox_policy() {
                    eprintln!("Firefox policy repair failed: {err}");
                }
                if let Err(err) = app.repair_hosts() {
                    eprintln!("hosts repair failed: {err}");
                }
            }
        });
    }

    fn spawn_process_scan_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(app.process_scan_interval);
            loop {
                interval.tick().await;
                if let Err(err) = app.scan_processes_once() {
                    eprintln!("process scan failed: {err}");
                }
            }
        });
    }

    fn scan_processes_once(&self) -> Result<()> {
        let forbidden = self.forbidden_processes()?;
        if forbidden.is_empty() {
            return Ok(());
        }

        let processes = scan_procfs(Path::new("/proc"))?;
        let events = enforce_forbidden_processes(&processes, &forbidden, &LinuxSignalKiller)?;
        if events.is_empty() {
            return Ok(());
        }

        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let now = chrono::Utc::now();
        for event in events {
            core.database().record_event(
                "app_killed",
                event.command_name.as_deref(),
                Some(&format!(
                    "pid={};rule_id={};exe={}",
                    event.pid,
                    event.rule_id,
                    event
                        .executable_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string())
                )),
                now,
            )?;
        }
        Ok(())
    }

    fn forbidden_processes(&self) -> Result<Vec<ForbiddenProcess>> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let mut statement = core
            .database()
            .connection()
            .prepare(
                r#"
                SELECT COALESCE(rule_id, id), executable_path, command_name
                FROM apps
                WHERE enabled = 1
                  AND (executable_path IS NOT NULL OR command_name IS NOT NULL)
                "#,
            )
            .map_err(focus_core::Error::from)?;

        let rows = statement
            .query_map([], |row| {
                let rule_id: String = row.get(0)?;
                let executable_path: Option<String> = row.get(1)?;
                let command_name: Option<String> = row.get(2)?;
                Ok(ForbiddenProcess {
                    rule_id,
                    executable_path: executable_path.map(Into::into),
                    command_name,
                })
            })
            .map_err(focus_core::Error::from)?;

        let mut forbidden = Vec::new();
        for row in rows {
            forbidden.push(row.map_err(focus_core::Error::from)?);
        }
        Ok(forbidden)
    }

    async fn accept_loop(self, listener: UnixListener) -> Result<()> {
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let context = self.rpc_context.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_client(stream, context).await {
                            eprintln!("client error: {err}");
                        }
                    });
                }
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    return Ok(());
                }
            }
        }
    }
}

async fn handle_client(mut stream: UnixStream, context: RpcContext) -> Result<()> {
    let request = read_limited(&mut stream).await?;
    let response = handle_payload(&context, &request);
    stream.write_all(&response).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn read_limited(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Ok(payload);
        }
        if payload.len() + read > MAX_PAYLOAD_BYTES {
            return Err(DaemonError::InvalidRequest(format!(
                "request exceeds {MAX_PAYLOAD_BYTES} bytes"
            )));
        }
        payload.extend_from_slice(&buffer[..read]);
    }
}

fn create_parent_dir(path: &Path, mode: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}
