use std::fs;
use std::path::{Path, PathBuf};

use crate::packaging::types::{PackagingError, PackagingResult};

pub fn long_path_prefix() -> &'static str {
    r"\\?\"
}

pub fn is_extended_path(path: &str) -> bool {
    path.starts_with(r"\\?\") || path.starts_with(r"\\?\UNC\")
}

pub fn normalize_windows_path(path: &str) -> String {
    if cfg!(windows) && path.len() >= 240 && !is_extended_path(path) {
        format!("{}{}", long_path_prefix(), path)
    } else {
        path.to_string()
    }
}

/// Build a deterministic long-path leaf (>260 chars) under `root`.
pub fn create_long_path_fixture(root: &Path) -> PackagingResult<PathBuf> {
    fs::create_dir_all(root)?;
    let mut current = root.to_path_buf();
    // Nested segments keep individual names short while total path exceeds MAX_PATH.
    for i in 0..20 {
        current = current.join(format!("segment-{i:02}-abcdefghijklmnopqrstuvwxyz"));
        fs::create_dir_all(&current)?;
    }
    let marker = current.join("long-path-marker.txt");
    fs::write(&marker, b"tiamat-long-path-ok\n")?;
    let display = marker.to_string_lossy().to_string();
    if display.len() < 240 {
        return Err(PackagingError::Message(format!(
            "expected long path >= 240 chars, got {} ({display})",
            display.len()
        )));
    }
    Ok(marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn extended_prefix_detected() {
        assert!(is_extended_path(r"\\?\C:\very\long"));
        assert!(!is_extended_path(r"C:\short"));
    }

    #[test]
    fn creates_long_path_fixture() {
        let dir = tempdir().unwrap();
        let marker = create_long_path_fixture(&dir.path().join("long-path")).unwrap();
        assert!(marker.exists());
        assert!(marker.to_string_lossy().len() >= 240);
    }
}
