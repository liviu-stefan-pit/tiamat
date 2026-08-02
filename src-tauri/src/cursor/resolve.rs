use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use super::types::{CANDIDATE_NAMES, ENV_EXECUTABLE_KEYS};

/// Resolve Cursor CLI executable.
///
/// Order: user-configured path → env overrides → PATH → known install locations.
pub fn resolve_cursor_executable(
    environ: &HashMap<String, String>,
    which: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    resolve_cursor_executable_with_configured(None, environ, which)
}

pub fn resolve_cursor_executable_with_configured(
    configured: Option<&str>,
    environ: &HashMap<String, String>,
    which: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    if let Some(path) = configured.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(path.to_string());
    }

    for key in ENV_EXECUTABLE_KEYS {
        if let Some(raw) = environ.get(*key) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    for name in CANDIDATE_NAMES {
        if let Some(found) = which(name) {
            return Some(found);
        }
    }

    known_install_paths(environ).into_iter().next()
}

pub fn resolve_from_process_env() -> Option<String> {
    let environ = env_map();
    resolve_cursor_executable(&environ, &default_which)
}

pub fn resolve_from_configured_and_env(configured: Option<&str>) -> Option<String> {
    let environ = env_map();
    resolve_cursor_executable_with_configured(configured, &environ, &default_which)
}

pub fn env_map() -> HashMap<String, String> {
    env::vars().collect()
}

pub fn default_which(name: &str) -> Option<String> {
    which_on_path(name, &env::var_os("PATH").unwrap_or_default())
}

fn which_on_path(name: &str, path_value: &std::ffi::OsStr) -> Option<String> {
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat", ".ps1"]
    } else {
        &[""]
    };

    for dir in env::split_paths(path_value) {
        for ext in extensions {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn known_install_paths(environ: &HashMap<String, String>) -> Vec<String> {
    let mut found = Vec::new();
    let relatives: &[(&str, PathBuf)] = &[
        (
            "LOCALAPPDATA",
            PathBuf::from("cursor-agent").join("agent.cmd"),
        ),
        (
            "LOCALAPPDATA",
            PathBuf::from("cursor-agent").join("agent.exe"),
        ),
        ("HOME", PathBuf::from(".local").join("bin").join("agent")),
        (
            "HOME",
            PathBuf::from(".local").join("bin").join("cursor-agent"),
        ),
        (
            "USERPROFILE",
            PathBuf::from(".local").join("bin").join("agent"),
        ),
    ];

    for (env_key, relative) in relatives {
        let Some(root) = environ
            .get(*env_key)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let candidate = Path::new(root).join(relative);
        if candidate.is_file() {
            found.push(candidate.to_string_lossy().to_string());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_configured_path_over_env() {
        let mut env = HashMap::new();
        env.insert(
            "TIAMAT_CURSOR_CLI".into(),
            "C:\\tools\\fake-agent.cmd".into(),
        );
        let resolved = resolve_cursor_executable_with_configured(
            Some("C:\\configured\\agent.cmd"),
            &env,
            &|_| Some("ignored".into()),
        );
        assert_eq!(resolved.as_deref(), Some("C:\\configured\\agent.cmd"));
    }

    #[test]
    fn prefers_tiamat_env_override() {
        let mut env = HashMap::new();
        env.insert(
            "TIAMAT_CURSOR_CLI".into(),
            "C:\\tools\\fake-agent.cmd".into(),
        );
        env.insert("CURSOR_CLI_PATH".into(), "C:\\other\\agent.exe".into());
        let resolved = resolve_cursor_executable(&env, &|_| Some("ignored".into()));
        assert_eq!(resolved.as_deref(), Some("C:\\tools\\fake-agent.cmd"));
    }

    #[test]
    fn falls_back_to_path_lookup() {
        let env = HashMap::new();
        let resolved = resolve_cursor_executable(&env, &|name| {
            if name == "agent" {
                Some("/usr/bin/agent".into())
            } else {
                None
            }
        });
        assert_eq!(resolved.as_deref(), Some("/usr/bin/agent"));
    }

    #[test]
    fn prefers_agent_before_cursor_agent() {
        let env = HashMap::new();
        let resolved = resolve_cursor_executable(&env, &|name| match name {
            "agent" => Some("/bin/agent".into()),
            "cursor-agent" => Some("/bin/cursor-agent".into()),
            _ => None,
        });
        assert_eq!(resolved.as_deref(), Some("/bin/agent"));
    }
}
