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
    let text = stripped.to_string_lossy();
    if cfg!(windows) {
        text.to_ascii_lowercase().replace('/', "\\")
    } else {
        text.into_owned()
    }
}

/// Stable lock name for a managed project write root.
pub fn lock_name_for(project_id: &str) -> String {
    format!("write:{project_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Path syntax is platform-specific, so the fixtures are too.
    #[cfg(windows)]
    mod fixture {
        pub const APP: &str = r"C:\managed\run\projects\app";
        pub const A: &str = r"C:\managed\run\projects\a";
        pub const B: &str = r"C:\managed\run\projects\b";
        pub const A_SRC: &str = r"C:\managed\run\projects\a\src";
        pub const OUTSIDE: &str = r"C:\source\app";
        pub const TRAVERSAL: &str = r"..\other";
        pub const NESTED: &str = r"src\main.rs";
    }
    #[cfg(unix)]
    mod fixture {
        pub const APP: &str = "/managed/run/projects/app";
        pub const A: &str = "/managed/run/projects/a";
        pub const B: &str = "/managed/run/projects/b";
        pub const A_SRC: &str = "/managed/run/projects/a/src";
        pub const OUTSIDE: &str = "/source/app";
        pub const TRAVERSAL: &str = "../other";
        pub const NESTED: &str = "src/main.rs";
    }

    #[test]
    fn rejects_parent_traversal() {
        let root = Path::new(fixture::APP);
        assert!(validate_relative_within(root, fixture::TRAVERSAL).is_err());
        assert!(validate_relative_within(root, fixture::NESTED).is_ok());
    }

    #[test]
    fn write_root_must_be_within_approved() {
        let approved = vec![fixture::A.to_string(), fixture::B.to_string()];
        assert!(validate_write_roots(&approved, &[fixture::A_SRC.into()]).is_ok());
        assert!(validate_write_roots(&approved, &[fixture::OUTSIDE.into()]).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn containment_is_case_insensitive_on_windows() {
        assert!(is_within_managed(
            r"C:\Managed\Run\projects\App",
            r"c:\managed\run\projects\app\src"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn containment_is_case_sensitive_on_unix() {
        assert!(is_within_managed(
            fixture::APP,
            "/managed/run/projects/app/src"
        ));
        assert!(!is_within_managed(
            fixture::APP,
            "/managed/run/projects/APP/src"
        ));
    }
}
