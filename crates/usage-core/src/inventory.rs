//! Cached directory inventory with bounded historical metadata checks. Native
//! watcher hints invalidate these caches; polling independently detects changes.
use crate::parser::LogSource;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStamp {
    pub size: u64,
    pub modified_ns: i64,
    pub changed_ns: i64,
    pub device: u64,
    pub inode: u64,
    #[serde(default)]
    pub inode_high: u64,
}
impl FileStamp {
    pub fn read(path: &Path) -> std::io::Result<Self> {
        #[cfg(not(windows))]
        {
            Self::from_metadata(&fs::metadata(path)?)
        }
        #[cfg(windows)]
        {
            use std::{
                fs::OpenOptions,
                mem::{MaybeUninit, size_of},
                os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
            };
            use windows_sys::Win32::Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_ID_INFO, FILE_READ_ATTRIBUTES, FileBasicInfo, FileIdInfo,
                GetFileInformationByHandle, GetFileInformationByHandleEx,
            };
            let file = OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)?;
            let mut stamp = Self::from_metadata(&file.metadata()?)?;
            let mut basic = MaybeUninit::<FILE_BASIC_INFO>::uninit();
            let mut id = MaybeUninit::<FILE_ID_INFO>::uninit();
            // SAFETY: the handle is live, the output buffers are correctly
            // sized and aligned, and each is initialized only after success.
            unsafe {
                if GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    FileBasicInfo,
                    basic.as_mut_ptr().cast(),
                    size_of::<FILE_BASIC_INFO>() as u32,
                ) == 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                stamp.changed_ns = basic.assume_init().ChangeTime;
                if GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    FileIdInfo,
                    id.as_mut_ptr().cast(),
                    size_of::<FILE_ID_INFO>() as u32,
                ) != 0
                {
                    let id = id.assume_init();
                    stamp.device = id.VolumeSerialNumber;
                    stamp.inode = u64::from_le_bytes(id.FileId.Identifier[..8].try_into().unwrap());
                    stamp.inode_high =
                        u64::from_le_bytes(id.FileId.Identifier[8..].try_into().unwrap());
                } else {
                    // WSL's Windows file share does not implement FileIdInfo,
                    // but provides the volume and 64-bit file index here.
                    let mut legacy = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
                    if GetFileInformationByHandle(file.as_raw_handle(), legacy.as_mut_ptr()) == 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    let legacy = legacy.assume_init();
                    stamp.device = u64::from(legacy.dwVolumeSerialNumber);
                    stamp.inode =
                        (u64::from(legacy.nFileIndexHigh) << 32) | u64::from(legacy.nFileIndexLow);
                    stamp.inode_high = 0;
                }
            }
            Ok(stamp)
        }
    }
    fn from_metadata(meta: &fs::Metadata) -> std::io::Result<Self> {
        let time = meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map(|t| t.as_nanos() as i64)
            .unwrap_or_else(|e| -(e.duration().as_nanos() as i64));
        #[cfg(unix)]
        let (device, inode, changed) = {
            use std::os::unix::fs::MetadataExt;
            (
                meta.dev(),
                meta.ino(),
                meta.ctime()
                    .saturating_mul(1_000_000_000)
                    .saturating_add(meta.ctime_nsec()),
            )
        };
        #[cfg(windows)]
        let (device, inode, changed) = {
            use std::os::windows::fs::MetadataExt;
            (0, meta.creation_time(), meta.last_write_time() as i64)
        };
        #[cfg(not(any(unix, windows)))]
        let (device, inode, changed) = (0, 0, time);
        Ok(Self {
            size: meta.len(),
            modified_ns: time,
            changed_ns: changed,
            device,
            inode,
            inode_high: 0,
        })
    }
    pub fn same_identity(&self, other: &Self) -> bool {
        self.device == other.device
            && (self.inode != 0 || self.inode_high != 0)
            && self.inode == other.inode
            && self.inode_high == other.inode_high
    }
}
#[derive(Debug, Clone)]
pub struct InventoryFile {
    pub source: LogSource,
    pub path: PathBuf,
    pub kind: String,
    pub stamp: FileStamp,
}
struct Directory {
    checked: Instant,
    stamp: FileStamp,
    files: Vec<PathBuf>,
    children: Vec<PathBuf>,
}
#[derive(Debug, Default, Clone, Serialize)]
pub struct InventoryMetrics {
    pub metadata_checks: u64,
    pub directory_reads: u64,
    pub historical_hits: u64,
}
pub struct Inventory {
    sources: Vec<LogSource>,
    directories: HashMap<PathBuf, Directory>,
    metadata: HashMap<PathBuf, (Instant, FileStamp)>,
    pub metrics: InventoryMetrics,
}
impl Inventory {
    pub fn new(sources: Vec<LogSource>) -> Self {
        Self {
            sources,
            directories: HashMap::new(),
            metadata: HashMap::new(),
            metrics: InventoryMetrics::default(),
        }
    }
    pub fn invalidate(&mut self, path: &Path) {
        self.metadata.remove(path);
        self.directories.remove(path);
        if let Some(parent) = path.parent() {
            self.directories.remove(parent);
        }
    }
    pub fn invalidate_all(&mut self) {
        self.metadata.clear();
        self.directories.clear();
    }
    fn directory(&mut self, path: &Path) -> std::io::Result<Option<(Vec<PathBuf>, Vec<PathBuf>)>> {
        let stamp = match FileStamp::read(path) {
            Ok(stamp) => stamp,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        if let Some(cached) = self.directories.get(path)
            && cached.stamp == stamp
            && cached.checked.elapsed() < Duration::from_secs(5)
        {
            return Ok(Some((cached.files.clone(), cached.children.clone())));
        }
        let mut files = Vec::new();
        let mut children = Vec::new();
        self.metrics.directory_reads += 1;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                children.push(path);
            } else if path.extension().is_some_and(|e| e == "jsonl")
                || crate::reader::is_compressed(&path)
            {
                self.metadata.remove(&path);
                files.push(path);
            }
        }
        // Codex briefly keeps both representations while publishing a
        // compressed or resumed log. Prefer the plain file, as Codex does.
        let plain: HashSet<_> = files
            .iter()
            .filter(|p| !crate::reader::is_compressed(p))
            .cloned()
            .collect();
        files
            .retain(|p| !crate::reader::is_compressed(p) || !plain.contains(&p.with_extension("")));
        self.directories.insert(
            path.into(),
            Directory {
                checked: Instant::now(),
                stamp,
                files: files.clone(),
                children: children.clone(),
            },
        );
        Ok(Some((files, children)))
    }
    pub fn discover(&mut self) -> std::io::Result<Vec<InventoryFile>> {
        self.discover_sources(None, &HashSet::new())
    }
    /// Scan only `due` sources (all when `None`); skipped sources keep their
    /// caches. Day-old history on network sources is re-stamped at most every
    /// five minutes; `invalidate_all` (force refresh) still checks everything.
    pub fn discover_sources(
        &mut self,
        due: Option<&HashSet<String>>,
        network: &HashSet<String>,
    ) -> std::io::Result<Vec<InventoryFile>> {
        let network_homes: HashSet<_> = self
            .sources
            .iter()
            .filter(|s| network.contains(&s.id))
            .map(|s| s.home.clone())
            .collect();
        self.discover_scoped(due, |home| {
            if network_homes.contains(home) {
                Duration::from_secs(300)
            } else if home.to_string_lossy().starts_with("/mnt/") {
                Duration::from_secs(2)
            } else {
                Duration::ZERO
            }
        })
    }
    #[cfg(test)]
    fn discover_with_policy(
        &mut self,
        history_ttl: impl Fn(&Path) -> Duration,
    ) -> std::io::Result<Vec<InventoryFile>> {
        self.discover_scoped(None, history_ttl)
    }
    fn discover_scoped(
        &mut self,
        due: Option<&HashSet<String>>,
        history_ttl: impl Fn(&Path) -> Duration,
    ) -> std::io::Result<Vec<InventoryFile>> {
        let now = Instant::now();
        let mut checked_paths = Vec::new();
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut present = HashSet::new();
        let mut skipped = Vec::new();
        for source in self.sources.clone() {
            if due.is_some_and(|due| !due.contains(&source.id)) {
                skipped.push(source.home);
                continue;
            }
            let ttl = history_ttl(&source.home);
            let mut retried = HashSet::new();
            let mut pending = vec![
                (source.home.join("archived_sessions"), "archived"),
                (source.home.join("sessions"), "active"),
            ];
            while let Some((directory, kind)) = pending.pop() {
                visited.insert(directory.clone());
                let Some((files, children)) = self.directory(&directory)? else {
                    continue;
                };
                let result_start = result.len();
                let mut missing = false;
                for path in files {
                    let cached = self.metadata.get(&path).filter(|(checked, stamp)| {
                        stamp.modified_ns / 1_000_000_000 < epoch - 86400
                            && now.duration_since(*checked) < ttl
                    });
                    let stamp = if let Some((_, stamp)) = cached {
                        self.metrics.historical_hits += 1;
                        stamp.clone()
                    } else {
                        self.metrics.metadata_checks += 1;
                        let stamp = match FileStamp::read(&path) {
                            Ok(stamp) => stamp,
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                missing = true;
                                self.metadata.remove(&path);
                                continue;
                            }
                            Err(e) => return Err(e),
                        };
                        self.metadata.insert(path.clone(), (now, stamp.clone()));
                        checked_paths.push(path.clone());
                        stamp
                    };
                    present.insert(path.clone());
                    result.push(InventoryFile {
                        source: source.clone(),
                        path,
                        kind: kind.into(),
                        stamp,
                    });
                }
                // Directory timestamps can remain unchanged during a rapid
                // plain/compressed replacement. A missing cached entry proves
                // the listing stale: rediscover once before publishing usage.
                if missing && retried.insert(directory.clone()) {
                    self.directories.remove(&directory);
                    result.truncate(result_start);
                    pending.push((directory, kind));
                } else if kind == "active" {
                    pending.extend(children.into_iter().rev().map(|p| (p, kind)));
                }
            }
        }
        // A mounted sweep can itself exceed the TTL. Start the reuse window at
        // completed publication, otherwise every subsequent scan immediately
        // expires all rows again. Cache hits never extend their expiry.
        let completed = Instant::now();
        for path in checked_paths {
            if let Some((checked, _)) = self.metadata.get_mut(&path) {
                *checked = completed;
            }
        }
        let kept = |path: &Path| skipped.iter().any(|home| path.starts_with(home));
        self.directories
            .retain(|path, _| visited.contains(path) || kept(path));
        self.metadata
            .retain(|path, _| present.contains(path) || kept(path));
        result.sort_by_key(|file| std::cmp::Reverse(file.stamp.modified_ns));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missed_create_with_colliding_directory_stamp_expires() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("sessions");
        fs::create_dir(&directory).unwrap();
        let mut inventory = Inventory::new(vec![LogSource {
            id: "fixture".into(),
            label: "Synthetic".into(),
            home: root.path().into(),
        }]);
        assert!(inventory.discover().unwrap().is_empty());
        fs::write(directory.join("new.jsonl"), b"{}\n").unwrap();
        let cached = inventory.directories.get_mut(&directory).unwrap();
        cached.stamp = FileStamp::read(&directory).unwrap();
        assert!(inventory.discover().unwrap().is_empty());
        inventory.directories.get_mut(&directory).unwrap().checked =
            Instant::now() - Duration::from_secs(6);
        assert_eq!(inventory.discover().unwrap().len(), 1);
    }
    #[test]
    fn missing_cached_plain_log_rediscovers_compressed_replacement() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("sessions");
        fs::create_dir(&directory).unwrap();
        let plain = directory.join("rollout.jsonl");
        let compressed = plain.with_extension("jsonl.zst");
        fs::write(&plain, b"{}\n").unwrap();
        let mut inventory = Inventory::new(vec![LogSource {
            id: "fixture".into(),
            label: "Synthetic".into(),
            home: root.path().into(),
        }]);
        assert_eq!(inventory.discover().unwrap()[0].path, plain);
        fs::write(
            &compressed,
            zstd::stream::encode_all(&b"{}\n"[..], 1).unwrap(),
        )
        .unwrap();
        fs::remove_file(&plain).unwrap();
        // Reproduce a filesystem whose directory stamp did not expose the swap.
        inventory.directories.get_mut(&directory).unwrap().stamp =
            FileStamp::read(&directory).unwrap();
        let files = inventory.discover().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, compressed);
        assert_eq!(inventory.metrics.directory_reads, 2);
    }
    #[test]
    fn historical_cache_hits_do_not_extend_expiry_and_hints_invalidate() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("sessions")).unwrap();
        let path = root.path().join("sessions/rollout-old.jsonl");
        fs::write(&path, b"{}\n").unwrap();
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
            .unwrap();
        let mut inventory = Inventory::new(vec![LogSource {
            id: "fixture".into(),
            label: "Synthetic".into(),
            home: root.path().into(),
        }]);
        let ttl = |_: &Path| Duration::from_secs(2);
        inventory.discover_with_policy(ttl).unwrap();
        let checked = inventory.metadata[&path].0;
        inventory.discover_with_policy(ttl).unwrap();
        assert_eq!(inventory.metrics.metadata_checks, 1);
        assert_eq!(inventory.metadata[&path].0, checked);
        inventory.metadata.get_mut(&path).unwrap().0 = Instant::now() - Duration::from_secs(3);
        inventory.discover_with_policy(ttl).unwrap();
        assert_eq!(inventory.metrics.metadata_checks, 2);
        inventory.invalidate(&path);
        inventory.discover_with_policy(ttl).unwrap();
        assert_eq!(inventory.metrics.metadata_checks, 3);
    }
}
