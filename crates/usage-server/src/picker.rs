use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::process::Command;

#[derive(Deserialize, Serialize)]
pub struct Selection {
    pub path: Option<PathBuf>,
}

// This runs in a helper process so macOS can create its native dialog on the
// main thread without blocking the dashboard's HTTP server.
pub async fn show(initial: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err("No graphical desktop session is available".into());
    }
    let chinese = crate::preferences::load()
        .ok()
        .and_then(|preferences| preferences.language)
        .is_some_and(|language| language == "zh");
    let title = if chinese {
        "选择 Codex 日志文件夹"
    } else {
        "Select Codex log folder"
    };
    let initial = initial.filter(|path| path.is_dir());
    #[cfg(any(windows, target_os = "macos"))]
    let path = {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(initial) = initial {
            dialog = dialog.set_directory(initial);
        }
        dialog.pick_folder()
    };
    #[cfg(target_os = "linux")]
    let path = linux_picker(initial, title).await?;
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    let path = {
        let _ = (initial, title);
        return Err("Folder selection is not supported on this platform".into());
    };
    let selection = Selection { path };
    println!("{}", serde_json::to_string(&selection)?);
    Ok(())
}

#[cfg(target_os = "linux")]
async fn linux_picker(
    initial: Option<PathBuf>,
    title: &str,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    use ashpd::desktop::{ResponseError, file_chooser::OpenFileRequest};
    use std::os::unix::ffi::OsStringExt;

    let request = OpenFileRequest::default()
        .title(Some(title))
        .directory(true)
        .multiple(false)
        .current_folder::<&PathBuf>(initial.as_ref())?;
    match request.send().await {
        Ok(request) => match request.response() {
            Ok(files) => match files.uris().first() {
                Some(uri) => uri
                    .to_file_path()
                    .map(Some)
                    .map_err(|_| "The selected folder is not a local path".into()),
                None => Ok(None),
            },
            Err(ashpd::Error::Response(ResponseError::Cancelled)) => Ok(None),
            Err(error) => Err(error.into()),
        },
        Err(_) => {
            let mut command = Command::new("zenity");
            command.args(["--file-selection", "--directory", "--title", title]);
            if let Some(initial) = initial {
                let mut folder = initial.into_os_string();
                folder.push("/");
                command.arg("--filename").arg(folder);
            }
            let output = command.output().map_err(
                |_| "No desktop file chooser is available; install a desktop portal or zenity",
            )?;
            match output.status.code() {
                Some(0) => {
                    let mut path = output.stdout;
                    if path.last() == Some(&b'\n') {
                        path.pop();
                    }
                    if path.is_empty() {
                        return Err("Folder picker returned an empty path".into());
                    }
                    Ok(Some(PathBuf::from(std::ffi::OsString::from_vec(path))))
                }
                Some(1) => Ok(None),
                _ => Err("The system folder picker failed".into()),
            }
        }
    }
}

#[cfg(unix)]
struct PickerGroup(Option<i32>);
#[cfg(unix)]
impl Drop for PickerGroup {
    fn drop(&mut self) {
        if let Some(group) = self.0 {
            // The helper starts its own process group, including any GUI fallback.
            unsafe { libc::kill(-group, libc::SIGTERM) };
        }
    }
}

pub async fn request(initial: Option<PathBuf>) -> Result<Selection, String> {
    let mut command =
        tokio::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.arg("--choose-source-dialog");
    if let Some(initial) = initial {
        command.arg("--picker-start").arg(initial);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    #[cfg(unix)]
    command.process_group(0);
    let child = command
        .spawn()
        .map_err(|e| format!("Cannot open folder picker: {e}"))?;
    #[cfg(unix)]
    let mut group = PickerGroup(child.id().and_then(|pid| i32::try_from(pid).ok()));
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Cannot open folder picker: {e}"))?;
    #[cfg(unix)]
    {
        group.0 = None;
    }
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Cannot open folder picker: {}", reason.trim()));
    }
    let selection: Selection = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Invalid folder picker response".to_string())?;
    if selection
        .path
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err("Folder picker returned a non-absolute path".into());
    }
    Ok(selection)
}
