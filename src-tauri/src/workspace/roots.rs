use std::path::{Component, Path, PathBuf};

use crate::intake::{is_path_within_root, strip_verbatim_prefix};
use crate::workspace::error::{WorkspaceError, WorkspaceResult};

/// Case-insensitive containment for managed workspace roots.
pub fn is_within_managed(root: &str, candidate: &str) -> bool {
    is_path_within_root(Path::new(root), Path::new(candidate))
}

/// Reject path escapes, parent traversal, and absolute paths outside managed roots.
pub fn validate_relative_within(root: &Path, relative: &str) -> WorkspaceResult<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() {
        return Err(WorkspaceError::PathEscape(format!(
            "absolute path not allowed as relative write: {relative}"
        )));
    }
    for component in rel.components() {
        match component {
            Component::ParentDir => {
                return Err(WorkspaceError::PathEscape(format!(
                    "parent traversal rejected: {relative}"
                )));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(WorkspaceError::PathEscape(format!(
                    "rooted relative path rejected: {relative}"
                )));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    let joined = root.join(rel);
    let canonical_root = strip_verbatim_prefix(root.to_path_buf());
    if !is_path_within_root(&canonical_root, &joined) && !is_path_within_root(root, &joined) {
        // Best-effort before the target exists: normalize string compare.
        let root_s = normalize(root);
        let join_s = normalize(&joined);
        if !(join_s == root_s
            || join_s.starts_with(&(root_s.clone() + "\\"))
            || join_s.starts_with(&(root_s + "/")))
        {
            return Err(WorkspaceError::PathEscape(format!(
                "{} escapes {}",
                joined.display(),
                root.display()
            )));
        }
    }
    Ok(joined)
}

pub fn validate_write_roots(approved: &[String], requested: &[String]) -> WorkspaceResult<()> {
    for req in requested {
        let ok = approved.iter().any(|root| is_within_managed(root, req));
        if !ok {
            return Err(WorkspaceError::InvalidWriteRoot(req.clone()));
        }
    }
    Ok(())
}

pub fn validate_read_roots(approved: &[String], requested: &[String]) -> WorkspaceResult<()> {
    for req in requested {
        let ok = approved.iter().any(|root| is_within_managed(root, req));
        if !ok {
            return Err(WorkspaceError::InvalidReadRoot(req.clone()));
        }
    }
    Ok(())
}

fn normalize(path: &Path) -> String {
    let stripped = strip_verbatim_prefix(path.to_path_buf());
    stripped
        .to_string_lossy()
        .to_ascii_lowercase()
        .replace('/', "\\")
}

/// Stable lock name for a managed project write root.
pub fn lock_name_for(project_id: &str) -> String {
    format!("write:{project_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_traversal() {
        let root = Path::new(r"C:\managed\run\projects\app");
        assert!(validate_relative_within(root, r"..\other").is_err());
        assert!(validate_relative_within(root, r"src\main.rs").is_ok());
    }

    #[test]
    fn write_root_must_be_within_approved() {
        let approved = vec![
            r"C:\managed\run\projects\a".into(),
            r"C:\managed\run\projects\b".into(),
        ];
        assert!(
            validate_write_roots(&approved, &[r"C:\managed\run\projects\a\src".into()]).is_ok()
        );
        assert!(validate_write_roots(&approved, &[r"C:\source\app".into()]).is_err());
    }

    #[test]
    fn containment_is_case_insensitive() {
        assert!(is_within_managed(
            r"C:\Managed\Run\projects\App",
            r"c:\managed\run\projects\app\src"
        ));
    }
}
