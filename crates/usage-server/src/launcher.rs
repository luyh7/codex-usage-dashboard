//! Native process lifecycle. All normal launches use this executable directly.
use crate::{Args, config};
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const URL: &str = "http://127.0.0.1:8765/";
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
fn random() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn matches(health: &Value, args: &Args) -> bool {
    let expected = config::sources(&args.codex_home, !args.no_auto_windows);
    let mut homes: Vec<_> = expected
        .iter()
        .map(|s| s.home.to_string_lossy().into_owned())
        .collect();
    homes.sort();
    let mut actual: Vec<_> = health["codex_homes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    actual.sort();
    valid(health)
        && health["index"]["phase"] != "error"
        && health["build_id"] == env!("COUSASH_BUILD_ID")
        && homes == actual
        && args
            .timezone
            .as_ref()
            .is_none_or(|zone| health["timezone"].as_str() == Some(zone))
}
pub fn run(args: &Args) -> Result<()> {
    if args.host != "127.0.0.1" {
        return Err(
            "The desktop launcher binds 127.0.0.1; use --serve for an explicit server host".into(),
        );
    }
    fs::create_dir_all(root())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root(), fs::Permissions::from_mode(0o700))?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root().join("launcher.lock"))?;
    lock.lock_exclusive()?;
    if args.stop {
        stop()?;
        return Ok(());
    }
    if args.shortcut {
        return shortcut();
    }
    if let Some(current) = health()
        && matches(&current, args)
    {
        if !args.no_browser {
            open_browser()?;
        }
        println!("{URL}");
        return Ok(());
    }
    stop()?;
    let executable = std::env::current_exe()?;
    let token = random()?;
    let instance = random()?;
    let log = root().join("dashboard.log");
    let output = OpenOptions::new().create(true).append(true).open(&log)?;
    let mut child = Command::new(executable);
    child.arg("--serve").arg("--host").arg(&args.host);
    for home in &args.codex_home {
        child.arg("--codex-home").arg(home);
    }
    if args.no_auto_windows {
        child.arg("--no-auto-windows");
    }
    if let Some(zone) = &args.timezone {
        child.arg("--timezone").arg(zone);
    }
    child
        .env("COUSASH_SHUTDOWN_TOKEN", &token)
        .env("COUSASH_INSTANCE_ID", &instance)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // setsid detaches the server from the invoking terminal/session.
        unsafe {
            child.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::{
            Foundation::{HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation},
            System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
        };
        // Command::spawn inherits more than the redirected standard handles on
        // Windows. Keeping the caller's pipes open in the detached server makes
        // Node/skill launchers wait forever for EOF even after this process exits.
        for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            unsafe {
                let handle = GetStdHandle(stream);
                if !handle.is_null()
                    && handle != INVALID_HANDLE_VALUE
                    && SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) == 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
        }
        child.creation_flags(0x08000000 | 0x00000200); // NO_WINDOW | NEW_PROCESS_GROUP
    }
    let mut child = child.spawn()?;
    config::atomic_json(
        &root().join("launcher-process.json"),
        &json!({"pid":child.id(),"build_id":env!("COUSASH_BUILD_ID"),"instance_id":instance,"shutdown_token":token}),
    )?;
    let deadline = Instant::now() + crate::network::STARTUP_TIMEOUT;
    let progress_at = Instant::now() + Duration::from_secs(2);
    let mut reported_wait = false;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(format!("Dashboard exited ({status}); see {}", log.display()).into());
        }
        if let Some(current) = health()
            && current["instance_id"] == instance
        {
            if current["index"]["phase"] == "error" {
                return Err(format!("Dashboard index failed; see {}", log.display()).into());
            }
            if matches(&current, args) && current["index"]["phase"] == "ready" {
                if !args.no_browser {
                    open_browser()?;
                }
                println!("{URL}");
                return Ok(());
            }
        }
        if Instant::now() > deadline {
            return Err(format!("Dashboard startup timed out; see {}", log.display()).into());
        }
        if !reported_wait && Instant::now() >= progress_at {
            eprintln!("Waiting for the dashboard on port 8765; see {}", log.display());
            reported_wait = true;
        }
        thread::sleep(Duration::from_millis(100));
    }
}
pub fn open_browser() -> Result<()> {
    let mut command;
    if cfg!(windows) {
        command = Command::new("cmd");
        command.args(["/c", "start", "", URL]);
    } else if cfg!(target_os = "macos") {
        command = Command::new("open");
        command.arg(URL);
    } else if config::wsl() {
        command = Command::new("cmd.exe");
        command.args(["/c", "start", "", URL]);
    } else {
        command = Command::new("xdg-open");
        command.arg(URL);
    }
    match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => Ok(()),
        Err(_) => {
            eprintln!("Open {URL} in your browser.");
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
