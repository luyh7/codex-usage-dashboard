//! Optional Git enrichment; failures retain the original cwd grouping.
use crate::parser::{SessionDetail, folder_name};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use wait_timeout::ChildExt;

/// Bounded command used only by background discovery, never by an HTTP query.
pub fn command(program: &str, args: &[&str]) -> String {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        use std::io::Read;
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
    let bytes = reader.join().unwrap_or_default();
    if success {
        String::from_utf8_lossy(&bytes).trim().to_owned()
    } else {
        String::new()
    }
}
pub fn windows_path(value: &str) -> Option<PathBuf> {
    let value = value.trim().trim_matches('"').replace('\r', "");
    let bytes = value.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !b"/\\".contains(&bytes[2])
    {
        return None;
    }
    Some(PathBuf::from(format!(
        "/mnt/{}/{}",
        (bytes[0] as char).to_ascii_lowercase(),
        value[3..].replace('\\', "/").trim_matches('/')
    )))
}
pub fn expand_home(value: &str) -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default();
    if value == "~" {
        home
    } else if let Some(suffix) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        home.join(suffix)
    } else {
        value.into()
    }
}
/// Match pathlib's public Windows path spelling while retaining canonical
/// handles/paths internally (including extended-length paths).
fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        if let Some(local) = text.strip_prefix(r"\\?\") {
            return local.into();
        }
    }
    text.into_owned()
}
fn path_from_cwd(value: &str) -> Option<PathBuf> {
    let value = value.trim().trim_matches('"');
    if value.is_empty() {
        return None;
    }
    let path = if !cfg!(windows) {
        windows_path(value).unwrap_or_else(|| expand_home(value))
    } else {
        expand_home(value)
    };
    path.exists().then_some(path)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub project: String,
    pub project_root: String,
    pub workspace_root: String,
    pub project_branch: String,
    pub is_git_worktree: bool,
}
impl Project {
    pub fn apply(&self, detail: &mut SessionDetail) {
        detail.project = self.project.clone();
        detail.project_root = self.project_root.clone();
        detail.workspace_root = self.workspace_root.clone();
        detail.project_branch = self.project_branch.clone();
        detail.is_git_worktree = self.is_git_worktree;
    }
}
fn git(path: &Path, args: &[&str]) -> String {
    let text = path.to_string_lossy();
    let mut argv = vec!["-C", &text];
    argv.extend_from_slice(args);
    command("git", &argv)
}
pub fn resolve(cwd: &str) -> Project {
    let mut info = Project {
        project: if folder_name(cwd).is_empty() {
            "unknown".into()
        } else {
            folder_name(cwd)
        },
        project_root: cwd.trim().into(),
        workspace_root: cwd.trim().into(),
        project_branch: String::new(),
        is_git_worktree: false,
    };
    let Some(path) = path_from_cwd(cwd) else {
        return info;
    };
    let root = git(&path, &["rev-parse", "--show-toplevel"]);
    if root.is_empty() {
        if let Ok(path) = path.canonicalize() {
            info.project_root = display_path(&path);
            info.workspace_root = info.project_root.clone();
            info.project = folder_name(&info.project_root);
        }
        return info;
    }
    let workspace = PathBuf::from(root.lines().last().unwrap());
    let workspace = workspace.canonicalize().unwrap_or(workspace);
    info.workspace_root = display_path(&workspace);
    info.project_root = info.workspace_root.clone();
    info.project_branch = git(&workspace, &["branch", "--show-current"])
        .lines()
        .last()
        .unwrap_or("")
        .into();
    if !git(
        &workspace,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .is_empty()
    {
        let output = git(&workspace, &["worktree", "list", "--porcelain"]);
        let mut main = None;
        for entry in output.split("\n\n") {
            let mut worktree = None;
            let mut bare = false;
            let mut branch = "";
            for line in entry.lines() {
                if let Some(path) = line.strip_prefix("worktree ") {
                    let path = expand_home(path.trim());
                    worktree = Some(path.canonicalize().unwrap_or(path));
                }
                if line == "bare" {
                    bare = true;
                }
                if let Some(value) = line.strip_prefix("branch ") {
                    branch = value
                        .trim()
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value.trim());
                }
            }
            if let Some(path) = worktree {
                if path == workspace && !branch.is_empty() {
                    info.project_branch = branch.into();
                }
                if main.is_none() && !bare {
                    main = Some(path);
                }
            }
        }
        if let Some(main) = main {
            info.is_git_worktree = workspace != main;
            info.project_root = display_path(&main);
        }
    }
    info.project = folder_name(&info.project_root);
    info
}
/// One resolution per distinct cwd during a publication. No process-global cache
/// survives a scope change, and logs are never used as command arguments/options.
#[derive(Default)]
pub struct Resolver(Mutex<HashMap<String, Project>>);
impl Resolver {
    pub fn get(&self, cwd: &str) -> Project {
        let mut cache = self.0.lock().unwrap();
        cache
            .entry(cwd.into())
            .or_insert_with(|| resolve(cwd))
            .clone()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linked_workspace_keeps_main_project_and_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("project with spaces");
        let branch = tmp.path().join("worktree");
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-b", "main"]);
        git(
            &main,
            &[
                "-c",
                "user.name=Synthetic",
                "-c",
                "user.email=synthetic@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "fixture",
            ],
        );
        git(
            &main,
            &[
                "worktree",
                "add",
                "-b",
                "feature/test",
                branch.to_str().unwrap(),
            ],
        );
        let result = resolve(branch.to_str().unwrap());
        assert_eq!(result.project, "project with spaces");
        assert_eq!(
            Path::new(&result.project_root).canonicalize().unwrap(),
            main.canonicalize().unwrap()
        );
        assert_eq!(
            Path::new(&result.workspace_root).canonicalize().unwrap(),
            branch.canonicalize().unwrap()
        );
        assert_eq!(result.project_branch, "feature/test");
        assert!(result.is_git_worktree);
        let missing = resolve("Z:\\missing\\folder");
        assert_eq!(missing.project, "folder");
        assert!(!missing.is_git_worktree);
    }
}
