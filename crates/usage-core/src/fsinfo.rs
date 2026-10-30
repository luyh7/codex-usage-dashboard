//! Filesystem classification for Codex source homes. Network shares (SMB, NFS,
//! AFP, WebDAV, sshfs, 9p) are polled on a slower cadence and are never read
//! for background content audits; see `SourcePolicy`.
use std::{path::Path, time::Duration};

/// Cadence for local sources; unchanged from the original fixed poll.
pub const LOCAL_POLL: Duration = Duration::from_secs(2);
pub const DEFAULT_NETWORK_POLL_SECS: u64 = 30;
pub const MIN_NETWORK_POLL_SECS: u64 = 10;
/// Upper bound accepted from settings (one day).
pub const MAX_POLL_SECS: u64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePolicy {
    pub network: bool,
    pub poll_interval: Duration,
}
impl SourcePolicy {
    pub fn new(network: bool, poll_interval_secs: Option<u64>) -> Self {
        Self {
            network,
            poll_interval: poll_interval(network, poll_interval_secs),
        }
    }
    /// Network detection is automatic: `path` is classified with `is_network_path`.
    pub fn detect(path: &Path, poll_interval_secs: Option<u64>) -> Self {
        Self::new(is_network_path(path), poll_interval_secs)
    }
}

/// Local sources keep the 2 s cadence; network sources default to 30 s and
/// never poll faster than 10 s.
pub fn poll_interval(network: bool, requested_secs: Option<u64>) -> Duration {
    if !network {
        return LOCAL_POLL;
    }
    Duration::from_secs(
        requested_secs
            .unwrap_or(DEFAULT_NETWORK_POLL_SECS)
            .clamp(MIN_NETWORK_POLL_SECS, MAX_POLL_SECS),
    )
}

/// Filesystem type names (macOS `f_fstypename`, Linux mount table) that are
/// backed by a remote server.
pub fn is_network_fstype(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    matches!(
        name.as_str(),
        "smbfs"
            | "smb"
            | "smb2"
            | "smb3"
            | "cifs"
            | "nfs"
            | "nfs4"
            | "afpfs"
            | "webdav"
            | "davfs"
            | "ncpfs"
            | "9p"
            | "drvfs"
            | "afs"
            | "coda"
            | "ceph"
            | "glusterfs"
            | "lustre"
            | "sshfs"
    ) || name.starts_with("fuse.sshfs")
        || name.starts_with("fuse.rclone")
        || name.starts_with("fuse.s3fs")
        || name.starts_with("fuse.davfs")
        || name.starts_with("fuse.glusterfs")
        || name.starts_with("fuse.ceph")
}

/// macOS: a network type name, or a mount without `MNT_LOCAL`.
pub fn classify_macos(fstypename: &str, mnt_local: bool) -> bool {
    is_network_fstype(fstypename) || !mnt_local
}

pub const FUSE_SUPER_MAGIC: u32 = 0x6573_5546;
/// Linux `statfs.f_type` magic numbers of network filesystems.
pub fn is_network_magic(magic: u32) -> bool {
    matches!(
        magic,
        0xFF53_4D42 // CIFS
            | 0xFE53_4D42 // SMB2/SMB3
            | 0x0000_517B // SMB (legacy)
            | 0x0000_6969 // NFS
            | 0x0102_1997 // 9p (WSL2 drvfs)
            | 0x5346_414F // AFS
            | 0x7375_7245 // Coda
            | 0x00C3_6400 // Ceph
            | 0x0000_564C // NCP
    )
}

/// `\\server\share`, `//server/share` and `\\?\UNC\server\share`; device
/// (`\\.\`) and verbatim local (`\\?\C:\`) paths are not UNC shares.
pub fn is_unc_path(path: &str) -> bool {
    let p = path.replace('/', "\\");
    if let Some(rest) = p.strip_prefix("\\\\?\\") {
        return rest.len() > 4 && rest[..4].eq_ignore_ascii_case("UNC\\");
    }
    let Some(rest) = p.strip_prefix("\\\\") else {
        return false;
    };
    if rest.starts_with('.') && rest[1..].starts_with('\\') {
        return false;
    }
    let mut parts = rest.split('\\');
    matches!((parts.next(), parts.next()), (Some(server), Some(share)) if !server.is_empty() && !share.is_empty())
}

/// Whether `path` (or its nearest existing ancestor) lives on a network
/// filesystem. Unknown results are treated as local.
pub fn is_network_path(path: &Path) -> bool {
    if is_unc_path(&path.to_string_lossy()) {
        return true;
    }
    let mut current = Some(path);
    while let Some(candidate) = current {
        if let Some(network) = platform::probe(candidate) {
            return network;
        }
        current = candidate.parent();
    }
    false
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt, path::Path};
    pub fn probe(path: &Path) -> Option<bool> {
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: valid NUL-terminated path and a correctly sized out buffer;
        // the buffer is read only after statfs reports success.
        let stat = unsafe {
            if libc::statfs(c.as_ptr(), stat.as_mut_ptr()) != 0 {
                return None;
            }
            stat.assume_init()
        };
        // SAFETY: the kernel NUL-terminates f_fstypename within its array.
        let name = unsafe { CStr::from_ptr(stat.f_fstypename.as_ptr()) };
        let local = stat.f_flags & (libc::MNT_LOCAL as u32) != 0;
        Some(super::classify_macos(&name.to_string_lossy(), local))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{os::unix::ffi::OsStrExt, path::Path};
    pub fn probe(path: &Path) -> Option<bool> {
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: valid NUL-terminated path and a correctly sized out buffer;
        // the buffer is read only after statfs reports success.
        let stat = unsafe {
            if libc::statfs(c.as_ptr(), stat.as_mut_ptr()) != 0 {
                return None;
            }
            stat.assume_init()
        };
        // f_type's width differs by architecture; magics are 32-bit.
        let magic = stat.f_type as u32;
        if magic == super::FUSE_SUPER_MAGIC {
            // FUSE hides the backend; the mount table names it (fuse.sshfs).
            return Some(mount_type(path).is_some_and(|name| super::is_network_fstype(&name)));
        }
        Some(super::is_network_magic(magic))
    }
    fn mount_type(path: &Path) -> Option<String> {
        let path = path.canonicalize().ok()?;
        let mounts = std::fs::read_to_string("/proc/self/mounts").ok()?;
        mounts
            .lines()
            .filter_map(|line| {
                let mut fields = line.split(' ');
                let _source = fields.next()?;
                let target = fields.next()?.replace("\\040", " ");
                let kind = fields.next()?;
                path.starts_with(&target)
                    .then(|| (target.len(), kind.to_owned()))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, kind)| kind)
    }
}

#[cfg(windows)]
mod platform {
    use std::{
        os::windows::ffi::OsStrExt,
        path::{Component, Path, Prefix},
    };
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    const DRIVE_REMOTE: u32 = 4;
    pub fn probe(path: &Path) -> Option<bool> {
        let root = match path.components().next()? {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::UNC(..) | Prefix::VerbatimUNC(..) => return Some(true),
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    format!("{}:\\", letter as char)
                }
                _ => return None,
            },
            _ => return None,
        };
        let wide: Vec<u16> = std::ffi::OsStr::new(&root)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: NUL-terminated UTF-16 root path that outlives the call.
        let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
        Some(kind == DRIVE_REMOTE)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod platform {
    pub fn probe(_: &std::path::Path) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classifies_filesystem_names_and_magics() {
        for name in [
            "smbfs",
            "nfs",
            "afpfs",
            "webdav",
            "cifs",
            "SMB3",
            "fuse.sshfs",
            "9p",
        ] {
            assert!(is_network_fstype(name), "{name}");
        }
        for name in [
            "apfs", "hfs", "ext4", "btrfs", "tmpfs", "overlay", "fuse", "ntfs",
        ] {
            assert!(!is_network_fstype(name), "{name}");
        }
        assert!(classify_macos("smbfs", true));
        assert!(classify_macos("mystery", false));
        assert!(!classify_macos("apfs", true));
        for magic in [0xFF53_4D42, 0xFE53_4D42, 0x6969, 0x0102_1997] {
            assert!(is_network_magic(magic), "{magic:#x}");
        }
        // ext4, tmpfs, overlayfs, btrfs, and bare FUSE are not network types.
        for magic in [
            0xEF53,
            0x0102_1994,
            0x794C_7630,
            0x9123_683E,
            FUSE_SUPER_MAGIC,
        ] {
            assert!(!is_network_magic(magic), "{magic:#x}");
        }
    }
    #[test]
    fn recognizes_unc_paths() {
        for path in [
            r"\\server\share",
            r"\\nas\codex\.codex",
            "//server/share/x",
            r"\\?\UNC\server\share",
        ] {
            assert!(is_unc_path(path), "{path}");
        }
        for path in [
            r"C:\Users\me\.codex",
            r"\\?\C:\Users",
            r"\\.\pipe\x",
            r"\\server",
            "/Volumes/.codex",
            "",
        ] {
            assert!(!is_unc_path(path), "{path}");
        }
        assert!(is_network_path(Path::new(r"\\server\share\.codex")));
    }
    #[test]
    fn local_temporary_directories_are_not_network() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_network_path(dir.path()));
        // Missing homes fall back to their nearest existing ancestor.
        assert!(!is_network_path(&dir.path().join("missing/.codex")));
    }
    #[test]
    fn network_intervals_are_clamped_and_local_cadence_is_unchanged() {
        assert_eq!(poll_interval(true, Some(3)), Duration::from_secs(10));
        assert_eq!(poll_interval(true, Some(0)), Duration::from_secs(10));
        assert_eq!(poll_interval(true, Some(10)), Duration::from_secs(10));
        assert_eq!(poll_interval(true, Some(45)), Duration::from_secs(45));
        assert_eq!(poll_interval(true, None), Duration::from_secs(30));
        assert_eq!(poll_interval(false, Some(60)), LOCAL_POLL);
        assert_eq!(poll_interval(false, None), LOCAL_POLL);
        let network = SourcePolicy::new(true, None);
        assert!(network.network && network.poll_interval == Duration::from_secs(30));
        let local = SourcePolicy::new(false, Some(60));
        assert!(!local.network && local.poll_interval == LOCAL_POLL);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(SourcePolicy::detect(dir.path(), Some(5)), local);
    }
}
