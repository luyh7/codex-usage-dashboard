use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct Directories {
    source: Option<PathBuf>,
    export: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub enum Kind {
    Source,
    Export,
}

pub struct History {
    file: PathBuf,
    directories: Mutex<Directories>,
}

impl History {
    pub fn load(root: &Path) -> Self {
        let file = root.join("desktop-file-dialogs.json");
        let directories = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            file,
            directories: Mutex::new(directories),
        }
    }

    pub fn directory(&self, kind: Kind) -> Option<PathBuf> {
        let directories = self.directories.lock().unwrap();
        match kind {
            Kind::Source => &directories.source,
            Kind::Export => &directories.export,
        }
        .as_ref()
        .filter(|path| path.is_dir())
        .cloned()
    }

    pub fn remember(&self, kind: Kind, directory: &Path) -> usage_runtime::Result<()> {
        if !directory.is_dir() {
            return Err("selected path is not a directory".into());
        }
        let mut directories = self.directories.lock().unwrap();
        let mut next = directories.clone();
        let field = match kind {
            Kind::Source => &mut next.source,
            Kind::Export => &mut next.export,
        };
        *field = Some(directory.to_owned());
        usage_runtime::atomic_json(&self.file, &next)?;
        *directories = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn independent_directories_survive_concurrent_saves_and_reopen() {
        let root = tempfile::tempdir().unwrap();
        let (export, source) = (
            root.path().join("导出 文件"),
            root.path().join("Codex logs"),
        );
        fs::create_dir(&export).unwrap();
        fs::create_dir(&source).unwrap();
        let history = Arc::new(History::load(root.path()));
        let source_writer = history.clone();
        let selected = source.clone();
        let writer =
            std::thread::spawn(move || source_writer.remember(Kind::Source, &selected).unwrap());
        history.remember(Kind::Export, &export).unwrap();
        writer.join().unwrap();
        let reopened = History::load(root.path());
        assert_eq!(reopened.directory(Kind::Export), Some(export));
        assert_eq!(reopened.directory(Kind::Source), Some(source));
    }

    #[test]
    fn corrupt_missing_and_removed_directories_use_the_callers_fallback() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("desktop-file-dialogs.json");
        let history = History::load(root.path());
        assert!(history.directory(Kind::Export).is_none());
        assert!(!file.exists());
        fs::write(&file, b"{broken").unwrap();
        assert!(History::load(root.path()).directory(Kind::Source).is_none());
        assert_eq!(fs::read(&file).unwrap(), b"{broken");
        let directory = root.path().join("removed");
        fs::create_dir(&directory).unwrap();
        history.remember(Kind::Export, &directory).unwrap();
        fs::remove_dir(directory).unwrap();
        assert!(History::load(root.path()).directory(Kind::Export).is_none());
        assert!(history.remember(Kind::Source, &file).is_err());
    }

    #[test]
    fn failed_write_preserves_the_previous_directory_and_unrelated_settings() {
        let root = tempfile::tempdir().unwrap();
        let (first, second) = (root.path().join("first"), root.path().join("second"));
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let geometry = root.path().join("desktop-settings.json");
        fs::write(&geometry, b"{\"width\":1380,\"height\":900}").unwrap();
        let history = History::load(root.path());
        history.remember(Kind::Export, &first).unwrap();
        let original = fs::read(&history.file).unwrap();
        let backup = root.path().join("previous.json");
        fs::rename(&history.file, &backup).unwrap();
        fs::create_dir(&history.file).unwrap();
        assert!(history.remember(Kind::Export, &second).is_err());
        assert_eq!(history.directory(Kind::Export), Some(first));
        assert_eq!(fs::read(backup).unwrap(), original);
        assert_eq!(
            fs::read(geometry).unwrap(),
            b"{\"width\":1380,\"height\":900}"
        );
    }
}
