use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{DaemonError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForbiddenProcess {
    pub rule_id: String,
    pub executable_path: Option<PathBuf>,
    pub command_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub executable_path: Option<PathBuf>,
    pub command_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessKillEvent {
    pub pid: u32,
    pub rule_id: String,
    pub executable_path: Option<PathBuf>,
    pub command_name: Option<String>,
}

pub trait ProcessKiller {
    fn kill(&self, pid: u32) -> Result<()>;
}

#[derive(Debug, Clone, Copy)]
pub struct LinuxSignalKiller;

impl ProcessKiller for LinuxSignalKiller {
    fn kill(&self, pid: u32) -> Result<()> {
        let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if result == 0 {
            return Ok(());
        }

        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        Err(DaemonError::KillFailed { pid, errno })
    }
}

pub fn scan_procfs(proc_root: &Path) -> Result<Vec<ProcessInfo>> {
    let mut processes = Vec::new();
    for entry in fs::read_dir(proc_root)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(pid) = file_name.to_str().and_then(|name| name.parse::<u32>().ok()) else {
            continue;
        };

        let process_dir = entry.path();
        let executable_path = fs::read_link(process_dir.join("exe")).ok();
        let command_name = fs::read_to_string(process_dir.join("comm"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        processes.push(ProcessInfo {
            pid,
            executable_path,
            command_name,
        });
    }

    Ok(processes)
}

pub fn enforce_forbidden_processes<K: ProcessKiller>(
    processes: &[ProcessInfo],
    forbidden: &[ForbiddenProcess],
    killer: &K,
) -> Result<Vec<ProcessKillEvent>> {
    let mut events = Vec::new();
    for process in processes {
        let Some(rule) = forbidden.iter().find(|rule| process_matches(process, rule)) else {
            continue;
        };
        killer.kill(process.pid)?;
        events.push(ProcessKillEvent {
            pid: process.pid,
            rule_id: rule.rule_id.clone(),
            executable_path: process.executable_path.clone(),
            command_name: process.command_name.clone(),
        });
    }
    Ok(events)
}

fn process_matches(process: &ProcessInfo, rule: &ForbiddenProcess) -> bool {
    let path_matches = match (&process.executable_path, &rule.executable_path) {
        (Some(process_path), Some(rule_path)) => process_path == rule_path,
        _ => false,
    };
    let command_matches = match (&process.command_name, &rule.command_name) {
        (Some(process_name), Some(rule_name)) => process_name == rule_name,
        _ => false,
    };

    path_matches || command_matches
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::os::unix::fs::symlink;

    use super::{
        enforce_forbidden_processes, scan_procfs, ForbiddenProcess, ProcessInfo, ProcessKiller,
    };
    use crate::error::Result;

    #[derive(Default)]
    struct RecordingKiller {
        killed: RefCell<Vec<u32>>,
    }

    impl ProcessKiller for RecordingKiller {
        fn kill(&self, pid: u32) -> Result<()> {
            self.killed.borrow_mut().push(pid);
            Ok(())
        }
    }

    #[test]
    fn scans_procfs_and_kills_forbidden_command_or_path() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let bin_dir = temp.path().join("bin");
        std::fs::create_dir(&bin_dir).expect("bin dir should exist");
        let app_path = bin_dir.join("blocked-app");
        std::fs::write(&app_path, "#!/bin/sh\n").expect("fake executable should write");

        let proc_dir = temp.path().join("proc/1234");
        std::fs::create_dir_all(&proc_dir).expect("proc dir should exist");
        symlink(&app_path, proc_dir.join("exe")).expect("exe symlink should create");
        std::fs::write(proc_dir.join("comm"), "blocked-app\n").expect("comm should write");

        let processes = scan_procfs(&temp.path().join("proc")).expect("proc scan should pass");
        assert_eq!(
            processes,
            vec![ProcessInfo {
                pid: 1234,
                executable_path: Some(app_path.clone()),
                command_name: Some("blocked-app".to_string())
            }]
        );

        let killer = RecordingKiller::default();
        let events = enforce_forbidden_processes(
            &processes,
            &[ForbiddenProcess {
                rule_id: "app-rule".to_string(),
                executable_path: Some(app_path),
                command_name: None,
            }],
            &killer,
        )
        .expect("enforcement should pass");

        assert_eq!(killer.killed.borrow().as_slice(), &[1234]);
        assert_eq!(events[0].rule_id, "app-rule");
    }
}
