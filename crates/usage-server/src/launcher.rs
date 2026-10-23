//! Native process lifecycle. All normal launches use this executable directly.
use crate::{Args, config};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
// Keep compatibility with existing managed Rust process records.
fn root() -> PathBuf {
    std::env::var_os("COUSASH_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| usage_core::project::expand_home("~/.codex-usage-dashboard"))
}
fn request(method: &str, path: &str, token: &str) -> Result<(u16, Value)> {
    let mut stream = TcpStream::connect_timeout(&address(), Duration::from_millis(400))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    // HTTP/1.0 + connection close avoids transfer framing in this local client.
    write!(
        stream,
        "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1:8765\r\nConnection: close\r\nContent-Length: 0\r\nAuthorization: Bearer {token}\r\n\r\n"
    )?;
    let mut bytes = Vec::new();
    stream.take(1024 * 1024).read_to_end(&mut bytes)?;
    let split = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("invalid local HTTP response")?;
    let header = std::str::from_utf8(&bytes[..split])?;
    let code = header
        .split_whitespace()
        .nth(1)
        .ok_or("missing HTTP status")?
        .parse()?;
    let body = &bytes[split + 4..];
    Ok((
        code,
        if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(body)?
        },
    ))
}
fn address() -> SocketAddr {
    "127.0.0.1:8765".parse().unwrap()
}
fn occupied() -> bool {
    TcpStream::connect_timeout(&address(), Duration::from_millis(200)).is_ok()
}
fn health() -> Option<Value> {
    request("GET", "/api/health", "")
        .ok()
        .filter(|(code, _)| *code == 200)
        .map(|(_, value)| value)
}
fn valid(health: &Value) -> bool {
    health["app"] == "codex-usage-dashboard"
        && health["implementation"] == "rust"
        && health["protocol_version"] == 1
}
fn stop() -> Result<()> {
    if !occupied() {
        return Ok(());
    }
    let health =
        health().ok_or("Port 8765 is occupied by an unidentified service; it was left running.")?;
    let record = config::read_json(&root().join("launcher-process.json")).unwrap_or(Value::Null);
    if !valid(&health)
        || !["pid", "build_id", "instance_id"]
            .iter()
            .all(|key| !record[key].is_null() && record[key] == health[key])
    {
        return Err("Port 8765 is not a Rust dashboard managed by this launcher; stop its owning process before retrying. It was left running.".into());
    }
    let token = record["shutdown_token"]
        .as_str()
        .filter(|s| s.len() >= 32 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or("invalid launcher record")?;
    if request("POST", "/api/shutdown", token)?.0 != 202 {
        return Err("Dashboard refused authenticated shutdown".into());
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while occupied() || process_alive(health["pid"].as_u64().ok_or("missing PID")? as u32) {
        if Instant::now() > deadline {
            return Err("Dashboard did not release port 8765; no force-kill was attempted".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        #[cfg(target_os = "linux")]
        if fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|s| s.rsplit_once(") ").map(|(_, tail)| tail.starts_with('Z')))
            .unwrap_or(false)
        {
            return false;
        }
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, WaitForSingleObject},
        };
        unsafe {
            let handle = OpenProcess(0x00100000, 0, pid);
            if handle.is_null() {
                return false;
            }
            let alive = WaitForSingleObject(handle, 0) == 258;
            CloseHandle(handle);
            alive
        }
    }
}
pub fn run(args: &Args) -> Result<()> {
    if args.host != "127.0.0.1" {
        return Err(
            "The launcher binds 127.0.0.1; use --serve for an explicit loopback host".into(),
        );
    }
    if args.stop || args.shortcut {
        return legacy_run(args);
    }
    let homes = if args.codex_home.is_empty() {
        crate::preferences::load()?.codex_homes
    } else {
        args.codex_home.clone()
    };
    let service = usage_runtime::ensure_service_options(
        &std::env::current_exe()?,
        &config::config_dir(),
        &homes,
        None,
        8765,
        !args.no_auto_windows,
        args.timezone.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    let url = format!("{}/", service.record.url());
    if !args.no_browser {
        open_browser_url(&url)?;
    }
    println!("{url}");
    Ok(())
}
fn legacy_run(args: &Args) -> Result<()> {
    if args.shortcut {
        return shortcut();
    }
    if let Some(record) =
        usage_runtime::Record::read(&config::config_dir()).map_err(|e| e.to_string())?
        && record.health().is_ok()
    {
        return record.stop().map_err(|e| e.to_string().into());
    }
    let _lock = usage_runtime::lock(&root(), "launcher.lock", Duration::from_secs(15))
        .map_err(|e| e.to_string())?;
    stop()
}
pub fn open_browser_url(url: &str) -> Result<()> {
    let mut command;
    if cfg!(windows) {
        command = Command::new("cmd");
        command.args(["/c", "start", "", url]);
    } else if cfg!(target_os = "macos") {
        command = Command::new("open");
        command.arg(url);
    } else if config::wsl() {
        command = Command::new("cmd.exe");
        command.args(["/c", "start", "", url]);
    } else {
        command = Command::new("xdg-open");
        command.arg(url);
    }
    match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => Ok(()),
        Err(_) => {
            eprintln!("Open {url} in your browser.");
            Ok(())
        }
    }
}
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}
fn shortcut() -> Result<()> {
    let binary = std::env::current_exe()?;
    let home = usage_core::project::expand_home("~");
    let desktop = if home.join("Desktop").is_dir() {
        home.join("Desktop")
    } else {
        home.clone()
    };
    let output;
    if cfg!(windows) {
        output = desktop.join("Codex Usage Dashboard.lnk");
        let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "''"));
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut({});$s.TargetPath={};$s.Arguments='--launch';$s.Save()",
            quote(&output),
            quote(&binary)
        );
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()?;
        if !status.success() {
            return Err("Failed to create desktop shortcut".into());
        }
    } else if cfg!(target_os = "macos") {
        output = desktop.join("Codex Usage Dashboard.command");
        fs::write(
            &output,
            format!("#!/bin/sh\nexec {} --launch\n", shell_quote(&binary)),
        )?;
    } else {
        let applications = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("applications");
        fs::create_dir_all(&applications)?;
        output = applications.join("codex-usage-dashboard.desktop");
        let escaped = binary
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%");
        fs::write(
            &output,
            format!(
                "[Desktop Entry]\nType=Application\nName=Codex Usage Dashboard\nExec=\"{escaped}\" --launch\nTerminal=false\nCategories=Development;\n"
            ),
        )?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&output, fs::Permissions::from_mode(0o755))?;
    }
    println!("{}", output.display());
    Ok(())
}
