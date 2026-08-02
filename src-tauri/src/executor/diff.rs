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

/// Normalize a relative path for snapshot set membership / rejoining.
/// Always uses `/` separators. Case-folds only on Windows so Linux stays
/// case-sensitive (required for escape proofs like `ESCAPE_PROOF.txt`).
fn normalize_rel_key(rel: &str) -> String {
    let unified = rel.replace('\\', "/");
    if cfg!(windows) {
        unified.to_ascii_lowercase()
    } else {
        unified
    }
}

fn rel_key(root: &Path, path: &Path) -> String {
    let rel = path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned());
    normalize_rel_key(&rel)
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
            // Keep forward slashes so Path::join works on Unix and Windows.
            files.push(path.replace('\\', "/"));
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
        out.insert(rel_key(root, &path));
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
            // Forward-slash relatives join correctly on both platforms.
            repo_root.join(rel.replace('\\', "/"))
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
        // Keys use `/`; Path::join accepts that on Windows and Unix.
        let abs = managed_run_root.join(rel.replace('\\', "/"));
        let abs_s = abs.to_string_lossy().to_string();
        // Ignore control/.tiamat and quarantine itself, under either separator.
        let normalized = normalize_rel_key(rel);
        if normalized.starts_with("control/")
            || normalized.starts_with("quarantine/")
            || normalized.starts_with("fingerprints/")
            || normalized == "manifest.json"
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
    // Case folding is correct on Windows only; on Unix it would equate distinct paths.
    let norm = |s: &str| {
        let unified = s.replace('\\', "/");
        let folded = if cfg!(windows) {
            unified.to_ascii_lowercase()
        } else {
            unified
        };
        folded.trim_end_matches('/').to_string()
    };
    norm(a) == norm(b)
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

    #[cfg(windows)]
    mod fixture {
        pub const APP: &str = r"C:\managed\run\projects\app";
        pub const OUTSIDE_FILE: &str = r"C:\managed\run\projects\other\evil.ts";
        pub const REL_OK: &str = r"src\ok.ts";
        pub const REL_FEATURE: &str = r"src\feature.ts";
    }
    #[cfg(unix)]
    mod fixture {
        pub const APP: &str = "/managed/run/projects/app";
        pub const OUTSIDE_FILE: &str = "/managed/run/projects/other/evil.ts";
        pub const REL_OK: &str = "src/ok.ts";
        pub const REL_FEATURE: &str = "src/feature.ts";
    }

    #[test]
    fn detects_escape_outside_write_root() {
        let root = Path::new(fixture::APP);
        let report = validate_diff_boundaries(
            root,
            &[fixture::APP.into()],
            &[fixture::REL_OK.into(), fixture::OUTSIDE_FILE.into()],
        )
        .unwrap();
        assert!(!report.ok);
        assert_eq!(report.escaped_paths.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn detects_case_only_escape_on_unix() {
        let root = Path::new(fixture::APP);
        let report = validate_diff_boundaries(
            root,
            &[fixture::APP.into()],
            &["/managed/run/projects/APP/evil.ts".into()],
        )
        .unwrap();
        assert!(!report.ok, "case-only difference must not pass containment");
    }

    #[test]
    fn accepts_in_root_relative_paths() {
        let root = Path::new(fixture::APP);
        let report =
            validate_diff_boundaries(root, &[fixture::APP.into()], &[fixture::REL_FEATURE.into()])
                .unwrap();
        assert!(report.ok);
    }

    #[test]
    fn find_new_escapes_detects_mixed_case_proof_file() {
        let dir = tempfile::tempdir().unwrap();
        let managed = dir.path();
        let write_root = managed.join("projects").join("app");
        std::fs::create_dir_all(&write_root).unwrap();
        std::fs::write(write_root.join("ok.ts"), "ok\n").unwrap();

        let before = snapshot_paths(managed, 6);
        // Mixed-case filename: lowercasing the snapshot key must not break detection.
        let escape = managed.join("ESCAPE_PROOF.txt");
        std::fs::write(&escape, "out of bounds\n").unwrap();
        let after = snapshot_paths(managed, 6);

        let escaped = find_new_escapes(
            managed,
            &[write_root.display().to_string()],
            &before,
            &after,
        );
        assert_eq!(escaped.len(), 1, "escaped={escaped:?}");
        assert!(
            escaped[0].ends_with("ESCAPE_PROOF.txt") || escaped[0].ends_with("escape_proof.txt"),
            "unexpected path {}",
            escaped[0]
        );
        // On Unix the on-disk name must be preserved exactly.
        #[cfg(unix)]
        assert!(escaped[0].ends_with("ESCAPE_PROOF.txt"));
    }

    #[test]
    fn find_new_escapes_ignores_files_inside_write_root() {
        let dir = tempfile::tempdir().unwrap();
        let managed = dir.path();
        let write_root = managed.join("projects").join("app");
        std::fs::create_dir_all(write_root.join("src")).unwrap();

        let before = snapshot_paths(managed, 6);
        std::fs::write(write_root.join("src").join("feature.ts"), "export {}\n").unwrap();
        let after = snapshot_paths(managed, 6);

        let escaped = find_new_escapes(
            managed,
            &[write_root.display().to_string()],
            &before,
            &after,
        );
        assert!(escaped.is_empty(), "escaped={escaped:?}");
    }
}
