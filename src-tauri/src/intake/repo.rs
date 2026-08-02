use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    pub is_git: bool,
    pub git_dir: Option<String>,
    pub branch: Option<String>,
    pub is_dirty: bool,
    pub has_submodules: bool,
    pub has_lfs: bool,
    pub nested_repos: Vec<String>,
    pub head: Option<String>,
    pub warnings: Vec<String>,
}

/// Detect git repository state, nested repos, submodules, and LFS hints.
pub fn inspect_repo(root: &Path) -> RepoState {
    let mut state = RepoState::default();
    let git_meta = root.join(".git");
    if !git_meta.exists() {
        return state;
    }

    state.is_git = true;
    state.git_dir = Some(git_meta.display().to_string());

    if root.join(".gitmodules").exists() {
        state.has_submodules = true;
        state
            .warnings
            .push("Repository declares submodules (.gitmodules).".into());
    }

    if let Ok(attrs) = fs::read_to_string(root.join(".gitattributes")) {
        if attrs.contains("filter=lfs") {
            state.has_lfs = true;
            state
                .warnings
                .push("Git LFS filters detected in .gitattributes.".into());
        }
    }

    if let Some(branch) = git_output(root, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        state.branch = Some(branch);
    }
    if let Some(head) = git_output(root, &["rev-parse", "HEAD"]) {
        state.head = Some(head);
    }
    if let Some(porcelain) = git_output(root, &["status", "--porcelain"]) {
        state.is_dirty = !porcelain.trim().is_empty();
        if state.is_dirty {
            state
                .warnings
                .push("Working tree has uncommitted changes.".into());
        }
    }

    state.nested_repos = find_nested_repos(root);
    if !state.nested_repos.is_empty() {
        state.warnings.push(format!(
            "Found {} nested git repositories.",
            state.nested_repos.len()
        ));
    }

    state
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn find_nested_repos(root: &Path) -> Vec<String> {
    let mut nested = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".git" || crate::intake::ignore::is_ignored_dir_name(&name) {
                continue;
            }
            if path.join(".git").exists() {
                if let Ok(rel) = path.strip_prefix(root) {
                    nested.push(rel.display().to_string());
                }
                // Do not descend into nested repo for further nesting in this pass.
                continue;
            }
            stack.push(path);
        }
    }
    nested.sort();
    nested
}

pub fn stable_project_id(root: &Path) -> String {
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let mut id = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            id.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            id.push('-');
        }
    }
    if id.is_empty() {
        "project".into()
    } else {
        id
    }
}

pub fn classify_kind(is_git: bool, has_code_signals: bool) -> tiamat_contracts::ProjectKind {
    if is_git {
        tiamat_contracts::ProjectKind::Git
    } else if has_code_signals {
        tiamat_contracts::ProjectKind::Folder
    } else {
        tiamat_contracts::ProjectKind::Notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn non_git_folder_has_empty_state() {
        let dir = tempdir().unwrap();
        let state = inspect_repo(dir.path());
        assert!(!state.is_git);
        assert!(state.nested_repos.is_empty());
    }

    #[test]
    fn stable_project_id_sanitizes() {
        #[cfg(windows)]
        let path = Path::new(r"C:\tmp\My App!");
        #[cfg(not(windows))]
        let path = Path::new("/tmp/My App!");
        assert_eq!(stable_project_id(path), "my-app");
    }
}
