use std::path::Path;

use crate::cursor::{
    build_cursor_command, BuiltCursorCommand, CursorFeatureFlags, CursorInvokeRequest,
};
use crate::planner::types::ArchitectInvocationProof;

/// Build an architect-only Cursor invocation.
///
/// Guarantees:
/// - `plan_mode = true` (emits `--mode plan` when discovered)
/// - `force = false` and `auto_review = false` (no implementation approval)
/// - workspace is the read-only control/intake mount
pub fn build_architect_command(
    executable: &str,
    features: &CursorFeatureFlags,
    workspace: &Path,
    model: &str,
    prompt: &str,
    resume_chat_id: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<(BuiltCursorCommand, ArchitectInvocationProof), String> {
    if !features.mode_plan {
        return Err(
            "Cursor CLI does not advertise plan mode; architect invocation is blocked".into(),
        );
    }

    let request = CursorInvokeRequest {
        workspace: workspace.display().to_string(),
        model: Some(model.to_string()),
        prompt: prompt.to_string(),
        output_format: Some("stream-json".into()),
        resume_chat_id: resume_chat_id.map(|s| s.to_string()),
        force: false,
        trust: true,
        auto_review: false,
        plan_mode: true,
        api_key: None,
        timeout_ms,
    };

    let built = build_cursor_command(executable, features, &request, Some(workspace))
        .map_err(|e| e.to_string())?;

    let proof = ArchitectInvocationProof {
        plan_mode: true,
        force: false,
        auto_review: false,
        workspace: built.workspace.clone(),
        argv: built.argv.clone(),
        model: model.to_string(),
    };

    if !proof.cannot_implement() {
        return Err(format!(
            "architect command failed cannot-implement proof: argv={:?}",
            proof.argv
        ));
    }

    Ok((built, proof))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn features() -> CursorFeatureFlags {
        CursorFeatureFlags {
            print_mode: true,
            output_format: true,
            stream_json: true,
            workspace: true,
            force: true, // available, but architect must not use it
            model: true,
            list_models: true,
            trust: true,
            api_key: true,
            stream_partial_output: false,
            mode_plan: true,
            resume: true,
            auto_review: true, // available, but architect must not use it
        }
    }

    #[test]
    fn architect_command_is_plan_mode_without_force() {
        let dir = tempdir().unwrap();
        let (built, proof) = build_architect_command(
            "agent",
            &features(),
            dir.path(),
            "gpt-5.6-sol-high",
            "plan only",
            None,
            Some(5_000),
        )
        .unwrap();
        assert!(proof.cannot_implement());
        assert!(built.argv.windows(2).any(|w| w == ["--mode", "plan"]));
        assert!(!built.argv.iter().any(|a| a == "--force"));
        assert!(!built.argv.iter().any(|a| a == "--auto-review"));
        assert!(built
            .argv
            .windows(2)
            .any(|w| w == ["--model", "gpt-5.6-sol-high"]));
    }

    #[test]
    fn blocks_when_plan_mode_unavailable() {
        let dir = tempdir().unwrap();
        let mut feats = features();
        feats.mode_plan = false;
        let err = build_architect_command(
            "agent",
            &feats,
            dir.path(),
            "gpt-5.6-sol-high",
            "x",
            None,
            None,
        )
        .unwrap_err();
        assert!(err.contains("plan mode"));
    }
}
