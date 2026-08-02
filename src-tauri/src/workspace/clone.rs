use std::fs;
use std::path::{Path, PathBuf};

use crate::workspace::dirty::{apply_dirty_overlay, capture_dirty_snapshot};
use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::git::{configure_identity, git, git_text};
use crate::workspace::types::{DirtyOverlayMetadata, ManagedProjectKind};

pub struct CloneResult {
    pub managed_root: PathBuf,
    pub baseline_commit: String,
    pub baseline_branch: String,
    pub worktree_path: Option<PathBuf>,
    pub dirty_overlay: Option<DirtyOverlayMetadata>,
    pub kind: ManagedProjectKind,
}

/// Create an owned `--no-hardlinks` clone. Never attaches a worktree to the source.
pub fn clone_owned_no_hardlinks(source: &Path, dest: &Path) -> WorkspaceResult<()> {
    if dest.exists() {
        return Err(WorkspaceError::Message(format!(
            "clone destination already exists: {}",
            dest.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let source_s = source.display().to_string();
    let dest_s = dest.display().to_string();

    // Prefer local no-hardlinks clone so object files are copied, not linked.
    let output = std::process::Command::new("git")
        .args(["clone", "--no-hardlinks", "--local", &source_s, &dest_s])
        .output()
        .map_err(|err| WorkspaceError::Git {
            command: "clone --no-hardlinks --local".into(),
            detail: err.to_string(),
        })?;
    if !output.status.success() {
        // Fallback without --local for some Windows path forms.
        let output2 = std::process::Command::new("git")
            .args(["clone", "--no-hardlinks", &source_s, &dest_s])
            .output()
            .map_err(|err| WorkspaceError::Git {
                command: "clone --no-hardlinks".into(),
                detail: err.to_string(),
            })?;
        if !output2.status.success() {
            return Err(WorkspaceError::Git {
                command: "clone --no-hardlinks".into(),
                detail: format!(
                    "local={} fallback={}",
                    String::from_utf8_lossy(&output.stderr).trim(),
                    String::from_utf8_lossy(&output2.stderr).trim()
                ),
            });
        }
    }
    configure_identity(dest)?;
    Ok(())
}

/// Clone, reconstruct dirty overlay if needed, create intake baseline + internal worktree.
pub fn materialize_git_project(
    source: &Path,
    dest: &Path,
    project_id: &str,
    create_worktree: bool,
) -> WorkspaceResult<CloneResult> {
    let is_dirty = {
        let status = git_text(source, &["status", "--porcelain"]).unwrap_or_default();
        !status.trim().is_empty()
    };

    let dirty_snapshot = if is_dirty {
        Some(capture_dirty_snapshot(source)?)
    } else {
        None
    };

    clone_owned_no_hardlinks(source, dest)?;

    let dirty_overlay = if let Some(snap) = dirty_snapshot.as_ref() {
        Some(apply_dirty_overlay(dest, source, snap)?)
    } else {
        None
    };

    // Create intake baseline commit in the owned clone only.
    let baseline_branch = format!("tiamat/intake-{project_id}");
    let _ = git(dest, &["checkout", "-B", &baseline_branch]);
    git(dest, &["add", "-A"])?;
    // Allow empty baseline when the clone is already clean and matches HEAD.
    let commit_result = git(
        dest,
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("tiamat intake baseline for {project_id}"),
        ],
    );
    if let Err(err) = commit_result {
        // If nothing to commit and allow-empty failed for some reason, still resolve HEAD.
        let _ = err;
        let _ = git(
            dest,
            &[
                "commit",
                "--allow-empty",
                "-m",
                &format!("tiamat intake baseline for {project_id}"),
            ],
        )?;
    }
    let baseline_commit = git_text(dest, &["rev-parse", "HEAD"])?;

    let worktree_path = if create_worktree {
        let wt = dest
            .parent()
            .unwrap_or(dest)
            .join(format!("{project_id}-worktree"));
        if wt.exists() {
            let _ = fs::remove_dir_all(&wt);
        }
        let branch = format!("tiamat/work/{project_id}");
        git(dest, &["branch", "-f", &branch, &baseline_commit])?;
        git(
            dest,
            &["worktree", "add", &wt.display().to_string(), &branch],
        )?;
        Some(wt)
    } else {
        None
    };

    Ok(CloneResult {
        managed_root: dest.to_path_buf(),
        baseline_commit,
        baseline_branch,
        worktree_path,
        dirty_overlay,
        kind: ManagedProjectKind::GitClone,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::git::{configure_identity, git, git_text};
    use tempfile::tempdir;

    #[test]
    fn clone_does_not_require_source_worktree() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        fs::create_dir_all(&source).unwrap();
        git(&source, &["init"]).unwrap();
        configure_identity(&source).unwrap();
        fs::write(source.join("a.txt"), "a\n").unwrap();
        git(&source, &["add", "."]).unwrap();
        git(&source, &["commit", "-m", "init"]).unwrap();

        let dest = dir.path().join("owned");
        clone_owned_no_hardlinks(&source, &dest).unwrap();
        assert!(dest.join(".git").exists());
        assert!(dest.join("a.txt").exists());
        // Source should remain a plain repo without worktrees attached by us.
        let wt = git_text(&source, &["worktree", "list"]).unwrap();
        assert_eq!(wt.lines().count(), 1);
    }
}
