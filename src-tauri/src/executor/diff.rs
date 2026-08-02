use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::executor::error::{ExecutorError, ExecutorResult};
use crate::workspace::git_text;
use crate::workspace::roots::is_within_managed;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffBoundaryReport {
    pub changed_files: Vec<String>,
    pub escaped_paths: Vec<String>,
    pub ok: bool,
}

/// List changed/untracked files relative to HEAD via porcelain status.
pub fn collect_changed_files(repo_root: &Path) -> ExecutorResult<Vec<String>> {
    let porcelain = git_text(repo_root, &["status", "--porcelain"]).unwrap_or_default();
    let mut files = Vec::new();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let path_part = line[3..].trim();
        let path = if let Some((_, right)) = path_part.split_once(" -> ") {
            right.trim()
        } else {
            path_part
        };
        if !path.is_empty() {
            files.push(path.replace('/', "\\"));
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// Snapshot relative paths under a root (non-recursive depth-limited walk for escape proofs).
pub fn snapshot_paths(root: &Path, max_depth: usize) -> HashSet<String> {
    let mut out = HashSet::new();
    walk(root, root, 0, max_depth, &mut out);
    out
}

fn walk(root: &Path, current: &Path, depth: usize, max_depth: usize, out: &mut HashSet<String>) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('/', "\\"))
            .unwrap_or_else(|_| path.to_string_lossy().replace('/', "\\"));
        out.insert(rel.to_ascii_lowercase());
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if name == ".git" || name == "node_modules" || name == "quarantine" {
                continue;
            }
            walk(root, &path, depth + 1, max_depth, out);
        }
    }
}

/// Ensure every changed path resolves inside at least one approved write root.
pub fn validate_diff_boundaries(
    repo_root: &Path,
    write_roots: &[String],
    changed_files: &[String],
) -> ExecutorResult<DiffBoundaryReport> {
    let mut escaped = Vec::new();
    for rel in changed_files {
        let abs = if Path::new(rel).is_absolute() {
            PathBuf::from(rel)
        } else {
            repo_root.join(rel)
        };
        let abs_s = abs.to_string_lossy().to_string();
        let ok = write_roots
            .iter()
            .any(|root| is_within_managed(root, &abs_s) || is_within_managed(root, rel));
        let under_repo = is_within_managed(&repo_root.display().to_string(), &abs_s);
        let repo_is_write = write_roots
            .iter()
            .any(|r| paths_equal(r, &repo_root.display().to_string()));
        if !(ok || (under_repo && repo_is_write)) {
            escaped.push(abs_s);
        }
    }
    Ok(DiffBoundaryReport {
        changed_files: changed_files.to_vec(),
        escaped_paths: escaped.clone(),
        ok: escaped.is_empty(),
    })
}

/// Detect new files under the managed run root that fall outside approved write roots.
pub fn find_new_escapes(
    managed_run_root: &Path,
    write_roots: &[String],
    before: &HashSet<String>,
    after: &HashSet<String>,
) -> Vec<String> {
    let mut escaped = Vec::new();
    for rel in after.difference(before) {
        let abs = managed_run_root.join(rel);
        let abs_s = abs.to_string_lossy().to_string();
        // Ignore control/.tiamat and quarantine itself.
        let lower = rel.to_ascii_lowercase();
        if lower.starts_with("control\\")
            || lower.starts_with("quarantine\\")
            || lower.starts_with("fingerprints\\")
            || lower == "manifest.json"
        {
            continue;
        }
        let ok = write_roots
            .iter()
            .any(|root| is_within_managed(root, &abs_s));
        if !ok && abs.is_file() {
            escaped.push(abs_s);
        }
    }
    escaped.sort();
    escaped
}

fn paths_equal(a: &str, b: &str) -> bool {
    let na = a.replace('/', "\\").to_ascii_lowercase();
    let nb = b.replace('/', "\\").to_ascii_lowercase();
    na.trim_end_matches('\\') == nb.trim_end_matches('\\')
}

#[allow(dead_code)]
pub fn assert_no_escape(report: &DiffBoundaryReport) -> ExecutorResult<()> {
    if report.ok {
        Ok(())
    } else {
        Err(ExecutorError::BoundaryEscape(format!(
            "paths outside write roots: {}",
            report.escaped_paths.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_escape_outside_write_root() {
        let root = Path::new(r"C:\managed\run\projects\app");
        let report = validate_diff_boundaries(
            root,
            &[r"C:\managed\run\projects\app".into()],
            &[
                r"src\ok.ts".into(),
                r"C:\managed\run\projects\other\evil.ts".into(),
            ],
        )
        .unwrap();
        assert!(!report.ok);
        assert_eq!(report.escaped_paths.len(), 1);
    }

    #[test]
    fn accepts_in_root_relative_paths() {
        let root = Path::new(r"C:\managed\run\projects\app");
        let report = validate_diff_boundaries(
            root,
            &[r"C:\managed\run\projects\app".into()],
            &[r"src\feature.ts".into()],
        )
        .unwrap();
        assert!(report.ok);
    }
}
