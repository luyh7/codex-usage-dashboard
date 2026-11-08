//! Inspect WSL's running distributions without opening their UNC shares.
use std::{collections::HashSet, path::Path};

fn distribution_name(path: &str) -> Option<String> {
    let path = path.replace('/', "\\");
    let rest = path.strip_prefix(r"\\")?;
    let rest = if rest
        .get(..6)
        .is_some_and(|p| p.eq_ignore_ascii_case("?\\UNC\\"))
    {
        &rest[6..]
    } else {
        rest
    };
    let mut parts = rest.split('\\');
    let host = parts.next()?;
    let distro = parts.next()?;
    (!distro.is_empty()
        && (host.eq_ignore_ascii_case("wsl$") || host.eq_ignore_ascii_case("wsl.localhost")))
    .then(|| distro.to_lowercase())
}

pub fn distribution(path: &Path) -> Option<String> {
    if cfg!(windows) {
        distribution_name(&path.to_string_lossy())
    } else {
        None
    }
}

/// Match Windows' usual canonical UNC spelling without filesystem access.
/// This keeps an existing index's identity stable even while WSL is stopped.
pub fn lexical_home(path: &Path) -> Option<std::path::PathBuf> {
    distribution(path)?;
    let text = path.to_string_lossy().replace('/', "\\");
    let rest = text.strip_prefix(r"\\")?;
    let rest = if rest
        .get(..6)
        .is_some_and(|p| p.eq_ignore_ascii_case("?\\UNC\\"))
    {
        &rest[6..]
    } else {
        rest
    };
    let mut parts = Vec::new();
    for part in rest.split('\\') {
        match part {
            "" | "." => {}
            ".." if parts.len() > 2 => {
                parts.pop();
            }
            ".." => {}
            _ => parts.push(part),
        }
    }
    Some(format!(r"\\?\UNC\{}", parts.join("\\")).into())
}

#[derive(Default)]
pub struct RunningDistributions(HashSet<String>);

impl RunningDistributions {
    /// A failed/unsupported query leaves every WSL share paused.
    pub fn query() -> Self {
        #[cfg(windows)]
        {
            Self(
                running_output()
                    .map(|bytes| decode_names(&bytes))
                    .unwrap_or_default(),
            )
        }
        #[cfg(not(windows))]
        {
            Self::default()
        }
    }

    pub fn is_paused(&self, path: &Path) -> bool {
        distribution(path).is_some_and(|distro| !self.0.contains(&distro))
    }
}

#[cfg(any(windows, test))]
fn decode_names(bytes: &[u8]) -> HashSet<String> {
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.contains(&0) {
        if !bytes.len().is_multiple_of(2) {
            return HashSet::new();
        }
        let wide: Vec<_> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&wide).unwrap_or_default()
    } else {
        String::from_utf8(bytes.to_vec()).unwrap_or_default()
    };
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[cfg(windows)]
fn running_output() -> Option<Vec<u8>> {
    use std::{
        io::Read,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::Duration,
    };
    use wait_timeout::ChildExt;
    let program =
        std::path::PathBuf::from(std::env::var_os("SystemRoot")?).join("System32/wsl.exe");
    let mut child = Command::new(program)
        .args(["--list", "--running", "--quiet"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let success = match child.wait_timeout(Duration::from_secs(2)) {
        Ok(Some(status)) => status.success(),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            false
        }
    };
    let bytes = reader.join().ok()?;
    success.then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_only_wsl_share_hosts() {
        for path in [
            r"\\wsl.localhost\Ubuntu\home\me\.codex",
            r"\\WSL$\UBUNTU\home\me",
            r"\\?\UNC\wsl.localhost\Ubuntu\home",
            "//wsl$/Ubuntu/home",
        ] {
            assert_eq!(distribution_name(path).as_deref(), Some("ubuntu"), "{path}");
        }
        for path in [
            r"\\wsl.localhost.example\Ubuntu\home",
            r"\\nas\Ubuntu",
            r"C:\wsl.localhost\Ubuntu",
            r"\\wsl$\",
            r"\\?\C:\Ubuntu",
        ] {
            assert!(distribution_name(path).is_none(), "{path}");
        }
    }
    #[test]
    fn running_names_support_utf8_utf16_and_non_ascii() {
        let text = "Ubuntu\r\n开发环境\r\n";
        let expected = HashSet::from(["ubuntu".to_owned(), "开发环境".to_owned()]);
        assert_eq!(decode_names(text.as_bytes()), expected);
        let wide: Vec<_> = format!("\u{feff}{text}")
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(decode_names(&wide), expected);
        assert!(decode_names(b"").is_empty());
        assert!(decode_names(&[0xff, 0xfe, 0]).is_empty());
    }
    #[cfg(windows)]
    #[test]
    fn lexical_homes_have_stable_cache_paths_without_probing_wsl() {
        let expected = std::path::PathBuf::from(r"\\?\UNC\wsl.localhost\Ubuntu\home\me\.codex");
        for path in [
            r"\\wsl.localhost\Ubuntu\home\me\.codex",
            r"\\?\UNC\wsl.localhost\Ubuntu\home\me\.codex",
            r"\\wsl.localhost\Ubuntu\home\me\old\..\.codex",
        ] {
            assert_eq!(lexical_home(Path::new(path)).unwrap(), expected);
        }
        let empty = RunningDistributions::default();
        assert!(empty.is_paused(&expected));
        assert!(!empty.is_paused(Path::new(r"C:\Users\me\.codex")));
        let running = RunningDistributions(HashSet::from(["ubuntu".into()]));
        assert!(!running.is_paused(&expected));
    }
}
