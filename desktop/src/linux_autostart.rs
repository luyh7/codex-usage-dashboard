use super::desktop_entry::command;
use std::{fs, io::Write, path::PathBuf};
use tauri::Manager;

fn paths(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let name = format!("{}.desktop", app.package_info().name);
    let current = app
        .path()
        .config_dir()
        .map_err(|e| e.to_string())?
        .join("autostart")
        .join(&name);
    // auto-launch <=0.6 ignored XDG_CONFIG_HOME. Recognize and remove our old
    // entry when the user changes this setting, avoiding duplicate launches.
    let legacy = app
        .path()
        .home_dir()
        .map_err(|e| e.to_string())?
        .join(".config/autostart")
        .join(name);
    Ok((current, legacy))
}

pub fn enabled(app: &tauri::AppHandle) -> Result<bool, String> {
    let (current, legacy) = paths(app)?;
    for path in [current, legacy] {
        match fs::symlink_metadata(path) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(false)
}

pub fn set_enabled(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let (current, legacy) = paths(app)?;
    if enabled {
        let executable = match app.env().appimage {
            Some(path) => PathBuf::from(path),
            None => std::env::current_exe().map_err(|e| e.to_string())?,
        };
        let executable = executable.to_str().ok_or("application path is not UTF-8")?;
        let contents = format!(
            "[Desktop Entry]\nType=Application\nVersion=1.0\nName={}\nExec={}\nStartupNotify=false\nTerminal=false\n",
            app.package_info().name,
            command(executable)
        );
        let directory = current.parent().ok_or("invalid autostart directory")?;
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
        file.write_all(contents.as_bytes())
            .map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(&current).map_err(|e| e.to_string())?;
    }
    for path in [
        (!enabled).then_some(current.clone()),
        (legacy != current).then_some(legacy),
    ]
    .into_iter()
    .flatten()
    {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}
