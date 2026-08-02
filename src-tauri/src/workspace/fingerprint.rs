use std::fs;
use std::path::Path;
use std::process::Command;

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::intake::{is_ignored_dir_name, is_ignored_file_name};
use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::types::SourceFingerprint;

/// Capture a source fingerprint (git status + content tree hash) without mutating the source.
pub fn capture_fingerprint(path: &Path, kind: &str) -> WorkspaceResult<SourceFingerprint> {
    let mut head = None;
    let mut branch = None;
    let mut status_porcelain = String::new();

    if path.join(".git").exists() {
        head = git_output(path, &["rev-parse", "HEAD"]).ok();
        branch = git_output(path, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();
        status_porcelain = git_output(path, &["status", "--porcelain=v1", "--untracked-files=all"])
            .unwrap_or_default();
    }

    let status_hash = hash_bytes(status_porcelain.as_bytes());
    let tree_hash = hash_tree(path)?;

    Ok(SourceFingerprint {
        path: path.display().to_string(),
        kind: kind.to_string(),
        head,
        branch,
        status_porcelain,
        status_hash,
        tree_hash,
        captured_at_utc: Utc::now(),
    })
}

pub fn fingerprints_equal(a: &SourceFingerprint, b: &SourceFingerprint) -> bool {
    a.path == b.path
        && a.head == b.head
        && a.branch == b.branch
        && a.status_hash == b.status_hash
        && a.tree_hash == b.tree_hash
        && a.status_porcelain == b.status_porcelain
}

fn hash_tree(root: &Path) -> WorkspaceResult<String> {
    let mut entries: Vec<(String, String)> = Vec::new();
    walk_files(root, root, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel, file_hash) in entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update(file_hash.as_bytes());
        hasher.update(b"\n");
    }
    Ok(hex::encode(hasher.finalize()))
}

fn walk_files(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> WorkspaceResult<()> {
    let read = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() {
            // Record symlink target text only; do not follow.
            let rel = relative(root, &path);
            let target = fs::read_link(&path)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            out.push((format!("symlink:{rel}"), hash_bytes(target.as_bytes())));
            continue;
        }
        if meta.is_dir() {
            if name == ".git" || is_ignored_dir_name(&name) {
                continue;
            }
            walk_files(root, &path, out)?;
            continue;
        }
        if is_ignored_file_name(&name) {
            continue;
        }
        let rel = relative(root, &path);
        let bytes = fs::read(&path)?;
        out.push((rel, hash_bytes(&bytes)));
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.display().to_string())
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn write_fingerprint_file(path: &Path, fp: &SourceFingerprint) -> WorkspaceResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(fp)?;
    fs::write(path, json)?;
    Ok(())
}

fn git_output(cwd: &Path, args: &[&str]) -> WorkspaceResult<String> {
    let output = Command::new("git")
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
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn ensure_source_unchanged(
    before: &SourceFingerprint,
    after: &SourceFingerprint,
) -> WorkspaceResult<()> {
    if fingerprints_equal(before, after) {
        return Ok(());
    }
    Err(WorkspaceError::SourceMutated(format!(
        "source changed at {} (status {} → {}, tree {} → {})",
        before.path, before.status_hash, after.status_hash, before.tree_hash, after.tree_hash
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tree_hash_stable_for_same_content() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let a = hash_tree(dir.path()).unwrap();
        let b = hash_tree(dir.path()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn tree_hash_changes_when_file_changes() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let a = hash_tree(dir.path()).unwrap();
        fs::write(dir.path().join("a.txt"), "world").unwrap();
        let b = hash_tree(dir.path()).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn ensure_source_unchanged_blocks_mutation() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let before = capture_fingerprint(dir.path(), "folder").unwrap();
        fs::write(dir.path().join("a.txt"), "mutated").unwrap();
        let after = capture_fingerprint(dir.path(), "folder").unwrap();
        assert!(ensure_source_unchanged(&before, &after).is_err());
        assert!(ensure_source_unchanged(&before, &before).is_ok());
    }
}
