use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::intake::error::{IntakeError, IntakeResult};

/// Reject unsupported Windows path forms before any filesystem access.
pub fn validate_raw_path(raw: &str) -> IntakeResult<()> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(IntakeError::UnsupportedPath("empty path".into()));
    }

    // Device namespace and extended UNC forms we do not support in v1.
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with(r"\\.\")
        || lower.starts_with(r"\\?\unc\")
        || lower.starts_with("//./")
        || lower.starts_with("//?/unc/")
    {
        return Err(IntakeError::UnsupportedPath(format!(
            "device/UNC namespace rejected: {trimmed}"
        )));
    }

    // Ordinary UNC shares are unsupported in v1 preflight.
    if (trimmed.starts_with(r"\\") || trimmed.starts_with("//"))
        && !trimmed.starts_with(r"\\?\")
        && !trimmed.starts_with("//?/")
    {
        return Err(IntakeError::UnsupportedPath(format!(
            "UNC path rejected: {trimmed}"
        )));
    }

    // A colon is legal in a Unix filename, so this check only applies to NTFS.
    if cfg!(windows) && has_alternate_data_stream(trimmed) {
        return Err(IntakeError::AlternateDataStream(trimmed.to_string()));
    }

    Ok(())
}

/// Detect NTFS alternate data stream syntax after the drive letter.
/// Examples: `C:\foo:bar`, `C:\foo:bar:$DATA`
fn has_alternate_data_stream(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    // Skip drive letter prefix like `C:`
    let start = if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        2
    } else {
        0
    };
    raw[start..].contains(':')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalPath {
    pub original: PathBuf,
    pub final_path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
}

/// Canonicalize with reparse-point resolution and basic volume identity checks.
pub fn canonicalize_path(raw: &str) -> IntakeResult<CanonicalPath> {
    validate_raw_path(raw)?;
    let original = PathBuf::from(raw.trim());
    if !original.exists() {
        return Err(IntakeError::NotFound(original));
    }

    let is_symlink = is_reparse_point(&original);
    let metadata = fs::symlink_metadata(&original)?;
    let is_dir = metadata.is_dir() || (metadata.file_type().is_symlink() && original.is_dir());

    let final_path = fs::canonicalize(&original).map_err(|err| {
        IntakeError::UnsupportedPath(format!("cannot canonicalize {}: {err}", original.display()))
    })?;

    let final_display = final_path.to_string_lossy();
    if final_display.starts_with(r"\\?\UNC\") || final_display.starts_with("//?/UNC/") {
        return Err(IntakeError::UnsupportedPath(format!(
            "canonical UNC rejected: {}",
            final_path.display()
        )));
    }

    Ok(CanonicalPath {
        original,
        final_path: strip_verbatim_prefix(final_path),
        is_dir,
        is_symlink,
    })
}

fn is_reparse_point(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Strip Windows `\\?\` verbatim prefix for stable display/comparisons.
pub fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix("UNC\\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        return PathBuf::from(rest);
    }
    if let Some(rest) = s.strip_prefix("//?/") {
        if let Some(unc) = rest.strip_prefix("UNC/") {
            return PathBuf::from(format!("//{unc}"));
        }
        return PathBuf::from(rest);
    }
    path
}

/// Path containment check. Case-insensitive on Windows, case-sensitive on Unix:
/// folding case on a case-sensitive filesystem would make `/managed/APP/evil`
/// look like it lives inside `/managed/app`, defeating the write-root boundary.
pub fn is_path_within_root(root: &Path, candidate: &Path) -> bool {
    let root_c = normalize_for_compare(root);
    let cand_c = normalize_for_compare(candidate);
    if root_c.is_empty() {
        return false;
    }
    if cand_c == root_c {
        return true;
    }
    cand_c.starts_with(&format!("{root_c}\\")) || cand_c.starts_with(&format!("{root_c}/"))
}

fn normalize_for_compare(path: &Path) -> String {
    let stripped = strip_verbatim_prefix(path.to_path_buf());
    let mut out = String::new();
    for component in stripped.components() {
        match component {
            Component::Prefix(p) => {
                out.push_str(&fold_case(&p.as_os_str().to_string_lossy()));
            }
            Component::RootDir => {
                if !out.ends_with('\\') && !out.ends_with('/') {
                    out.push('\\');
                }
            }
            Component::Normal(os) => {
                if !out.ends_with('\\') && !out.ends_with('/') && !out.is_empty() {
                    out.push('\\');
                }
                out.push_str(&fold_case(&os.to_string_lossy()));
            }
            Component::CurDir | Component::ParentDir => {}
        }
    }
    out
}

/// Fold case only where the filesystem does.
pub(crate) fn fold_case(value: &str) -> String {
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}

/// Ensure a resolved path remains inside an approved root; otherwise reject as escape.
pub fn assert_within_approved_roots(
    approved_roots: &[PathBuf],
    candidate: &Path,
) -> IntakeResult<()> {
    if approved_roots
        .iter()
        .any(|root| is_path_within_root(root, candidate))
    {
        return Ok(());
    }
    Err(IntakeError::PathEscape(format!(
        "{} escapes approved intake roots",
        candidate.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unc_and_device_paths() {
        assert!(validate_raw_path(r"\\server\share\proj").is_err());
        assert!(validate_raw_path(r"\\.\pipe\foo").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_alternate_data_streams() {
        assert!(validate_raw_path(r"C:\temp\file.txt:secret").is_err());
        assert!(validate_raw_path(r"C:\temp\file.txt").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn allows_colons_in_unix_filenames() {
        assert!(validate_raw_path("/tmp/build:2026-08-02.log").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn containment_is_case_insensitive_on_windows() {
        let root = Path::new(r"C:\Projects\App");
        assert!(is_path_within_root(root, Path::new(r"c:\projects\app\src")));
        assert!(!is_path_within_root(
            root,
            Path::new(r"C:\Projects\Other\src")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn containment_is_case_sensitive_on_unix() {
        let root = Path::new("/managed/run/projects/app");
        assert!(is_path_within_root(
            root,
            Path::new("/managed/run/projects/app/src")
        ));
        assert!(!is_path_within_root(
            root,
            Path::new("/managed/run/projects/other/src")
        ));
        // A case-only difference is a different directory on Unix and must not pass.
        assert!(!is_path_within_root(
            root,
            Path::new("/managed/run/projects/APP/evil.ts")
        ));
    }
}
