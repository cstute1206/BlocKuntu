use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn entry_path() -> Result<PathBuf, String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or("Cannot locate your desktop login configuration directory")?;
    Ok(config.join("autostart/blockuntu-gui.desktop"))
}

fn desktop_entry(executable: &Path) -> Result<String, String> {
    let path = executable
        .to_str()
        .ok_or("Executable path is not valid UTF-8")?;
    if path.contains(['\n', '\r', '\0']) {
        return Err("Unsupported executable path".into());
    }
    // Desktop Entry string escaping is applied after Exec argument escaping.
    let mut quoted = String::new();
    for c in path.chars() {
        match c {
            '%' => quoted.push_str("%%"),
            '\\' => quoted.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            _ => quoted.push(c),
        }
    }
    Ok(format!("[Desktop Entry]\nType=Application\nName=BlocKuntu\nComment=Start BlocKuntu in the system tray\nExec=\"{quoted}\" --tray\nIcon=blockuntu-gui\nTerminal=false\n"))
}

fn write_setting(path: &Path, executable: &Path, enabled: bool) -> Result<(), String> {
    if enabled {
        let content = desktop_entry(executable)?;
        fs::create_dir_all(path.parent().ok_or("Invalid startup path")?)
            .map_err(|e| e.to_string())?;
        let pending = path.with_extension("desktop.tmp");
        fs::write(&pending, content).map_err(|e| e.to_string())?;
        fs::rename(&pending, path).map_err(|e| e.to_string())?;
    } else {
        match fs::remove_file(path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

#[tauri::command]
pub fn login_startup_enabled() -> Result<bool, String> {
    let path = entry_path()?;
    match fs::read_to_string(path) {
        Ok(content) => Ok(content
            .lines()
            .any(|line| line.starts_with("Exec=") && line.ends_with(" --tray"))
            && !content.lines().any(|line| line == "Hidden=true")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn set_login_startup(enabled: bool) -> Result<(), String> {
    // For AppImage, current_exe points into a temporary mount.
    let executable = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(Ok)
        .unwrap_or_else(std::env::current_exe)
        .map_err(|e| e.to_string())?;
    write_setting(&entry_path()?, &executable, enabled)
}

pub fn tray_requested(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().skip(1).any(|arg| arg == "--tray")
}

/// AppIndicator creation can succeed even when no desktop displays the icon.
/// If a watcher cannot be confirmed, keep the main window accessible instead.
pub fn tray_host_available() -> bool {
    Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--timeout",
            "2",
            "--dest",
            "org.kde.StatusNotifierWatcher",
            "--object-path",
            "/StatusNotifierWatcher",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "org.kde.StatusNotifierWatcher",
            "IsStatusNotifierHostRegistered",
        ])
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "(<true>,)"
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_setting_persists_and_removes_only_its_entry() {
        let root =
            std::env::temp_dir().join(format!("blockuntu-login-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let entry = root.join("blockuntu.desktop");
        let other = root.join("other.desktop");
        fs::write(&other, "other").unwrap();
        write_setting(&entry, Path::new("/opt/Bloc Kuntu/gui"), true).unwrap();
        assert!(fs::read_to_string(&entry)
            .unwrap()
            .contains("Exec=\"/opt/Bloc Kuntu/gui\" --tray"));
        write_setting(&entry, Path::new("/opt/gui"), false).unwrap();
        write_setting(&entry, Path::new("/opt/gui"), false).unwrap();
        assert!(!entry.exists());
        assert_eq!(fs::read_to_string(other).unwrap(), "other");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn exec_escaping_and_launch_modes() {
        let entry = desktop_entry(Path::new("/opt/a%/b\\c\"$`/gui")).unwrap();
        assert!(entry.contains(r#"Exec="/opt/a%%/b\\\\c\\"\\$\\`/gui" --tray"#));
        assert!(desktop_entry(Path::new("/opt/bad\nExec=other")).is_err());
        assert!(tray_requested(["gui".into(), "--tray".into()]));
        assert!(!tray_requested(["gui".into()]));
    }
}
