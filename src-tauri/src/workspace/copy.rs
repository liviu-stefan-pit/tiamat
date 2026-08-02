use std::fs;
use std::path::{Path, PathBuf};

use crate::intake::{is_ignored_dir_name, is_ignored_file_name, is_path_within_root};
use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::git::{configure_identity, git, git_text};
use crate::workspace::types::ManagedProjectKind;

pub struct CopyResult {
    pub managed_root: PathBuf,
    pub baseline_commit: String,
    pub baseline_branch: String,
    pub kind: ManagedProjectKind,
}

/// Guarded recursive copy that skips heavy/ignored directories and does not follow escape symlinks.
/// When `source` is a single file, creates `dest` as a directory and copies the file into it.
pub fn guarded_copy(source: &Path, dest: &Path, approved_roots: &[PathBuf]) -> WorkspaceResult<()> {
    if dest.exists() {
        return Err(WorkspaceError::Message(format!(
            "copy destination exists: {}",
            dest.display()
        )));
    }
    let source_meta = fs::symlink_metadata(source)?;
    if source_meta.file_type().is_symlink() {
        return Err(WorkspaceError::Message(format!(
            "refusing to copy symlink root: {}",
            source.display()
        )));
    }
    if source_meta.is_file() {
        fs::create_dir_all(dest)?;
        let name = source
            .file_name()
            .ok_or_else(|| WorkspaceError::Message("source file has no name".into()))?;
        fs::copy(source, dest.join(name))?;
        return Ok(());
    }
    if !source_meta.is_dir() {
        return Err(WorkspaceError::Message(format!(
            "unsupported source type: {}",
            source.display()
        )));
    }
    fs::create_dir_all(dest)?;
    copy_dir(source, source, dest, approved_roots)?;
    Ok(())
}

fn copy_dir(
    source_root: &Path,
    dir: &Path,
    dest_root: &Path,
    approved_roots: &[PathBuf],
) -> WorkspaceResult<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = fs::symlink_metadata(&path)?;

        if meta.file_type().is_symlink() {
            // Do not follow; skip reparse points that escape approved roots.
            if let Ok(target) = fs::canonicalize(&path) {
                let ok = approved_roots
                    .iter()
                    .any(|root| is_path_within_root(root, &target))
                    || is_path_within_root(source_root, &target);
                if !ok {
                    continue;
                }
            }
            // Skip symlinks entirely for non-git copies in v1 (safe default).
            continue;
        }

        let rel = path
            .strip_prefix(source_root)
            .map_err(|_| WorkspaceError::PathEscape(path.display().to_string()))?;
        let dest_path = dest_root.join(rel);

        if meta.is_dir() {
            if name == ".git" || is_ignored_dir_name(&name) {
                continue;
            }
            fs::create_dir_all(&dest_path)?;
            copy_dir(source_root, &path, dest_root, approved_roots)?;
            continue;
        }

        if is_ignored_file_name(&name) {
            continue;
        }
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&path, &dest_path)?;
    }
    Ok(())
}

/// Copy non-git input, init a local git repo, and create a baseline commit.
pub fn materialize_non_git_project(
    source: &Path,
    dest: &Path,
    project_id: &str,
    approved_roots: &[PathBuf],
    as_notes: bool,
) -> WorkspaceResult<CopyResult> {
    guarded_copy(source, dest, approved_roots)?;
    git(dest, &["init"])?;
    configure_identity(dest)?;
    git(dest, &["add", "-A"])?;
    let baseline_branch = if as_notes {
        format!("tiamat/notes-{project_id}")
    } else {
        format!("tiamat/intake-{project_id}")
    };
    git(dest, &["checkout", "-B", &baseline_branch])?;
    git(
        dest,
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("tiamat intake baseline for {project_id}"),
        ],
    )?;
    let baseline_commit = git_text(dest, &["rev-parse", "HEAD"])?;
    Ok(CopyResult {
        managed_root: dest.to_path_buf(),
        baseline_commit,
        baseline_branch,
        kind: if as_notes {
            ManagedProjectKind::NotesSnapshot
        } else {
            ManagedProjectKind::NonGitCopy
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn guarded_copy_copies_single_file_into_dest_dir() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("Master Plan.md");
        fs::write(&src, "# plan\n").unwrap();
        let dest = dir.path().join("notes-dest");
        guarded_copy(&src, &dest, std::slice::from_ref(&src)).unwrap();
        assert!(dest.is_dir());
        assert_eq!(
            fs::read_to_string(dest.join("Master Plan.md")).unwrap(),
            "# plan\n"
        );
    }

    #[test]
    fn guarded_copy_skips_node_modules() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("node_modules")).unwrap();
        fs::write(src.join("app.js"), "ok").unwrap();
        fs::write(src.join("node_modules").join("x.js"), "skip").unwrap();
        let dest = dir.path().join("dest");
        guarded_copy(&src, &dest, std::slice::from_ref(&src)).unwrap();
        assert!(dest.join("app.js").exists());
        assert!(!dest.join("node_modules").exists());
    }

    #[test]
    fn guarded_copy_skips_nested_managed_run_dirs() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        let nested_run = src.join("run-26e49c28-c9e7-4b29-b5d0-2523eac6e44c");
        fs::create_dir_all(nested_run.join("notes")).unwrap();
        fs::write(src.join("plan.md"), "notes").unwrap();
        fs::write(nested_run.join("notes").join("x.md"), "nested").unwrap();
        let dest = dir.path().join("dest");
        guarded_copy(&src, &dest, std::slice::from_ref(&src)).unwrap();
        assert!(dest.join("plan.md").exists());
        assert!(!dest
            .join("run-26e49c28-c9e7-4b29-b5d0-2523eac6e44c")
            .exists());
    }
}
