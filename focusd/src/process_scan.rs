use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{DaemonError, Result};
use focus_core::ProcessIdentity;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub executable_path: Option<PathBuf>,
    pub executable_basename: Option<String>,
    pub command_name: Option<String>,
    pub desktop_id: Option<String>,
    pub window_titles: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedBrowser {
    Firefox,
    Chrome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessKillEvent {
    pub pid: u32,
    pub executable_path: Option<PathBuf>,
    pub executable_basename: Option<String>,
    pub command_name: Option<String>,
    pub desktop_id: Option<String>,
    pub window_titles: Vec<String>,
}

pub trait ProcessKiller {
    fn kill(&self, pid: u32) -> Result<()>;
}

pub trait WindowTitleProvider {
    fn titles_by_pid(&self) -> Result<HashMap<u32, Vec<String>>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WindowTitleSupport {
    pub available: bool,
    pub provider: Option<String>,
    pub session_type: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowTitleSnapshot {
    pub titles_by_pid: HashMap<u32, Vec<String>>,
    pub support: WindowTitleSupport,
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

#[derive(Debug, Clone, Copy)]
pub struct WmctrlWindowTitleProvider;

impl WindowTitleProvider for WmctrlWindowTitleProvider {
    fn titles_by_pid(&self) -> Result<HashMap<u32, Vec<String>>> {
        let output = match Command::new("wmctrl").args(["-lp"]).output() {
            Ok(output) => output,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(err) => return Err(err.into()),
        };

        if !output.status.success() {
            return Ok(HashMap::new());
        }

        Ok(parse_wmctrl_titles(&String::from_utf8_lossy(
            &output.stdout,
        )))
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
        let executable_basename = executable_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);
        let command_name = fs::read_to_string(process_dir.join("comm"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let desktop_id = desktop_id_from_process_dir(&process_dir);

        processes.push(ProcessInfo {
            pid,
            executable_path,
            executable_basename,
            command_name,
            desktop_id,
            window_titles: Vec::new(),
        });
    }

    Ok(processes)
}

pub fn attach_detected_window_titles(processes: &mut [ProcessInfo]) -> WindowTitleSnapshot {
    let snapshot = detect_window_titles(processes);
    merge_titles_into_processes(processes, &snapshot.titles_by_pid);
    snapshot
}

pub fn attach_window_titles<P: WindowTitleProvider>(
    processes: &mut [ProcessInfo],
    provider: &P,
) -> Result<()> {
    let titles = provider.titles_by_pid()?;
    for process in processes {
        if let Some(process_titles) = titles.get(&process.pid) {
            process.window_titles = process_titles.clone();
        }
    }
    Ok(())
}

pub fn kill_processes<K: ProcessKiller>(
    processes: &[ProcessInfo],
    blocked_pids: &[u32],
    killer: &K,
) -> Result<Vec<ProcessKillEvent>> {
    let mut events = Vec::new();
    for process in processes {
        if !blocked_pids.contains(&process.pid) {
            continue;
        }
        killer.kill(process.pid)?;
        events.push(ProcessKillEvent {
            pid: process.pid,
            executable_path: process.executable_path.clone(),
            executable_basename: process.executable_basename.clone(),
            command_name: process.command_name.clone(),
            desktop_id: process.desktop_id.clone(),
            window_titles: process.window_titles.clone(),
        });
    }
    Ok(events)
}

impl ProcessInfo {
    pub fn identity(&self) -> ProcessIdentity {
        ProcessIdentity {
            pid: Some(self.pid),
            executable_path: self
                .executable_path
                .as_ref()
                .map(|path| path.display().to_string()),
            executable_basename: self.executable_basename.clone(),
            command_name: self.command_name.clone(),
            desktop_id: self.desktop_id.clone(),
            window_titles: self.window_titles.clone(),
        }
    }
}

pub fn supported_browser_for_process(process: &ProcessIdentity) -> Option<SupportedBrowser> {
    let names = [
        process.executable_basename.as_deref(),
        process.command_name.as_deref(),
        process.desktop_id.as_deref(),
    ];

    if names.iter().flatten().any(|value| {
        matches_normalized(
            value,
            &[
                "firefox",
                "firefox-esr",
                "firefox-bin",
                "firefox.desktop",
                "org.mozilla.firefox.desktop",
            ],
        )
    }) {
        return Some(SupportedBrowser::Firefox);
    }

    if names.iter().flatten().any(|value| {
        matches_normalized(
            value,
            &[
                "chrome",
                "google-chrome",
                "google-chrome-stable",
                "google-chrome-beta",
                "google-chrome-unstable",
                "google-chrome.desktop",
                "com.google.chrome.desktop",
            ],
        )
    }) {
        return Some(SupportedBrowser::Chrome);
    }

    None
}

fn matches_normalized(value: &str, expected_values: &[&str]) -> bool {
    let value = value.trim().to_ascii_lowercase();
    expected_values.iter().any(|expected| value == *expected)
}

fn desktop_id_from_process_dir(process_dir: &Path) -> Option<String> {
    let environ = fs::read(process_dir.join("environ")).ok();
    let from_environ = environ.as_deref().and_then(desktop_id_from_environ);
    if from_environ.is_some() {
        return from_environ;
    }

    let cmdline = fs::read(process_dir.join("cmdline")).ok();
    cmdline.as_deref().and_then(desktop_id_from_cmdline)
}

fn desktop_id_from_environ(environ: &[u8]) -> Option<String> {
    for entry in environ.split(|byte| *byte == 0) {
        let Ok(entry) = std::str::from_utf8(entry) else {
            continue;
        };
        if let Some(value) = entry.strip_prefix("GIO_LAUNCHED_DESKTOP_FILE=") {
            return desktop_id_from_path_or_value(value);
        }
        if let Some(value) = entry.strip_prefix("FLATPAK_ID=") {
            return Some(format!("{value}.desktop"));
        }
    }
    None
}

fn desktop_id_from_cmdline(cmdline: &[u8]) -> Option<String> {
    let parts = cmdline
        .split(|byte| *byte == 0)
        .filter_map(|part| std::str::from_utf8(part).ok())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    for (index, part) in parts.iter().enumerate() {
        if let Some(value) = part.strip_prefix("--desktopfile=") {
            return desktop_id_from_path_or_value(value);
        }
        if *part == "--desktopfile" {
            if let Some(value) = parts.get(index + 1) {
                return desktop_id_from_path_or_value(value);
            }
        }
    }
    None
}

fn desktop_id_from_path_or_value(value: &str) -> Option<String> {
    let path = Path::new(value);
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .or_else(|| {
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        })
}

fn parse_wmctrl_titles(output: &str) -> HashMap<u32, Vec<String>> {
    let mut titles: HashMap<u32, Vec<String>> = HashMap::new();
    for line in output.lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 5 {
            continue;
        }
        let Ok(pid) = parts[2].parse::<u32>() else {
            continue;
        };
        if pid == 0 {
            continue;
        }
        let title = parts[4..].join(" ");
        if title.trim().is_empty() {
            continue;
        }
        titles.entry(pid).or_default().push(title);
    }
    titles
}

fn detect_window_titles(_processes: &[ProcessInfo]) -> WindowTitleSnapshot {
    let session_type = env::var("XDG_SESSION_TYPE")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let available = matches!(session_type.as_deref(), Some("x11"));
    let titles_by_pid = if available {
        WmctrlWindowTitleProvider
            .titles_by_pid()
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let detail = if available {
        "Open windows uses wmctrl and is only supported on X11.".to_string()
    } else {
        "Open windows is unavailable in this session. BlocKuntu only supports window-based filtering on X11 via wmctrl.".to_string()
    };

    WindowTitleSnapshot {
        titles_by_pid,
        support: WindowTitleSupport {
            available,
            provider: available.then_some("wmctrl".to_string()),
            session_type,
            detail,
        },
    }
}

fn merge_titles_into_processes(
    processes: &mut [ProcessInfo],
    titles_by_pid: &HashMap<u32, Vec<String>>,
) {
    for process in processes {
        if let Some(process_titles) = titles_by_pid.get(&process.pid) {
            process.window_titles = process_titles.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::os::unix::fs::symlink;

    use super::{
        attach_window_titles, kill_processes, parse_wmctrl_titles, scan_procfs, ProcessInfo,
        ProcessKiller, WindowTitleProvider,
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

    struct StaticWindowTitles {
        titles: HashMap<u32, Vec<String>>,
    }

    impl WindowTitleProvider for StaticWindowTitles {
        fn titles_by_pid(&self) -> Result<HashMap<u32, Vec<String>>> {
            Ok(self.titles.clone())
        }
    }

    #[test]
    fn scans_procfs_and_kills_selected_processes() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let bin_dir = temp.path().join("bin");
        std::fs::create_dir(&bin_dir).expect("bin dir should exist");
        let app_path = bin_dir.join("blocked-app");
        std::fs::write(&app_path, "#!/bin/sh\n").expect("fake executable should write");

        let proc_dir = temp.path().join("proc/1234");
        std::fs::create_dir_all(&proc_dir).expect("proc dir should exist");
        symlink(&app_path, proc_dir.join("exe")).expect("exe symlink should create");
        std::fs::write(proc_dir.join("comm"), "blocked-app\n").expect("comm should write");
        std::fs::write(
            proc_dir.join("environ"),
            b"GIO_LAUNCHED_DESKTOP_FILE=/usr/share/applications/org.example.Blocked.desktop\0",
        )
        .expect("environ should write");

        let mut processes = scan_procfs(&temp.path().join("proc")).expect("proc scan should pass");
        attach_window_titles(
            &mut processes,
            &StaticWindowTitles {
                titles: HashMap::from([(1234, vec!["Blocked App".to_string()])]),
            },
        )
        .expect("window titles should attach");
        assert_eq!(
            processes,
            vec![ProcessInfo {
                pid: 1234,
                executable_path: Some(app_path.clone()),
                executable_basename: Some("blocked-app".to_string()),
                command_name: Some("blocked-app".to_string()),
                desktop_id: Some("org.example.Blocked.desktop".to_string()),
                window_titles: vec!["Blocked App".to_string()],
            }]
        );

        let killer = RecordingKiller::default();
        let events = kill_processes(&processes, &[1234], &killer).expect("enforcement should pass");

        assert_eq!(killer.killed.borrow().as_slice(), &[1234]);
        assert_eq!(events[0].executable_path, Some(app_path));
    }

    #[test]
    fn parses_wmctrl_titles_by_pid() {
        let titles = parse_wmctrl_titles(
            "0x04000007  0 1234 host KMines - 4 mines\n0x04000008  0 0 host Panel\n",
        );

        assert_eq!(
            titles.get(&1234),
            Some(&vec!["KMines - 4 mines".to_string()])
        );
    }
}
