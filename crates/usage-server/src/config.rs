//! Compatibility with existing configuration, device IDs and source discovery.
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    env, fs,
    io::Write,
    path::{Component, Path, PathBuf},
};
use usage_core::{
    Error,
    parser::LogSource,
    project::{command, expand_home, windows_path},
};
pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "Darwin"
    } else if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        env::consts::OS
    }
}
pub fn wsl() -> bool {
    env::var_os("WSL_DISTRO_NAME").is_some()
        || env::var_os("WSL_INTEROP").is_some()
        || fs::read_to_string("/proc/sys/kernel/osrelease").is_ok_and(|s| {
            let s = s.to_lowercase();
            s.contains("microsoft") || s.contains("wsl")
        })
}
pub fn config_dir() -> PathBuf {
    usage_runtime::config_dir()
}
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidData("file has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn read_json(path: &Path) -> Option<Value> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes)).ok()
}
pub fn slug(value: &str) -> String {
    let value = regex::Regex::new("[^a-z0-9]+")
        .unwrap()
        .replace_all(&value.to_lowercase(), "-")
        .trim_matches('-')
        .to_owned();
    if value.is_empty() {
        "codex".into()
    } else {
        value
    }
}
pub fn hostname() -> String {
    env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| command("hostname", &[]))
}
pub fn device_code(root: &Path) -> Result<String, Error> {
    let prefix = match platform() {
        "Darwin" => "mac".into(),
        "Windows" => "win".into(),
        _ if wsl() => "wsl".into(),
        "Linux" => "linux".into(),
        other => slug(other),
    };
    let mut identity = match platform() {
        "Darwin" => {
            let output = command("ioreg", &["-rd1", "-c", "IOPlatformExpertDevice"]);
            regex::Regex::new(r#""IOPlatformUUID"\s*=\s*"([^"]+)""#)
                .unwrap()
                .captures(&output)
                .map(|c| c[1].trim().to_owned())
                .unwrap_or_default()
        }
        "Windows" => {
            let output = command(
                "reg",
                &[
                    "query",
                    r"HKLM\SOFTWARE\Microsoft\Cryptography",
                    "/v",
                    "MachineGuid",
                ],
            );
            regex::Regex::new(r"MachineGuid\s+REG_\w+\s+([^\r\n]+)").unwrap().captures(&output).map(|c|c[1].trim().to_owned()).unwrap_or_else(||command("powershell",&["-NoProfile","-Command",r"(Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Cryptography').MachineGuid"]).lines().last().unwrap_or("").trim().into())
        }
        _ => ["/etc/machine-id", "/var/lib/dbus/machine-id"]
            .into_iter()
            .filter_map(|p| fs::read_to_string(p).ok())
            .map(|s| s.trim().to_owned())
            .find(|s| !s.is_empty())
            .unwrap_or_default(),
    };
    if identity.is_empty() {
        let path = root.join("device-seed.json");
        identity = read_json(&path)
            .and_then(|v| {
                v["seed"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        if identity.is_empty() {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).map_err(|e| Error::InvalidData(e.to_string()))?;
            identity = format!("{:x}", Sha256::digest(bytes));
            atomic_json(
                &path,
                &json!({"seed":identity,"created_at":usage_core::timestamp::format(chrono::Utc::now())}),
            )?;
        }
    }
    Ok(format!(
        "{prefix}-{}",
        &format!(
            "{:x}",
            Sha256::digest(format!("{prefix}:{}", identity.trim()))
        )[..8]
    ))
}
fn absolute(path: PathBuf) -> PathBuf {
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir().unwrap_or_default().join(path)
    };
    let mut normalized = PathBuf::new();
    for p in path.components() {
        match p {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
fn has_logs(path: &Path) -> bool {
    ["sessions", "archived_sessions", "session_index.jsonl"]
        .iter()
        .any(|s| path.join(s).exists())
}
#[cfg(windows)]
fn windows_wsl_path(path: &Path) -> bool {
    use std::path::Prefix;
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::UNC(host, _) | Prefix::VerbatimUNC(host, _) => {
                let host = host.to_string_lossy();
                host.eq_ignore_ascii_case("wsl$") || host.eq_ignore_ascii_case("wsl.localhost")
            }
            _ => false,
        },
        _ => false,
    }
}
#[cfg(not(windows))]
fn windows_wsl_path(_: &Path) -> bool {
    false
}
pub fn sources(homes: &[PathBuf], auto_windows: bool) -> Vec<LogSource> {
    let mut paths = if homes.is_empty() {
        vec![
            env::var("CODEX_HOME")
                .map(|s| expand_home(&s))
                .unwrap_or_else(|_| expand_home("~/.codex")),
        ]
    } else {
        homes
            .iter()
            .map(|p| expand_home(&p.to_string_lossy()))
            .collect()
    };
    let is_wsl = wsl();
    if homes.is_empty() && auto_windows && is_wsl {
        let mut candidates = Vec::new();
        let profile = command("cmd.exe", &["/c", "echo", "%USERPROFILE%"]);
        if let Some(path) = profile.lines().last().and_then(windows_path) {
            candidates.push(path.join(".codex"));
        }
        for user in ["USER", "USERNAME"]
            .iter()
            .filter_map(|key| env::var(key).ok())
            .filter(|s| !s.is_empty())
        {
            candidates.push(PathBuf::from("/mnt/c/Users").join(user).join(".codex"));
        }
        if !candidates.iter().any(|p| has_logs(p)) {
            let matches: Vec<_> = fs::read_dir("/mnt/c/Users")
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|p| p.path().join(".codex"))
                .filter(|p| has_logs(p))
                .collect();
            if matches.len() == 1 {
                candidates.extend(matches);
            }
        }
        paths.extend(candidates.into_iter().filter(|p| has_logs(p)));
    }
    let mut seen = HashSet::new();
    let mut ids = HashSet::new();
    let mut result = Vec::new();
    for path in paths {
        let home = absolute(path);
        if !seen.insert(home.clone()) {
            continue;
        }
        let label = if windows_wsl_path(&home) {
            "WSL"
        } else if is_wsl {
            if home.starts_with("/mnt/c/Users") {
                "Windows"
            } else {
                "WSL"
            }
        } else if platform() == "Darwin" {
            "macOS"
        } else {
            platform()
        };
        let base = slug(label);
        let mut id = base.clone();
        let mut n = 1;
        while ids.contains(&id) {
            n += 1;
            id = format!("{base}-{n}");
        }
        ids.insert(id.clone());
        result.push(LogSource {
            id,
            label: if n == 1 {
                label.into()
            } else {
                format!("{label} {n}")
            },
            home,
        });
    }
    result
}
pub fn source_rows(sources: &[LogSource]) -> Vec<Value> {
    sources
        .iter()
        .map(|s| json!({"id":s.id,"label":s.label,"codex_home":s.home,"is_remote":false}))
        .collect()
}
pub fn homes_display(sources: &[LogSource]) -> String {
    if sources.len() == 1 {
        return sources[0].home.to_string_lossy().into_owned();
    }
    sources
        .iter()
        .map(|s| format!("{}: {}", s.label, s.home.display()))
        .collect::<Vec<_>>()
        .join(" · ")
}
pub fn index_path(root: &Path, sources: &[LogSource]) -> PathBuf {
    let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(sources).unwrap()));
    root.join("index-v1").join(format!("{hash}.sqlite3"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn recognizes_wsl_unc_paths() {
        for path in [
            r"\\wsl.localhost\Ubuntu\home\user\.codex",
            r"\\wsl$\Ubuntu\home\user\.codex",
            r"\\?\UNC\wsl.localhost\Ubuntu\home\user\.codex",
            r"\\?\UNC\WSL$\Ubuntu\home\user\.codex",
        ] {
            assert!(windows_wsl_path(Path::new(path)), "{path}");
        }
        assert!(!windows_wsl_path(Path::new(
            r"\\wsl.localhost.example\share\logs"
        )));
        assert!(!windows_wsl_path(Path::new(r"C:\Users\user\.codex")));
    }

    #[test]
    fn multiple_sources_have_distinct_labels() {
        let root = tempfile::tempdir().unwrap();
        let homes = [root.path().join("first"), root.path().join("second")];
        let sources = sources(&homes, false);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].label, "Windows");
        assert_eq!(sources[1].label, "Windows 2");
        assert_ne!(sources[0].id, sources[1].id);
    }
}
