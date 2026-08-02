use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use regex::Regex;

use super::process::run_argv_capture;
use super::resolve::env_map;
use super::types::{
    CursorAuthStatus, CursorCapabilityReport, CursorCapabilityStatus, CursorFeatureFlags,
    CursorModelInfo, CursorModelsReport, CursorModelsStatus, HELP_EXCERPT_LIMIT,
    MINIMUM_CURSOR_CLI_VERSION, MODELS_PROBE_TIMEOUT_MS, PROBE_TIMEOUT_MS,
};

static CACHE: Mutex<Option<CachedProbe>> = Mutex::new(None);
const CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct CachedProbe {
    executable: String,
    report: CursorCapabilityReport,
    at: Instant,
}

pub type ProbeRunner = dyn Fn(&[String], u64) -> Result<(i32, String, String), String>;

/// Discover features strictly from help text markers.
pub fn discover_features(help_text: &str) -> CursorFeatureFlags {
    CursorFeatureFlags {
        print_mode: help_text.contains("--print"),
        output_format: help_text.contains("--output-format"),
        stream_json: help_text.contains("stream-json"),
        workspace: help_text.contains("--workspace"),
        force: help_text.contains("--force"),
        model: help_text.contains("--model"),
        list_models: help_text.contains("--list-models"),
        trust: help_text.contains("--trust"),
        api_key: help_text.contains("--api-key"),
        stream_partial_output: help_text.contains("--stream-partial-output"),
        mode_plan: help_text.contains("--mode") || help_text.contains("--plan"),
        resume: help_text.contains("--resume"),
        auto_review: help_text.contains("--auto-review"),
    }
}

pub fn parse_version_string(raw: &str) -> Option<String> {
    let re = Regex::new(r"(?P<version>\d+(?:\.\d+){1,3}(?:[-+][0-9A-Za-z.-]+)?)").ok()?;
    re.captures(raw.trim())
        .and_then(|c| c.name("version").map(|m| m.as_str().to_string()))
}

fn version_tuple(version: &str) -> Vec<u64> {
    let core = version.split(['+', '-']).next().unwrap_or(version);
    core.split('.')
        .map_while(|p| p.parse::<u64>().ok())
        .collect()
}

pub fn is_version_supported(version: &str, minimum: &str) -> bool {
    version_tuple(version) >= version_tuple(minimum)
}

fn help_excerpt(help_text: &str) -> Option<String> {
    let text = help_text.trim();
    if text.is_empty() {
        return None;
    }
    if text.len() <= HELP_EXCERPT_LIMIT {
        Some(text.to_string())
    } else {
        let mut truncated = text
            .chars()
            .take(HELP_EXCERPT_LIMIT.saturating_sub(1))
            .collect::<String>();
        truncated.push('…');
        Some(truncated)
    }
}

fn default_run(argv: &[String], timeout_ms: u64) -> Result<(i32, String, String), String> {
    let capture = run_argv_capture(argv, timeout_ms, None)?;
    if capture.timed_out {
        return Err("Cursor CLI timed out".into());
    }
    Ok((
        capture.exit_code.unwrap_or(-1),
        capture.stdout,
        capture.stderr,
    ))
}

pub fn invalidate_probe_cache() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = None;
    }
}

pub fn probe_cursor_capability() -> CursorCapabilityReport {
    probe_with_deps(
        None,
        &env_map(),
        &crate::cursor::resolve::default_which,
        &default_run,
    )
}

pub fn probe_cursor_capability_with_configured(configured: Option<&str>) -> CursorCapabilityReport {
    probe_with_deps(
        configured,
        &env_map(),
        &crate::cursor::resolve::default_which,
        &default_run,
    )
}

pub fn probe_with_deps(
    configured: Option<&str>,
    environ: &HashMap<String, String>,
    which: &dyn Fn(&str) -> Option<String>,
    run: &ProbeRunner,
) -> CursorCapabilityReport {
    let executable = match crate::cursor::resolve::resolve_cursor_executable_with_configured(
        configured, environ, which,
    ) {
        Some(path) => path,
        None => {
            return CursorCapabilityReport::absent(
                    "Cursor CLI not found. Install the Cursor agent CLI (`agent` / `cursor-agent`), set TIAMAT_CURSOR_CLI, or configure the path in Settings.",
                );
        }
    };

    if let Ok(guard) = CACHE.lock() {
        if let Some(cached) = guard.as_ref() {
            if cached.executable == executable && cached.at.elapsed() < CACHE_TTL {
                return cached.report.clone();
            }
        }
    }

    let report = probe_executable(&executable, run);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(CachedProbe {
            executable: executable.clone(),
            report: report.clone(),
            at: Instant::now(),
        });
    }
    report
}

fn probe_executable(executable: &str, run: &ProbeRunner) -> CursorCapabilityReport {
    let version_argv = vec![executable.to_string(), "--version".into()];
    let (version_raw, version) = match run(&version_argv, PROBE_TIMEOUT_MS) {
        Ok((_code, stdout, stderr)) => {
            let raw = if !stdout.trim().is_empty() {
                stdout
            } else {
                stderr
            };
            let raw = raw.trim().to_string();
            let parsed = parse_version_string(&raw);
            (Some(raw), parsed)
        }
        Err(err) => {
            return CursorCapabilityReport {
                status: CursorCapabilityStatus::Error,
                message: format!("Could not execute Cursor CLI --version: {err}"),
                executable: Some(executable.into()),
                version: None,
                version_raw: None,
                minimum_version: MINIMUM_CURSOR_CLI_VERSION.into(),
                help_excerpt: None,
                features: CursorFeatureFlags::default(),
                auth: CursorAuthStatus::Unknown,
                auth_message: None,
                models: Vec::new(),
                probed_at_utc: chrono::Utc::now().to_rfc3339(),
            };
        }
    };

    let help_argv = vec![executable.to_string(), "--help".into()];
    let help_text = match run(&help_argv, PROBE_TIMEOUT_MS) {
        Ok((_code, stdout, stderr)) => format!("{stdout}\n{stderr}"),
        Err(err) => {
            return CursorCapabilityReport {
                status: CursorCapabilityStatus::Error,
                message: format!("Could not execute Cursor CLI --help: {err}"),
                executable: Some(executable.into()),
                version: version.clone(),
                version_raw: version_raw.clone(),
                minimum_version: MINIMUM_CURSOR_CLI_VERSION.into(),
                help_excerpt: None,
                features: CursorFeatureFlags::default(),
                auth: CursorAuthStatus::Unknown,
                auth_message: None,
                models: Vec::new(),
                probed_at_utc: chrono::Utc::now().to_rfc3339(),
            };
        }
    };

    let features = discover_features(&help_text);
    let excerpt = help_excerpt(&help_text);

    let Some(version) = version else {
        return CursorCapabilityReport {
            status: CursorCapabilityStatus::Error,
            message:
                "Cursor CLI responded but no version number could be parsed from --version output."
                    .into(),
            executable: Some(executable.into()),
            version: None,
            version_raw,
            minimum_version: MINIMUM_CURSOR_CLI_VERSION.into(),
            help_excerpt: excerpt,
            features,
            auth: CursorAuthStatus::Unknown,
            auth_message: None,
            models: Vec::new(),
            probed_at_utc: chrono::Utc::now().to_rfc3339(),
        };
    };

    if !is_version_supported(&version, MINIMUM_CURSOR_CLI_VERSION) {
        return CursorCapabilityReport {
            status: CursorCapabilityStatus::UnsupportedVersion,
            message: format!(
                "Cursor CLI version {version} is below the minimum supported version {MINIMUM_CURSOR_CLI_VERSION}."
            ),
            executable: Some(executable.into()),
            version: Some(version),
            version_raw,
            minimum_version: MINIMUM_CURSOR_CLI_VERSION.into(),
            help_excerpt: excerpt,
            features,
            auth: CursorAuthStatus::Unknown,
            auth_message: None,
            models: Vec::new(),
            probed_at_utc: chrono::Utc::now().to_rfc3339(),
        };
    }

    let (auth, auth_message) = probe_auth(executable, &features, run);
    let models = if features.list_models {
        list_models_for_executable(executable, run)
            .map(|r| r.models)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    CursorCapabilityReport {
        status: CursorCapabilityStatus::Available,
        message: format!("Cursor CLI available (version {version})."),
        executable: Some(executable.into()),
        version: Some(version),
        version_raw,
        minimum_version: MINIMUM_CURSOR_CLI_VERSION.into(),
        help_excerpt: excerpt,
        features,
        auth,
        auth_message,
        models,
        probed_at_utc: chrono::Utc::now().to_rfc3339(),
    }
}

fn probe_auth(
    executable: &str,
    _features: &CursorFeatureFlags,
    run: &ProbeRunner,
) -> (CursorAuthStatus, Option<String>) {
    // Prefer `status`, then `whoami`. These are readiness-only probes — never prompts.
    for sub in ["status", "whoami"] {
        let argv = vec![executable.to_string(), sub.into()];
        match run(&argv, PROBE_TIMEOUT_MS) {
            Ok((code, stdout, stderr)) => {
                let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
                if combined.contains("not logged")
                    || combined.contains("unauthenticated")
                    || (combined.contains("auth") && combined.contains("fail"))
                    || combined.contains("login required")
                    || code == 3
                {
                    return (
                        CursorAuthStatus::Unauthenticated,
                        Some(format!(
                            "Cursor CLI `{sub}` reported authentication is required."
                        )),
                    );
                }
                if code == 0
                    || combined.contains("logged in")
                    || combined.contains("authenticated")
                    || combined.contains("ok")
                {
                    return (
                        CursorAuthStatus::Ready,
                        Some(format!("Cursor CLI `{sub}` reports ready.")),
                    );
                }
                if code != 0 && !stdout.trim().is_empty() {
                    continue;
                }
            }
            Err(_) => continue,
        }
    }
    (
        CursorAuthStatus::Unknown,
        Some("Auth readiness was not determined.".into()),
    )
}

pub fn list_models_for_executable(
    executable: &str,
    run: &ProbeRunner,
) -> Result<CursorModelsReport, String> {
    let argv = vec![executable.to_string(), "--list-models".into()];
    let (code, stdout, stderr) = run(&argv, MODELS_PROBE_TIMEOUT_MS)?;
    if code != 0 && stdout.trim().is_empty() {
        return Ok(CursorModelsReport {
            status: CursorModelsStatus::Error,
            models: Vec::new(),
            message: Some(format!("--list-models failed: {}", stderr.trim())),
            executable: Some(executable.into()),
        });
    }
    let models = parse_models_output(&stdout);
    Ok(CursorModelsReport {
        status: CursorModelsStatus::Available,
        models,
        message: None,
        executable: Some(executable.into()),
    })
}

pub fn parse_models_output(stdout: &str) -> Vec<CursorModelInfo> {
    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in stdout.lines() {
        let id = line.trim();
        if id.is_empty() || id.eq_ignore_ascii_case("auto") {
            continue;
        }
        // Skip help-ish lines.
        if (id.starts_with('-') || (id.contains(' ') && !id.contains('/')))
            && !id.contains("composer")
            && !id.contains("grok")
            && !id.contains("gpt")
        {
            continue;
        }
        let key = id.to_ascii_lowercase();
        if seen.insert(key) {
            models.push(CursorModelInfo {
                id: id.to_string(),
                label: id.to_string(),
            });
        }
    }
    models
}

pub fn list_cursor_models() -> CursorModelsReport {
    let environ = env_map();
    let Some(executable) = crate::cursor::resolve::resolve_cursor_executable(
        &environ,
        &crate::cursor::resolve::default_which,
    ) else {
        return CursorModelsReport {
            status: CursorModelsStatus::Absent,
            models: Vec::new(),
            message: Some("Cursor CLI not found.".into()),
            executable: None,
        };
    };
    list_models_for_executable(&executable, &default_run).unwrap_or_else(|err| CursorModelsReport {
        status: CursorModelsStatus::Error,
        models: Vec::new(),
        message: Some(err),
        executable: Some(executable),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_features_requires_help_markers() {
        let help = r#"
Usage: agent [options]
  --print
  --output-format <text|json|stream-json>
  --workspace <path>
  --model <id>
  --list-models
  --trust
  --force
  --resume <id>
  --mode plan
  --auto-review
"#;
        let features = discover_features(help);
        assert!(features.print_mode);
        assert!(features.stream_json);
        assert!(features.resume);
        assert!(features.mode_plan);
        assert!(features.auto_review);
        assert!(!features.api_key);
        assert!(!features.stream_partial_output);
    }

    #[test]
    fn parse_version_and_support_gate() {
        assert_eq!(
            parse_version_string("agent 1.2.3\n").as_deref(),
            Some("1.2.3")
        );
        assert!(is_version_supported("1.0.0", "0.1.0"));
        assert!(!is_version_supported("0.0.1", "0.1.0"));
    }

    #[test]
    fn probe_uses_injected_runner_without_live_cli() {
        invalidate_probe_cache();
        let mut env = HashMap::new();
        env.insert("TIAMAT_CURSOR_CLI".into(), "fake-agent".into());
        let run = |argv: &[String], _timeout: u64| {
            let joined = argv.join(" ");
            if joined.contains("--version") {
                Ok((0, "1.2.3\n".into(), String::new()))
            } else if joined.contains("--help") {
                Ok((
                    0,
                    "--print --output-format stream-json --workspace --model --list-models --trust --force --resume\n".into(),
                    String::new(),
                ))
            } else if joined.contains("--list-models") {
                Ok((0, "composer-2.5\ncomposer-2.5-fast\n".into(), String::new()))
            } else if joined.ends_with(" status") || joined.contains(" status") {
                Ok((0, "logged in\n".into(), String::new()))
            } else {
                Ok((0, String::new(), String::new()))
            }
        };
        let report = probe_with_deps(None, &env, &|_| None, &run);
        assert_eq!(report.status, CursorCapabilityStatus::Available);
        assert_eq!(report.version.as_deref(), Some("1.2.3"));
        assert!(report.features.print_mode);
        assert!(report.features.stream_json);
        assert_eq!(report.auth, CursorAuthStatus::Ready);
        assert!(report.models.iter().any(|m| m.id == "composer-2.5"));
    }
}
