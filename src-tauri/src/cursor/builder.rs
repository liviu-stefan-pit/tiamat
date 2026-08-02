use std::path::{Path, PathBuf};

use thiserror::Error;

use super::redaction::{quote_windows_command, redact_argv, redact_text_secrets};
use super::types::{
    BuiltCursorCommand, CursorCommandPreview, CursorFeatureFlags, CursorInvokeRequest,
    DEFAULT_CURSOR_TIMEOUT_MS,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BuilderError {
    #[error("Cursor CLI executable is required")]
    ExecutableMissing,
    #[error("timeoutMs must be a positive integer")]
    TimeoutInvalid,
    #[error("workspace path is invalid: {0}")]
    WorkspaceInvalid(String),
    #[error("workspace '{0}' is outside the approved root '{1}'")]
    WorkspaceBoundary(String, String),
}

/// Build argv + stdin from discovered features. Unsupported flags are omitted.
pub fn build_cursor_command(
    executable: &str,
    features: &CursorFeatureFlags,
    request: &CursorInvokeRequest,
    allowed_root: Option<&Path>,
) -> Result<BuiltCursorCommand, BuilderError> {
    let executable = executable.trim();
    if executable.is_empty() {
        return Err(BuilderError::ExecutableMissing);
    }

    let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_CURSOR_TIMEOUT_MS);
    if timeout_ms == 0 {
        return Err(BuilderError::TimeoutInvalid);
    }

    let workspace = check_workspace_boundary(&request.workspace, allowed_root)?;

    let mut argv = vec![executable.to_string()];

    if features.print_mode {
        argv.push("--print".into());
    }

    if features.output_format {
        let fmt = request
            .output_format
            .as_deref()
            .unwrap_or(if features.stream_json {
                "stream-json"
            } else {
                "text"
            })
            .trim();
        let fmt = if fmt.is_empty() { "text" } else { fmt };
        // Prefer stream-json only when help advertised it.
        let fmt = if fmt == "stream-json" && !features.stream_json {
            "text"
        } else {
            fmt
        };
        argv.push("--output-format".into());
        argv.push(fmt.into());
    }

    if features.workspace {
        argv.push("--workspace".into());
        argv.push(workspace.display().to_string());
    }

    if features.trust && request.trust {
        argv.push("--trust".into());
    }

    if features.force && request.force {
        argv.push("--force".into());
    } else if features.auto_review && request.auto_review {
        argv.push("--auto-review".into());
    }

    if features.mode_plan && request.plan_mode {
        // Prefer --mode plan when advertised; --plan is also accepted via marker.
        argv.push("--mode".into());
        argv.push("plan".into());
    }

    if features.resume {
        if let Some(chat_id) = request
            .resume_chat_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            argv.push("--resume".into());
            argv.push(chat_id.into());
        }
    }

    // Strict: only pass --model when help advertised it.
    if features.model {
        if let Some(model) = request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            argv.push("--model".into());
            argv.push(model.into());
        }
    }

    if features.api_key {
        if let Some(api_key) = request
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            argv.push("--api-key".into());
            argv.push(api_key.into());
        }
    }

    Ok(BuiltCursorCommand {
        argv,
        stdin: request.prompt.clone(),
        timeout_ms,
        workspace: workspace.display().to_string(),
        executable: executable.to_string(),
    })
}

pub fn preview_built_command(built: &BuiltCursorCommand, secrets: &[&str]) -> CursorCommandPreview {
    let argv = redact_argv(&built.argv);
    let command_display = quote_windows_command(&argv);
    let mut secret_list = secrets.to_vec();
    if let Some(pos) = built.argv.iter().position(|a| a == "--api-key") {
        if let Some(value) = built.argv.get(pos + 1) {
            secret_list.push(value.as_str());
        }
    }
    for item in &built.argv {
        if let Some(value) = item.strip_prefix("--api-key=") {
            secret_list.push(value);
        }
    }
    let stdin_preview = redact_text_secrets(&built.stdin, &secret_list);
    CursorCommandPreview {
        argv,
        command_display,
        stdin_preview,
        timeout_ms: built.timeout_ms,
        workspace: built.workspace.clone(),
        executable: built.executable.clone(),
        spawned: false,
    }
}

pub fn check_workspace_boundary(
    workspace: &str,
    allowed_root: Option<&Path>,
) -> Result<PathBuf, BuilderError> {
    let raw = PathBuf::from(workspace);
    let resolved = raw.canonicalize().unwrap_or_else(|_| {
        // Allow non-existent managed roots during dry-run by normalizing without requiring existence.
        if raw.is_absolute() {
            raw.clone()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&raw)
        }
    });

    if let Some(root) = allowed_root {
        let root_resolved = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let ok = resolved.starts_with(&root_resolved)
            || resolved
                .to_string_lossy()
                .to_lowercase()
                .starts_with(&root_resolved.to_string_lossy().to_lowercase());
        if !ok {
            return Err(BuilderError::WorkspaceBoundary(
                resolved.display().to_string(),
                root_resolved.display().to_string(),
            ));
        }
    }

    if workspace.contains("..") {
        // Soft reject obvious traversal in the provided string when a root is set.
        if allowed_root.is_some() {
            let normalized = workspace.replace('\\', "/");
            if normalized.split('/').any(|p| p == "..") {
                return Err(BuilderError::WorkspaceInvalid(
                    "path traversal is not allowed".into(),
                ));
            }
        }
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn full_features() -> CursorFeatureFlags {
        CursorFeatureFlags {
            print_mode: true,
            output_format: true,
            stream_json: true,
            workspace: true,
            force: true,
            model: true,
            list_models: true,
            trust: true,
            api_key: true,
            stream_partial_output: false,
            mode_plan: true,
            resume: true,
            auto_review: true,
        }
    }

    #[test]
    fn omits_unsupported_flags() {
        let features = CursorFeatureFlags {
            print_mode: true,
            model: false,
            force: false,
            trust: false,
            output_format: false,
            workspace: false,
            ..CursorFeatureFlags::default()
        };
        let request = CursorInvokeRequest {
            workspace: ".".into(),
            model: Some("composer-2.5".into()),
            prompt: "hello".into(),
            force: true,
            trust: true,
            ..CursorInvokeRequest::default()
        };
        let built = build_cursor_command("agent", &features, &request, None).unwrap();
        assert_eq!(built.argv, vec!["agent".to_string(), "--print".to_string()]);
        assert!(!built.argv.iter().any(|a| a == "--model"));
        assert!(!built.argv.iter().any(|a| a == "--force"));
    }

    #[test]
    fn builds_stream_json_resume_command_with_argv_array() {
        let dir = tempdir().unwrap();
        let request = CursorInvokeRequest {
            workspace: dir.path().display().to_string(),
            model: Some("composer-2.5".into()),
            prompt: "continue".into(),
            output_format: Some("stream-json".into()),
            resume_chat_id: Some("chat-123".into()),
            force: true,
            trust: true,
            ..CursorInvokeRequest::default()
        };
        let built = build_cursor_command(
            "C:\\tools\\agent.exe",
            &full_features(),
            &request,
            Some(dir.path()),
        )
        .unwrap();
        assert_eq!(built.argv[0], "C:\\tools\\agent.exe");
        assert!(built.argv.contains(&"--print".into()));
        assert!(built
            .argv
            .windows(2)
            .any(|w| w == ["--output-format", "stream-json"]));
        assert!(built.argv.windows(2).any(|w| w == ["--resume", "chat-123"]));
        assert!(built
            .argv
            .windows(2)
            .any(|w| w == ["--model", "composer-2.5"]));
        assert_eq!(built.stdin, "continue");
        // Never a single shell-concatenated command string for execution.
        assert!(built.argv.len() > 1);
    }

    #[test]
    fn falls_back_from_stream_json_when_not_advertised() {
        let mut features = full_features();
        features.stream_json = false;
        let request = CursorInvokeRequest {
            workspace: ".".into(),
            prompt: "x".into(),
            output_format: Some("stream-json".into()),
            ..CursorInvokeRequest::default()
        };
        let built = build_cursor_command("agent", &features, &request, None).unwrap();
        assert!(built
            .argv
            .windows(2)
            .any(|w| w == ["--output-format", "text"]));
    }

    #[test]
    fn preview_redacts_api_key_and_never_marks_spawned() {
        let built = BuiltCursorCommand {
            argv: vec!["agent".into(), "--api-key".into(), "sekrit".into()],
            stdin: "do not leak sekrit".into(),
            timeout_ms: 1000,
            workspace: "C:\\ws".into(),
            executable: "agent".into(),
        };
        let preview = preview_built_command(&built, &[]);
        assert!(!preview.spawned);
        assert!(!preview.argv.join(" ").contains("sekrit"));
        assert!(!preview.stdin_preview.contains("sekrit"));
        assert!(!preview.command_display.contains("sekrit"));
    }

    #[test]
    fn rejects_workspace_outside_allowed_root() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let request = CursorInvokeRequest {
            workspace: outside.path().display().to_string(),
            prompt: "x".into(),
            ..CursorInvokeRequest::default()
        };
        let err = build_cursor_command("agent", &full_features(), &request, Some(root.path()))
            .unwrap_err();
        assert!(matches!(err, BuilderError::WorkspaceBoundary(_, _)));
    }
}
