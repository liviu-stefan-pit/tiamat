use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use tiamat_lib::cursor::probe::{invalidate_probe_cache, probe_with_deps};
use tiamat_lib::cursor::types::{CursorCapabilityReport, CursorFeatureFlags};
use tiamat_lib::intake::{self, IntakeLimits};
use tiamat_lib::planner::{
    build_architect_command, project_graph, run_architect_pipeline, select_architect_model,
    validate_plan_json, ArchitectPipelineRequest, ARCHITECT_FALLBACK_MODEL,
    ARCHITECT_PREFERRED_MODEL,
};
use tiamat_lib::workspace::{materialize_run_workspace, MaterializeRequest};
use uuid::Uuid;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn fake_agent_js() -> PathBuf {
    repo_root().join("fixtures/cursor-cli/fake-agent.mjs")
}

fn rough_spec_dir() -> PathBuf {
    repo_root().join("fixtures/intake/rough-spec")
}

static GIT_LOCK: Mutex<()> = Mutex::new(());

fn probe_fake(mode: &str) -> CursorCapabilityReport {
    invalidate_probe_cache();
    let js = fake_agent_js();
    let mode_owned = mode.to_string();
    let js_for_run = js.clone();
    let run = move |argv: &[String], _timeout_ms: u64| {
        let rest: Vec<String> = if argv.len() > 1 {
            argv[1..].to_vec()
        } else {
            Vec::new()
        };
        let output = std::process::Command::new("node")
            .arg(&js_for_run)
            .args(&rest)
            .env("TIAMAT_FAKE_CLI_MODE", &mode_owned)
            .output()
            .map_err(|e| e.to_string())?;
        Ok((
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    };
    let mut env = std::collections::HashMap::new();
    env.insert("TIAMAT_CURSOR_CLI".into(), format!("node|{}", js.display()));
    probe_with_deps(None, &env, &|_| None, &run)
}

fn materialize_rough_spec(
    run_id: Uuid,
    parent: &std::path::Path,
) -> (
    tiamat_lib::intake::PreflightReport,
    tiamat_lib::workspace::RunWorkspaceManifest,
) {
    let source = rough_spec_dir();
    let report = intake::run_preflight(&[source.display().to_string()], IntakeLimits::default())
        .expect("preflight");
    let mut trusted = intake::apply_trust(report, true, true);
    assert!(trusted.can_start, "rough-spec should be startable");

    let manifest = materialize_run_workspace(MaterializeRequest {
        run_id,
        intake: trusted.manifest.clone(),
        managed_parent: parent.to_path_buf(),
        create_internal_worktrees: false,
    })
    .expect("materialize");
    // Keep trusted report in sync with materialized project ids.
    trusted.can_start = true;
    (trusted, manifest)
}

#[test]
fn architect_model_prefers_sol_and_falls_back_to_grok_high() {
    let with_sol = probe_fake("architect_valid");
    let sel = select_architect_model(&with_sol.models).unwrap();
    assert_eq!(sel.selected_model, ARCHITECT_PREFERRED_MODEL);
    assert!(!sel.degraded);

    let no_sol = probe_fake("architect_no_sol");
    let sel = select_architect_model(&no_sol.models).unwrap();
    assert_eq!(sel.selected_model, ARCHITECT_FALLBACK_MODEL);
    assert!(sel.degraded);
}

#[test]
fn architect_command_cannot_implement() {
    let dir = tempfile::tempdir().unwrap();
    let features = CursorFeatureFlags {
        print_mode: true,
        output_format: true,
        stream_json: true,
        workspace: true,
        force: true,
        model: true,
        list_models: true,
        trust: true,
        api_key: false,
        stream_partial_output: false,
        mode_plan: true,
        resume: true,
        auto_review: true,
    };
    let (built, proof) = build_architect_command(
        "agent",
        &features,
        dir.path(),
        ARCHITECT_PREFERRED_MODEL,
        "plan only",
        None,
        Some(5_000),
    )
    .unwrap();
    assert!(proof.cannot_implement());
    assert!(built.argv.windows(2).any(|w| w == ["--mode", "plan"]));
    assert!(!built.argv.iter().any(|a| a == "--force"));
    assert!(!built.argv.iter().any(|a| a == "--auto-review"));
}

#[test]
fn architect_valid_plan_persists_atomic_artifacts_and_graph() {
    let _guard = GIT_LOCK.lock().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let run_id = Uuid::new_v4();
    let (preflight, mut workspace) = materialize_rough_spec(run_id, parent.path());
    let capability = probe_fake("architect_valid");
    std::env::set_var("TIAMAT_FAKE_CLI_MODE", "architect_valid");

    let result = run_architect_pipeline(ArchitectPipelineRequest {
        run_id,
        preflight: &preflight,
        workspace: &mut workspace,
        capability: &capability,
        executable_override: Some(&format!("node|{}", fake_agent_js().display())),
        host: None,
    });
    assert!(result.ok, "{:?}", result.error);
    assert!(result.plan.is_some());
    assert!(result.checkpoint.is_some());
    assert!(!result.degraded_mode);

    let plan = result.plan.as_ref().unwrap();
    let json_path = PathBuf::from(result.plan_json_path.as_ref().unwrap());
    let md_path = PathBuf::from(result.master_plan_md_path.as_ref().unwrap());
    assert!(json_path.exists());
    assert!(md_path.exists());
    let json_text = fs::read_to_string(&json_path).unwrap();
    let md_text = fs::read_to_string(&md_path).unwrap();
    assert!(md_text.contains(&plan.title));
    assert!(md_text.contains("P01"));
    // JSON and Markdown phases match exactly via re-validation + renderer check.
    let revalidated = validate_plan_json(&json_text, run_id, &workspace).expect("revalidate");
    assert_eq!(revalidated.phases.len(), plan.phases.len());
    assert_eq!(revalidated.phases[0].phase_id, plan.phases[0].phase_id);
    assert!(md_text.contains(&plan.phases[0].title));

    let graph = project_graph(plan);
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].phase_id, "P01");

    // Architect attempt must prove cannot-implement.
    assert!(result.attempts.iter().all(|a| a.proof.cannot_implement()));
    std::env::remove_var("TIAMAT_FAKE_CLI_MODE");
}

#[test]
fn architect_invalid_fails_after_one_repair_with_evidence() {
    let _guard = GIT_LOCK.lock().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let run_id = Uuid::new_v4();
    let (preflight, mut workspace) = materialize_rough_spec(run_id, parent.path());
    let capability = probe_fake("architect_invalid");
    std::env::set_var("TIAMAT_FAKE_CLI_MODE", "architect_invalid");

    let result = run_architect_pipeline(ArchitectPipelineRequest {
        run_id,
        preflight: &preflight,
        workspace: &mut workspace,
        capability: &capability,
        executable_override: Some(&format!("node|{}", fake_agent_js().display())),
        host: None,
    });
    assert!(!result.ok);
    assert!(result.error.as_ref().unwrap().contains("repair failed"));
    assert_eq!(result.attempts.len(), 2);
    assert!(!result.attempts[0].repaired);
    assert!(result.attempts[1].repaired);
    assert!(result.attempts.iter().all(|a| a.proof.cannot_implement()));
    assert!(result.plan.is_none());
    std::env::remove_var("TIAMAT_FAKE_CLI_MODE");
}

#[test]
fn architect_repairable_succeeds_on_resume() {
    let _guard = GIT_LOCK.lock().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let run_id = Uuid::new_v4();
    let (preflight, mut workspace) = materialize_rough_spec(run_id, parent.path());
    let capability = probe_fake("architect_repairable");
    std::env::set_var("TIAMAT_FAKE_CLI_MODE", "architect_repairable");

    let result = run_architect_pipeline(ArchitectPipelineRequest {
        run_id,
        preflight: &preflight,
        workspace: &mut workspace,
        capability: &capability,
        executable_override: Some(&format!("node|{}", fake_agent_js().display())),
        host: None,
    });
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.attempts.len(), 2);
    assert!(!result.attempts[0].repaired);
    assert!(result.attempts[1].repaired);
    assert!(result.plan.is_some());
    assert!(result.evidence.iter().any(|e| e.contains("repair_resume")));
    std::env::remove_var("TIAMAT_FAKE_CLI_MODE");
}

#[test]
fn architect_degraded_mode_without_sol() {
    let _guard = GIT_LOCK.lock().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let run_id = Uuid::new_v4();
    let (preflight, mut workspace) = materialize_rough_spec(run_id, parent.path());
    let capability = probe_fake("architect_no_sol");
    std::env::set_var("TIAMAT_FAKE_CLI_MODE", "architect_no_sol");

    let result = run_architect_pipeline(ArchitectPipelineRequest {
        run_id,
        preflight: &preflight,
        workspace: &mut workspace,
        capability: &capability,
        executable_override: Some(&format!("node|{}", fake_agent_js().display())),
        host: None,
    });
    assert!(result.ok, "{:?}", result.error);
    assert!(result.degraded_mode);
    assert_eq!(
        result.model_selection.selected_model,
        ARCHITECT_FALLBACK_MODEL
    );
    std::env::remove_var("TIAMAT_FAKE_CLI_MODE");
}
