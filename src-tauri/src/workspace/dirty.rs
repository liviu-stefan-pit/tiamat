use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::git::{git, git_text};
use crate::workspace::types::DirtyOverlayMetadata;

/// Read-only capture of a dirty Git working tree. Never writes to the source repo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirtySnapshot {
    pub source_head: String,
    pub source_branch: Option<String>,
    pub staged_patch: Vec<u8>,
    pub unstaged_patch: Vec<u8>,
    pub untracked_files: Vec<UntrackedFile>,
    pub status_porcelain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UntrackedFile {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}

pub fn capture_dirty_snapshot(source: &Path) -> WorkspaceResult<DirtySnapshot> {
    let source_head = git_text(source, &["rev-parse", "HEAD"])?;
    let source_branch = git_text(source, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();
    let status_porcelain = git_text(
        source,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )
    .unwrap_or_default();

    let staged_patch = git_bytes(source, &["diff", "--cached", "--binary"])?;
    let unstaged_patch = git_bytes(source, &["diff", "--binary"])?;

    let untracked_list =
        git_text(source, &["ls-files", "--others", "--exclude-standard"]).unwrap_or_default();
    let mut untracked_files = Vec::new();
    for line in untracked_list.lines() {
        let rel = line.trim();
        if rel.is_empty() {
            continue;
        }
        let path = source.join(rel);
        if path.is_file() {
            let bytes = fs::read(&path)?;
            untracked_files.push(UntrackedFile {
                relative_path: rel.replace('\\', "/"),
                bytes,
            });
        }
    }

    Ok(DirtySnapshot {
        source_head,
        source_branch,
        staged_patch,
        unstaged_patch,
        untracked_files,
        status_porcelain,
    })
}

fn git_bytes(cwd: &Path, args: &[&str]) -> WorkspaceResult<Vec<u8>> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|err| WorkspaceError::Git {
            command: args.join(" "),
            detail: err.to_string(),
        })?;
    if !output.status.success() {
        return Err(WorkspaceError::Git {
            command: args.join(" "),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(output.stdout)
}

/// Reconstruct staged/unstaged/untracked state inside an owned clone without touching the source.
pub fn apply_dirty_overlay(
    owned: &Path,
    source: &Path,
    snapshot: &DirtySnapshot,
) -> WorkspaceResult<DirtyOverlayMetadata> {
    // Ensure we are at the captured HEAD.
    git(owned, &["checkout", "--force", &snapshot.source_head])?;
    git(owned, &["reset", "--hard", &snapshot.source_head])?;

    let overlay_dir = owned.join(".tiamat").join("intake-overlay");
    fs::create_dir_all(&overlay_dir)?;

    let staged_path = overlay_dir.join("staged.patch");
    let unstaged_path = overlay_dir.join("unstaged.patch");
    fs::write(&staged_path, &snapshot.staged_patch)?;
    fs::write(&unstaged_path, &snapshot.unstaged_patch)?;
    fs::write(
        overlay_dir.join("status.porcelain"),
        snapshot.status_porcelain.as_bytes(),
    )?;
    fs::write(
        overlay_dir.join("snapshot.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "sourceHead": snapshot.source_head,
            "sourceBranch": snapshot.source_branch,
            "untrackedFiles": snapshot.untracked_files.iter().map(|f| &f.relative_path).collect::<Vec<_>>(),
            "hadStaged": !snapshot.staged_patch.is_empty(),
            "hadUnstaged": !snapshot.unstaged_patch.is_empty(),
            "hadUntracked": !snapshot.untracked_files.is_empty(),
        }))?,
    )?;

    // Rebuild the index to match the source staged state (read-only against source).
    reconstruct_index_from_source(source, owned, &staged_path)?;

    // Copy the source working tree (excluding .git) so unstaged + untracked match exactly.
    copy_worktree_except_git(source, owned)?;

    let meta = DirtyOverlayMetadata {
        source_head: snapshot.source_head.clone(),
        had_staged: !snapshot.staged_patch.is_empty(),
        had_unstaged: !snapshot.unstaged_patch.is_empty(),
        had_untracked: !snapshot.untracked_files.is_empty(),
        staged_patch_bytes: snapshot.staged_patch.len() as u64,
        unstaged_patch_bytes: snapshot.unstaged_patch.len() as u64,
        untracked_files: snapshot
            .untracked_files
            .iter()
            .map(|f| f.relative_path.clone())
            .collect(),
        overlay_artifact: overlay_dir.display().to_string(),
    };

    fs::write(
        overlay_dir.join("metadata.json"),
        serde_json::to_vec_pretty(&meta)?,
    )?;

    Ok(meta)
}

fn reconstruct_index_from_source(
    source: &Path,
    owned: &Path,
    staged_patch: &Path,
) -> WorkspaceResult<()> {
    if staged_patch.metadata()?.len() > 0 {
        let patch_str = staged_patch.to_string_lossy().to_string();
        if git(
            owned,
            &["apply", "--cached", "--whitespace=nowarn", &patch_str],
        )
        .is_ok()
        {
            return Ok(());
        }
    }

    // Fallback: materialize staged blobs from the source index (read-only).
    let staged_names = git_text(source, &["diff", "--cached", "--name-only"]).unwrap_or_default();
    let names: Vec<String> = if staged_names.is_empty() {
        snapshot_staged_paths_from_status(source)?
    } else {
        staged_names
            .lines()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.replace('\\', "/"))
            .collect()
    };

    for rel in names {
        let output = std::process::Command::new("git")
            .args(["show", &format!(":{rel}")])
            .current_dir(source)
            .output()
            .map_err(|err| WorkspaceError::Git {
                command: format!("show :{rel}"),
                detail: err.to_string(),
            })?;
        if !output.status.success() {
            continue;
        }
        let dest = owned.join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &output.stdout)?;
        git(owned, &["add", "--", &rel])?;
    }
    Ok(())
}

fn snapshot_staged_paths_from_status(source: &Path) -> WorkspaceResult<Vec<String>> {
    let porcelain = git_text(source, &["status", "--porcelain=v1"]).unwrap_or_default();
    let mut out = Vec::new();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let x = line.as_bytes()[0];
        if x != b' ' && x != b'?' {
            let path = line[3..].trim();
            if let Some((left, _)) = path.split_once(" -> ") {
                out.push(left.replace('\\', "/"));
            } else {
                out.push(path.replace('\\', "/"));
            }
        }
    }
    Ok(out)
}

fn copy_worktree_except_git(source: &Path, owned: &Path) -> WorkspaceResult<()> {
    fn walk(src_root: &Path, src: &Path, dst_root: &Path) -> WorkspaceResult<()> {
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".git" {
                continue;
            }
            let meta = fs::symlink_metadata(&path)?;
            let rel = path
                .strip_prefix(src_root)
                .map_err(|_| WorkspaceError::PathEscape(path.display().to_string()))?;
            let dest = dst_root.join(rel);
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                fs::create_dir_all(&dest)?;
                walk(src_root, &path, dst_root)?;
            } else {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&path, &dest)?;
            }
        }
        Ok(())
    }
    walk(source, source, owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::git::{configure_identity, git};
    use tempfile::tempdir;

    fn init_repo(path: &Path) {
        fs::create_dir_all(path).unwrap();
        git(path, &["init"]).unwrap();
        configure_identity(path).unwrap();
        fs::write(path.join("README.md"), "base\n").unwrap();
        git(path, &["add", "."]).unwrap();
        git(path, &["commit", "-m", "init"]).unwrap();
    }

    #[test]
    fn capture_detects_staged_unstaged_untracked() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("src");
        init_repo(&repo);
        fs::write(repo.join("README.md"), "staged\n").unwrap();
        git(&repo, &["add", "README.md"]).unwrap();
        fs::write(repo.join("README.md"), "unstaged\n").unwrap();
        fs::write(repo.join("new.txt"), "untracked\n").unwrap();

        let snap = capture_dirty_snapshot(&repo).unwrap();
        assert!(!snap.staged_patch.is_empty() || snap.status_porcelain.contains("README.md"));
        assert!(!snap.untracked_files.is_empty());
        assert!(snap
            .untracked_files
            .iter()
            .any(|f| f.relative_path == "new.txt"));
    }
}
