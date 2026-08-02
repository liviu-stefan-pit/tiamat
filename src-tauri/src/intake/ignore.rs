use std::collections::HashSet;
use std::path::Component;
use std::sync::OnceLock;

fn default_ignored_dir_names() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        HashSet::from([
            "node_modules",
            "target",
            "dist",
            "build",
            ".cache",
            ".next",
            ".nuxt",
            ".turbo",
            ".venv",
            "venv",
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            ".git",
            ".hg",
            ".svn",
            ".tiamat",
            "coverage",
            ".idea",
            ".vs",
            "bin",
            "obj",
        ])
    })
}

fn default_ignored_file_names() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        HashSet::from([
            ".ds_store",
            "thumbs.db",
            "desktop.ini",
            "npm-debug.log",
            "yarn-error.log",
        ])
    })
}

/// Returns true when a directory name should be skipped during inventory walks.
/// `.git` is ignored for file inventory but still used by repo detection.
pub fn is_ignored_dir_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    default_ignored_dir_names().contains(lower.as_str())
}

pub fn is_ignored_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    default_ignored_file_names().contains(lower.as_str())
}

/// True when any path component is an ignored directory name.
#[allow(dead_code)]
pub fn path_has_ignored_component(path: &std::path::Path) -> bool {
    path.components().any(|c| match c {
        Component::Normal(os) => os.to_str().map(is_ignored_dir_name).unwrap_or(false),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn ignores_node_modules_and_target() {
        assert!(is_ignored_dir_name("node_modules"));
        assert!(is_ignored_dir_name("TARGET"));
        assert!(!is_ignored_dir_name("src"));
    }

    #[test]
    fn detects_ignored_components() {
        assert!(path_has_ignored_component(Path::new(
            "app/node_modules/left-pad/index.js"
        )));
        assert!(!path_has_ignored_component(Path::new("app/src/index.js")));
    }
}
