use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use super::types::{CANDIDATE_NAMES, ENV_EXECUTABLE_KEYS};

/// Result of expanding a Cursor Windows launcher (`.cmd`/`.ps1`) to Node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnwoundCursorRuntime {
    pub node_exe: String,
    pub index_js: String,
    /// Value for `CURSOR_INVOKED_AS` (launcher basename).
    pub invoked_as: String,
}

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

/// Strip lone `-` argv tokens that Windows PowerShell `-File` rejects
/// ([PowerShell#10510](https://github.com/PowerShell/PowerShell/issues/10510)).
/// Stdin is the prompt channel; `-` must never appear as a placeholder.
pub fn strip_lone_dash_argv(argv: &[String]) -> Vec<String> {
    argv.iter()
        .filter(|a| a.as_str() != "-")
        .cloned()
        .collect()
}

/// Expand a Cursor install launcher (`.cmd`/`.ps1` under `cursor-agent`) to
/// `node.exe` + `index.js`, bypassing PowerShell `-File` for hosted spawns.
///
/// Returns `None` when the path is not a recognized Windows launcher install
/// (fake fixtures, bare `agent` on PATH, non-Windows, etc.).
pub fn unwind_cursor_launcher(executable: &str) -> Option<UnwoundCursorRuntime> {
    let path = Path::new(executable.trim());
    if !path.is_file() {
        return None;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_launcher = name == "agent.cmd"
        || name == "cursor-agent.cmd"
        || name == "agent.ps1"
        || name == "cursor-agent.ps1";
    if !is_launcher {
        return None;
    }
    let script_dir = path.parent()?;
    // Same early path as cursor-agent.ps1: node.exe next to the launcher.
    let root_node = script_dir.join("node.exe");
    let root_index = script_dir.join("index.js");
    if root_node.is_file() && root_index.is_file() {
        return Some(UnwoundCursorRuntime {
            node_exe: root_node.to_string_lossy().to_string(),
            index_js: root_index.to_string_lossy().to_string(),
            invoked_as: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "agent.cmd".into()),
        });
    }
    let versions_dir = script_dir.join("versions");
    let version = latest_cursor_version_dir(&versions_dir)?;
    let node = versions_dir.join(&version).join("node.exe");
    let index = versions_dir.join(&version).join("index.js");
    if !node.is_file() || !index.is_file() {
        return None;
    }
    Some(UnwoundCursorRuntime {
        node_exe: node.to_string_lossy().to_string(),
        index_js: index.to_string_lossy().to_string(),
        invoked_as: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "agent.cmd".into()),
    })
}

/// Prepare argv + env for hosted Cursor spawns: unwind Windows launcher to Node
/// when possible, then drop lone `-` tokens.
///
/// Returns `(argv, extra_env)`. `extra_env` may include `CURSOR_INVOKED_AS`.
pub fn prepare_hosted_cursor_argv(argv: &[String]) -> (Vec<String>, Vec<(String, String)>) {
    if argv.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut env = Vec::new();
    let expanded = if let Some(runtime) = unwind_cursor_launcher(&argv[0]) {
        env.push(("CURSOR_INVOKED_AS".into(), runtime.invoked_as));
        let mut out = vec![runtime.node_exe, runtime.index_js];
        out.extend(argv.iter().skip(1).cloned());
        out
    } else {
        argv.to_vec()
    };
    (strip_lone_dash_argv(&expanded), env)
}

fn latest_cursor_version_dir(versions_dir: &Path) -> Option<String> {
    if !versions_dir.is_dir() {
        return None;
    }
    let mut scored: Vec<(i64, String)> = Vec::new();
    let entries = std::fs::read_dir(versions_dir).ok()?;
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(score) = parse_cursor_version_sort_key(&name) {
            scored.push((score, name));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    scored.into_iter().next().map(|(_, name)| name)
}

/// Match cursor-agent.ps1 version folder regex and produce a numeric sort key.
/// Supports `YYYY.MM.DD-commit` and `YYYY.MM.DD-HH-MM-SS-commit`.
fn parse_cursor_version_sort_key(name: &str) -> Option<i64> {
    let re = regex::Regex::new(
        r"^(\d{4})\.(\d{1,2})\.(\d{1,2})(?:-(\d{2})-(\d{2})-(\d{2}))?-[a-f0-9]+$",
    )
    .ok()?;
    let caps = re.captures(name)?;
    let year: i64 = caps.get(1)?.as_str().parse().ok()?;
    let month: i64 = caps.get(2)?.as_str().parse().ok()?;
    let day: i64 = caps.get(3)?.as_str().parse().ok()?;
    let hour: i64 = caps
        .get(4)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let minute: i64 = caps
        .get(5)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let second: i64 = caps
        .get(6)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    Some(year * 10_000_000_000 + month * 100_000_000 + day * 1_000_000 + hour * 10_000 + minute * 100 + second)
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

    #[test]
    fn strip_lone_dash_removes_only_exact_dash() {
        let argv = vec![
            "agent".into(),
            "--print".into(),
            "-".into(),
            "--mode".into(),
            "plan".into(),
            "--".into(),
        ];
        let cleaned = strip_lone_dash_argv(&argv);
        assert_eq!(
            cleaned,
            vec![
                "agent".to_string(),
                "--print".into(),
                "--mode".into(),
                "plan".into(),
                "--".into(),
            ]
        );
    }

    #[test]
    fn parse_version_sort_key_accepts_legacy_and_timestamped() {
        assert!(parse_cursor_version_sort_key("2026.07.23-e383d2b").is_some());
        assert!(parse_cursor_version_sort_key("2026.06.19-20-24-33-653a7fb").is_some());
        assert!(parse_cursor_version_sort_key("not-a-version").is_none());
        let newer = parse_cursor_version_sort_key("2026.07.23-12-00-00-abcdef0").unwrap();
        let older = parse_cursor_version_sort_key("2026.07.22-ffffff").unwrap();
        assert!(newer > older);
    }

    #[test]
    fn prepare_hosted_leaves_non_launcher_argv() {
        let argv = vec!["C:\\tools\\fake-agent.cmd".into(), "--print".into(), "-".into()];
        let (out, env) = prepare_hosted_cursor_argv(&argv);
        assert_eq!(out, vec!["C:\\tools\\fake-agent.cmd".to_string(), "--print".into()]);
        assert!(env.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn unwind_real_cursor_agent_install_when_present() {
        let local = std::env::var_os("LOCALAPPDATA");
        let Some(local) = local else {
            return;
        };
        let agent = Path::new(&local).join("cursor-agent").join("agent.cmd");
        if !agent.is_file() {
            return;
        }
        let runtime = unwind_cursor_launcher(&agent.to_string_lossy()).expect("unwind");
        assert!(
            Path::new(&runtime.node_exe).is_file(),
            "node missing: {}",
            runtime.node_exe
        );
        assert!(
            Path::new(&runtime.index_js).is_file(),
            "index missing: {}",
            runtime.index_js
        );
        let argv = vec![
            agent.to_string_lossy().into_owned(),
            "--print".into(),
            "-".into(),
            "--mode".into(),
            "plan".into(),
        ];
        let (prepared, env) = prepare_hosted_cursor_argv(&argv);
        assert_eq!(prepared[0], runtime.node_exe);
        assert_eq!(prepared[1], runtime.index_js);
        assert!(!prepared.iter().any(|a| a == "-"));
        assert!(prepared.windows(2).any(|w| w == ["--mode", "plan"]));
        assert_eq!(
            env.iter()
                .find(|(k, _)| k == "CURSOR_INVOKED_AS")
                .map(|(_, v)| v.as_str()),
            Some("agent.cmd")
        );
    }
}
