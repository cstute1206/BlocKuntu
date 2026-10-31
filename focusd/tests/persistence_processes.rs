use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use focus_core::{Config, Database, FocusCore};
use focusd::app::DaemonApp;
use focusd::cli::Args;
use focusd::policy_recovery::PolicyRecoveryManager;
use focusd::rpc::{handle_payload, RpcContext};
use serde_json::{json, Value};

fn policy(name: &str) -> Config {
    Config::from_toml_str(&format!(
        r#"
        [[rules]]
        id = "initial"
        name = "{name}"
        tier = "scheduled_block"
        patterns = [{{ kind = "domain", value = "initial.test" }}]
        [strict_mode]
        block_unsupported_browsers = false
        "#
    ))
    .unwrap()
}

fn maintenance_args(root: &Path) -> Args {
    Args::try_parse_from([
        "blockuntud".to_owned(),
        "--database".into(),
        root.join("policy.sqlite3").display().to_string(),
        "--config".into(),
        root.join("config.toml").display().to_string(),
        "--policy-recovery".into(),
        root.join("policy.toml").display().to_string(),
        "--event-log".into(),
        root.join("events.log").display().to_string(),
        "--hosts".into(),
        root.join("hosts").display().to_string(),
        "--no-policy-recovery-immutable".into(),
        "--no-hosts-immutable".into(),
        "--no-browser-policy-repair".into(),
        "repair-hosts".into(),
    ])
    .unwrap()
}

fn wait_for(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "process coordination timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

struct Worker {
    child: Child,
    log: PathBuf,
}

impl Worker {
    fn spawn(root: &Path, role: &str, index: usize) -> Self {
        let log = root.join(format!("{role}-{index}.log"));
        let output = fs::File::create(&log).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "persistence_process_worker", "--nocapture"])
            .env("BLOCKUNTU_PERSISTENCE_TEST_ROOT", root)
            .env("BLOCKUNTU_PERSISTENCE_TEST_ROLE", role)
            .env("BLOCKUNTU_PERSISTENCE_TEST_INDEX", index.to_string())
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output));
        if role == "protected-snapshot" {
            let mut paths = vec![root.join("bin")];
            paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
            command.env("PATH", std::env::join_paths(paths).unwrap());
        }
        let child = command.spawn().unwrap();
        Self { child, log }
    }

    fn finished(&mut self) -> bool {
        self.child.try_wait().unwrap().is_some()
    }

    fn assert_success(&mut self) {
        wait_for(|| self.finished());
        let status = self.child.wait().unwrap();
        assert!(
            status.success(),
            "{}",
            fs::read_to_string(&self.log).unwrap()
        );
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // A failing assertion must not leave writers running after TempDir cleanup.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn persistence_process_worker() {
    let Some(root) = std::env::var_os("BLOCKUNTU_PERSISTENCE_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let role = std::env::var("BLOCKUNTU_PERSISTENCE_TEST_ROLE").unwrap();
    let index = std::env::var("BLOCKUNTU_PERSISTENCE_TEST_INDEX").unwrap();
    fs::write(root.join(format!("ready-{index}")), "ready").unwrap();
    wait_for(|| root.join("start").exists());
    for iteration in 0..40 {
        if matches!(role.as_str(), "snapshot" | "protected-snapshot") {
            PolicyRecoveryManager::new(root.join("policy.toml"), role == "protected-snapshot")
                .write(&policy(&format!("writer-{index}-{iteration}")))
                .unwrap();
        } else {
            assert_eq!(role, "repair");
            DaemonApp::load(&maintenance_args(&root))
                .unwrap()
                .repair_hosts()
                .unwrap();
        }
    }
}

#[test]
fn competing_processes_write_complete_private_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let recovery = PolicyRecoveryManager::new(root.path().join("policy.toml"), false);
    recovery.write(&policy("initial")).unwrap();
    let mut workers: Vec<_> = (0..4)
        .map(|index| Worker::spawn(root.path(), "snapshot", index))
        .collect();
    wait_for(|| (0..4).all(|index| root.path().join(format!("ready-{index}")).exists()));
    fs::write(root.path().join("start"), "start").unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let snapshot = recovery.load().unwrap().unwrap();
        assert_eq!(snapshot.rules.len(), 1);
        assert!(
            snapshot.rules[0].name == "initial" || snapshot.rules[0].name.starts_with("writer-")
        );
        assert_eq!(
            fs::metadata(recovery.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
        if workers.iter_mut().all(Worker::finished) {
            break;
        }
        assert!(Instant::now() < deadline, "snapshot writers timed out");
        thread::sleep(Duration::from_millis(1));
    }
    for worker in &mut workers {
        worker.assert_success();
    }
}

#[test]
fn competing_processes_serialize_immutable_protection_changes() {
    let root = tempfile::tempdir().unwrap();
    let recovery = PolicyRecoveryManager::new(root.path().join("policy.toml"), false);
    recovery.write(&policy("initial")).unwrap();
    fs::create_dir(root.path().join("bin")).unwrap();
    let chattr = root.path().join("bin/chattr");
    // Model the protection window without requiring root/CAP_LINUX_IMMUTABLE.
    // A second -i before the first writer's +i fails, even with unique temp files.
    fs::write(
        &chattr,
        "#!/bin/sh\nset -eu\ncase \"$1\" in\n-i) mkdir \"$2.protection-open\"; sleep 0.005 ;;\n+i) rmdir \"$2.protection-open\" ;;\n*) exit 1 ;;\nesac\n",
    )
    .unwrap();
    fs::set_permissions(&chattr, fs::Permissions::from_mode(0o700)).unwrap();
    let mut workers: Vec<_> = (0..3)
        .map(|index| Worker::spawn(root.path(), "protected-snapshot", index))
        .collect();
    wait_for(|| (0..3).all(|index| root.path().join(format!("ready-{index}")).exists()));
    fs::write(root.path().join("start"), "start").unwrap();
    for worker in &mut workers {
        worker.assert_success();
    }
    assert!(!root.path().join("policy.toml.protection-open").exists());
    assert!(recovery.load().unwrap().is_some());
}

#[test]
fn maintenance_does_not_republish_a_database_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let recovery = PolicyRecoveryManager::new(root.path().join("policy.toml"), false);
    recovery.write(&policy("recovery version")).unwrap();
    let bytes = fs::read(recovery.path()).unwrap();
    let database = Database::open(root.path().join("policy.sqlite3")).unwrap();
    let current = policy("newer database version");
    database.replace_policy_config(&current).unwrap();
    fs::write(root.path().join("hosts"), "127.0.0.1 localhost\n").unwrap();

    // A repair helper must not publish a policy it loaded independently of the
    // daemon's in-flight snapshot/database transaction.
    DaemonApp::load(&maintenance_args(root.path()))
        .unwrap()
        .repair_hosts()
        .unwrap();
    assert_eq!(fs::read(recovery.path()).unwrap(), bytes);
    assert_eq!(database.load_policy_config().unwrap(), current);
}

#[test]
fn first_install_repair_leaves_a_policy_the_daemon_can_start_with() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("config.toml"),
        policy("first install").to_toml_string().unwrap(),
    )
    .unwrap();
    fs::write(root.path().join("hosts"), "127.0.0.1 localhost\n").unwrap();
    let mut args = maintenance_args(root.path());
    DaemonApp::load(&args).unwrap().repair_hosts().unwrap();
    let database = Database::open(root.path().join("policy.sqlite3")).unwrap();
    assert!(database.has_policy_config().unwrap());
    args.command = None; // The default serve command must not see an empty DB.
    DaemonApp::load(&args).unwrap();
}

#[test]
fn hosts_repair_processes_preserve_successful_policy_edits() {
    let root = tempfile::tempdir().unwrap();
    let initial = policy("initial");
    let recovery = PolicyRecoveryManager::new(root.path().join("policy.toml"), false);
    recovery.write(&initial).unwrap();
    let database = Database::open(root.path().join("policy.sqlite3")).unwrap();
    database.replace_policy_config(&initial).unwrap();
    fs::write(root.path().join("hosts"), "127.0.0.1 localhost\n").unwrap();
    let core = Arc::new(Mutex::new(FocusCore::new(initial, database).unwrap()));
    let context = RpcContext::new(core.clone()).with_policy_recovery(recovery.clone());
    let mut workers: Vec<_> = (0..2)
        .map(|index| Worker::spawn(root.path(), "repair", index))
        .collect();
    wait_for(|| (0..2).all(|index| root.path().join(format!("ready-{index}")).exists()));
    fs::write(root.path().join("start"), "start").unwrap();
    for index in 0..40 {
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&json!({
                "jsonrpc": "2.0", "id": index, "method": "upsert_site_list",
                "params": {"rule": {
                    "id": format!("edit-{index:03}"), "name": format!("Edit {index}"),
                    "tier": "scheduled_block", "enabled": true,
                    "patterns": [{"kind": "domain", "value": format!("edit-{index}.test")}]
                }}
            }))
            .unwrap(),
        ))
        .unwrap();
        assert!(response.get("error").is_none(), "{response}");
    }
    for worker in &mut workers {
        worker.assert_success();
    }
    let current = core.lock().unwrap();
    let mut persisted = current.database().load_policy_config().unwrap();
    let mut snapshot = recovery.load().unwrap().unwrap();
    persisted.rules.sort_by(|a, b| a.id.cmp(&b.id));
    snapshot.rules.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(persisted.rules.len(), 41);
    assert_eq!(persisted, snapshot);
    for rule in &persisted.rules {
        assert!(current.config().rules.contains(rule));
    }
}
