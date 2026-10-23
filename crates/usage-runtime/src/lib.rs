//! Shared, configuration-scoped service discovery and lifecycle for CLI and desktop.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const PROTOCOL: u32 = 1;
pub fn scope_id(root: &Path) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(absolute(root).to_string_lossy().as_bytes())
    )
}
pub const TIMEOUT: Duration = Duration::from_secs(90);
pub fn ureq_url(value: &str) -> Result<url::Url> {
    Ok(url::Url::parse(value)?)
}

pub fn activate_desktop(root: &Path, action: &str) -> Result<()> {
    use std::io::{Read, Write};
    let value: Value = serde_json::from_slice(&fs::read(runtime_dir(root).join("desktop.json"))?)?;
    let address: SocketAddr = value["address"]
        .as_str()
        .ok_or("invalid desktop record")?
        .parse()?;
    if !address.ip().is_loopback() || value["config_dir"] != json!(absolute(root)) {
        return Err("invalid desktop scope".into());
    }
    let mut stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    writeln!(
        stream,
        "{}",
        json!({"token":value["token"],"action":action})
    )?;
    let mut reply = String::new();
    stream.take(4096).read_to_string(&mut reply)?;
    if reply.trim() != "ok" {
        return Err("desktop activation refused".into());
    }
    Ok(())
}

pub fn config_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default();
    if let Some(path) = std::env::var_os("COUSASH_CONFIG_DIR").filter(|p| !p.is_empty()) {
        let path = Path::new(&path);
        return absolute(
            &path
                .strip_prefix("~")
                .map(|rest| home.join(rest))
                .unwrap_or_else(|_| path.to_owned()),
        );
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"))
            .join("cousash")
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support/cousash")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("cousash")
    }
}
pub fn absolute(path: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    path.canonicalize().unwrap_or(path)
}
pub fn random() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn cache_dir(root: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        root.join("cache")
    }
    #[cfg(unix)]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let base = if cfg!(target_os = "macos") {
            home.join("Library/Caches")
        } else {
            std::env::var_os("XDG_CACHE_HOME")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".cache"))
        };
        base.join("cousash/scopes").join(scope_id(root))
    }
}
pub fn runtime_dir(root: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        root.join("runtime")
    }
    #[cfg(unix)]
    {
        cache_dir(root).join("runtime")
    }
}
pub fn private_dir(root: &Path) -> Result<()> {
    fs::create_dir_all(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        let metadata = fs::metadata(root)?;
        if metadata.permissions().mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err("cannot protect this directory; use a private cache directory on a native filesystem".into());
        }
    }
    // Windows user data inherits the current user's profile ACL. Harden explicit
    // state directories as well; no secrets may inherit a shared parent ACL.
    #[cfg(windows)]
    {
        let output = hidden(&mut Command::new("icacls"))
            .arg(root)
            .args(["/inheritance:r", "/grant:r"])
            .arg(format!("{}:(OI)(CI)F", std::env::var("USERNAME")?))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !output.success() {
            return Err("cannot protect runtime directory ACL".into());
        }
    }
    Ok(())
}
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().ok_or("missing parent directory")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn lock(root: &Path, name: &str, timeout: Duration) -> Result<File> {
    fs::create_dir_all(root)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(name))?;
    let deadline = Instant::now() + timeout;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {}
            Err(e) => return Err(e.into()),
        }
        if Instant::now() >= deadline {
            return Err(format!("{name} is busy; another instance may be starting").into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}
pub fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}
pub fn detached(command: &mut Command) -> &mut Command {
    hidden(command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation},
            System::Console::{
                GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
            },
        };
        for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            unsafe {
                let h = GetStdHandle(stream);
                if !h.is_null() && h != INVALID_HANDLE_VALUE {
                    SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
                }
            }
        }
    }
    command
}
pub fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, WaitForSingleObject},
        };
        unsafe {
            let h = OpenProcess(0x00100000, 0, pid);
            if h.is_null() {
                return false;
            }
            let result = WaitForSingleObject(h, 0) == 258;
            CloseHandle(h);
            result
        }
    }
}
/// Captures the exact desktop owner, rather than trusting a reusable PID.
pub struct OwnerProcess {
    #[cfg(windows)]
    handle: usize,
    #[cfg(unix)]
    pid: u32,
}
impl OwnerProcess {
    pub fn capture(pid: u32) -> Result<Self> {
        #[cfg(windows)]
        {
            let handle =
                unsafe { windows_sys::Win32::System::Threading::OpenProcess(0x00100000, 0, pid) };
            if handle.is_null() {
                return Err("desktop owner has exited".into());
            }
            Ok(Self {
                handle: handle as usize,
            })
        }
        #[cfg(unix)]
        {
            let owner = Self { pid };
            if !owner.is_alive() {
                return Err("desktop owner is not the parent process".into());
            }
            Ok(owner)
        }
    }
    pub fn is_alive(&self) -> bool {
        #[cfg(windows)]
        {
            unsafe {
                windows_sys::Win32::System::Threading::WaitForSingleObject(self.handle as _, 0)
                    == 258
            }
        }
        #[cfg(unix)]
        {
            unsafe { libc::getppid() as u32 == self.pid }
        }
    }
}
#[cfg(windows)]
impl Drop for OwnerProcess {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Record {
    pub schema_version: u32,
    pub address: SocketAddr,
    pub pid: u32,
    pub instance_id: String,
    pub version: String,
    pub build_id: String,
    pub protocol_version: u32,
    pub config_dir: PathBuf,
    pub codex_homes: Vec<PathBuf>,
    #[serde(default)]
    pub executable: PathBuf,
    #[serde(default)]
    pub auto_windows: bool,
    // Serialized only to a private state file. Never return a Record to the UI.
    pub shutdown_token: String,
}
impl Record {
    pub fn read(root: &Path) -> Result<Option<Self>> {
        let bytes = match fs::read(runtime_dir(root).join("instance.json")) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let record: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "invalid instance record; inspect runtime/instance.json")?;
        if record.schema_version != 1
            || record.protocol_version != PROTOCOL
            || !record.address.ip().is_loopback()
            || record.address.port() == 0
            || record.config_dir != absolute(root)
            || record.instance_id.len() != 64
            || record.shutdown_token.len() != 64
        {
            return Err("invalid or incompatible instance record".into());
        }
        Ok(Some(record))
    }
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }
    pub fn health(&self) -> Result<Value> {
        let value = get_json(&format!("{}/api/health", self.url()))?;
        if value["app"] != "codex-usage-dashboard"
            || value["protocol_version"] != self.protocol_version
            || value["pid"] != self.pid
            || value["instance_id"] != self.instance_id
            || value["build_id"] != self.build_id
            || value["config_scope"] != scope_id(&self.config_dir)
            || !alive(self.pid)
        {
            return Err("instance identity mismatch; process left running".into());
        }
        Ok(value)
    }
    pub fn public_status(&self) -> Result<Value> {
        let health = self.health()?;
        Ok(
            json!({"state":if health["index"]["phase"]=="error" {"error"} else if health["index"]["phase"]=="ready" {"ready"} else {"starting"},"url":self.url(),"pid":self.pid,"build_id":self.build_id,"version":self.version,"index":health["index"]}),
        )
    }
    pub fn stop(&self) -> Result<()> {
        self.health()?;
        let response = agent()
            .post(format!("{}/api/shutdown", self.url()))
            .header("Authorization", format!("Bearer {}", self.shutdown_token))
            .send_empty()?;
        if response.status() != 202 {
            return Err("shutdown refused".into());
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.health().is_ok() || alive(self.pid) {
            if Instant::now() >= deadline {
                return Err("service did not stop; no force kill attempted".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }
}
pub fn agent() -> ureq::Agent {
    agent_with_timeout(Duration::from_secs(10))
}
pub fn agent_with_timeout(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(timeout))
        .http_status_as_error(true)
        .max_redirects(0)
        .build()
        .into()
}
pub fn get_json(url: &str) -> Result<Value> {
    Ok(agent().get(url).call()?.body_mut().read_json()?)
}

pub struct Service {
    pub record: Record,
    pub owned: bool,
}
pub fn ensure_service(
    binary: &Path,
    root: &Path,
    homes: &[PathBuf],
    owner: Option<u32>,
) -> Result<Service> {
    ensure_service_on_port(binary, root, homes, owner, 0)
}
pub fn ensure_service_on_port(
    binary: &Path,
    root: &Path,
    homes: &[PathBuf],
    owner: Option<u32>,
    port: u16,
) -> Result<Service> {
    ensure_service_options(binary, root, homes, owner, port, false, None)
}
pub fn ensure_service_options(
    binary: &Path,
    root: &Path,
    homes: &[PathBuf],
    owner: Option<u32>,
    port: u16,
    auto_windows: bool,
    timezone: Option<&str>,
) -> Result<Service> {
    fs::create_dir_all(root)?;
    let runtime = runtime_dir(root);
    private_dir(&runtime)?;
    let _lock = lock(&runtime, "startup.lock", TIMEOUT)?;
    use wait_timeout::ChildExt;
    let mut inspection = hidden(&mut Command::new(binary))
        .arg("--build-info")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    if inspection.wait_timeout(Duration::from_secs(5))?.is_none() {
        let _ = inspection.kill();
        let _ = inspection.wait();
        return Err("bundled service inspection timed out".into());
    }
    let output = inspection.wait_with_output()?;
    if !output.status.success() {
        return Err("cannot inspect bundled service".into());
    }
    let info: Value = serde_json::from_slice(&output.stdout)?;
    let discovered = match Record::read(root) {
        Ok(record) => record,
        Err(error) => {
            let _writer = lock(&runtime, "service.lock", Duration::ZERO)
                .map_err(|_| format!("{error}; the configuration is still owned by a service"))?;
            fs::rename(
                runtime.join("instance.json"),
                runtime.join(format!("invalid-instance-{}.json", random()?)),
            )?;
            None
        }
    };
    if let Some(record) = discovered
        && let Ok(health) = record.health()
    {
        if record.build_id != info["build_id"].as_str().unwrap_or("")
            || record.codex_homes != homes
            || (std::env::var_os("WSL_DISTRO_NAME").is_some()
                && record.auto_windows != auto_windows)
            || timezone.is_some_and(|zone| health["timezone"] != zone)
        {
            return Err("another service version or log source owns this configuration; quit it before retrying, or choose a separate COUSASH_CONFIG_DIR".into());
        }
        return Ok(Service {
            record,
            owned: false,
        });
    }
    let token = random()?;
    let instance = random()?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(runtime.join("dashboard.log"))?;
    let mut command = Command::new(binary);
    command
        .args(["--serve", "--port", &port.to_string(), "--managed"])
        .env("COUSASH_CONFIG_DIR", root)
        .env("COUSASH_INSTANCE_ID", &instance)
        .env("COUSASH_SHUTDOWN_TOKEN", &token)
        .env_remove("COUSASH_OWNER_PID")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    if !auto_windows {
        command.arg("--no-auto-windows");
    }
    if let Some(zone) = timezone {
        command.arg("--timezone").arg(zone);
    }
    if let Some(pid) = owner {
        command.env("COUSASH_OWNER_PID", pid.to_string());
    }
    for home in homes {
        command.arg("--codex-home").arg(home);
    }
    let mut child = detached(&mut command).spawn()?;
    match wait_ready(&mut child, root, &instance) {
        Ok(record) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(Service {
                record,
                owned: true,
            })
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            if Record::read(root)
                .ok()
                .flatten()
                .is_some_and(|r| r.instance_id == instance)
            {
                let _ = fs::remove_file(runtime.join("instance.json"));
            }
            Err(format!("{e}; see {}", runtime.join("dashboard.log").display()).into())
        }
    }
}
fn wait_ready(child: &mut Child, root: &Path, instance: &str) -> Result<Record> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(format!("service exited: {status}").into());
        }
        if let Some(record) = Record::read(root).ok().flatten()
            && record.instance_id == instance
            && let Ok(health) = record.health()
        {
            if health["index"]["phase"] == "error" {
                return Err(format!("index failed: {}", health["index"]["error"]).into());
            }
            // The loading page remains responsive while indexing finishes.
            return Ok(record);
        }
        if Instant::now() >= deadline {
            return Err("service startup timed out".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Owns the server writer lock and removes only this server's record on exit.
pub struct Registration {
    _lock: File,
    root: PathBuf,
    instance: String,
}
pub fn register(mut record: Record) -> Result<Registration> {
    fs::create_dir_all(&record.config_dir)?;
    record.config_dir = absolute(&record.config_dir);
    let root = record.config_dir.clone();
    let runtime = runtime_dir(&root);
    private_dir(&runtime)?;
    let lock = lock(&runtime, "service.lock", Duration::ZERO)?;
    atomic_json(&runtime.join("instance.json"), &record)?;
    Ok(Registration {
        _lock: lock,
        root,
        instance: record.instance_id,
    })
}
impl Drop for Registration {
    fn drop(&mut self) {
        if Record::read(&self.root)
            .ok()
            .flatten()
            .is_some_and(|r| r.instance_id == self.instance)
        {
            let _ = fs::remove_file(runtime_dir(&self.root).join("instance.json"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lock_has_bounded_wait_and_recovers() {
        let d = tempfile::tempdir().unwrap();
        let first = lock(d.path(), "test.lock", Duration::ZERO).unwrap();
        assert!(lock(d.path(), "test.lock", Duration::from_millis(60)).is_err());
        drop(first);
        assert!(lock(d.path(), "test.lock", Duration::ZERO).is_ok());
    }
    #[test]
    fn rejects_remote_wrong_scope_and_corrupt_records() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(runtime_dir(root)).unwrap();
        fs::write(runtime_dir(root).join("instance.json"), b"{").unwrap();
        assert!(Record::read(root).is_err());
        let mut r = Record {
            schema_version: 1,
            address: "192.168.1.1:1234".parse().unwrap(),
            pid: 1,
            instance_id: "a".repeat(64),
            version: "1".into(),
            build_id: "x".into(),
            protocol_version: PROTOCOL,
            config_dir: absolute(root),
            codex_homes: vec![],
            executable: PathBuf::new(),
            auto_windows: false,
            shutdown_token: "b".repeat(64),
        };
        atomic_json(&runtime_dir(root).join("instance.json"), &r).unwrap();
        assert!(Record::read(root).is_err());
        r.address = "127.0.0.1:1234".parse().unwrap();
        r.config_dir = PathBuf::from("wrong");
        atomic_json(&runtime_dir(root).join("instance.json"), &r).unwrap();
        assert!(Record::read(root).is_err());
        r.config_dir = absolute(root);
        atomic_json(&runtime_dir(root).join("instance.json"), &r).unwrap();
        assert!(Record::read(root).unwrap().is_some());
    }
}
