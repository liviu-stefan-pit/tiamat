//! P08 phase executor integration: success / failure / escape / timeout fixtures.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use tiamat_contracts::{
    AcceptanceCriterion, ModelTier, PhasePlan, PhaseStatus, ProjectPlan, RollbackSpec,
    RollbackStrategy, TestExpected, TestKind, TestSpec,
};
use tiamat_lib::cursor::probe::{invalidate_probe_cache, probe_with_deps};
use tiamat_lib::cursor::types::{CursorCapabilityReport, CursorFeatureFlags};
use tiamat_lib::executor::{
    assemble_phase_prompt, assemble_recovery_prompt, decide_partial_recovery, execute_phase,
    validate_phase_result_payload, ExecutePhaseRequest, ExecutionMode, PartialRecoveryDecision,
};
use tiamat_lib::intake::{self, IntakeLimits};
use tiamat_lib::workspace::{create_checkpoint, materialize_run_workspace, MaterializeRequest};
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

fn executor_app_dir() -> PathBuf {
    repo_root().join("fixtures/intake/executor-app")
}

static GIT_LOCK: Mutex<()> = Mutex::new(());

fn lock_fixtures() -> std::sync::MutexGuard<'static, ()> {
    GIT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn probe_fake() -> CursorCapabilityReport {
    invalidate_probe_cache();
    let js = fake_agent_js();
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
            .env("TIAMAT_FAKE_CLI_MODE", "success")
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
    let mut report = probe_with_deps(None, &env, &|_| None, &run);
    // Ensure implementation flags for force/trust.
    report.features = CursorFeatureFlags {
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
    };
    report
}

fn sample_phase(write_root: &str, project_id: &str) -> PhasePlan {
    PhasePlan {
        phase_id: "P01".into(),
        title: "Feature vertical slice".into(),
        objective: "Implement greet() with unit+integration+e2e gates".into(),
        dependencies: vec![],
        project_ids: vec![project_id.into()],
        read_roots: vec![write_root.into()],
        write_roots: vec![write_root.into()],
        model_tier: ModelTier::Composer,
        estimated_minutes: 10,
        acceptance_criteria: vec![AcceptanceCriterion {
            criterion_id: "AC-P01-01".into(),
            description: "greet implemented and all three test layers pass".into(),
            required_evidence_kinds: vec![TestKind::Unit, TestKind::Integration, TestKind::E2e],
        }],
        unit_tests: vec![TestSpec {
            test_id: "UT-P01-01".into(),
            command: vec!["node".into(), "tests/unit.mjs".into()],
            working_directory: ".".into(),
            timeout_seconds: 30,
            resource_locks: vec![],
            expected: TestExpected {
                exit_code: 0,
                artifacts: vec![],
            },
            covers: vec!["AC-P01-01".into()],
            inapplicable_reason: None,
        }],
        integration_tests: vec![TestSpec {
            test_id: "IT-P01-01".into(),
            command: vec!["node".into(), "tests/integration.mjs".into()],
            working_directory: ".".into(),
            timeout_seconds: 30,
            resource_locks: vec![],
            expected: TestExpected {
                exit_code: 0,
                artifacts: vec![],
            },
            covers: vec!["AC-P01-01".into()],
            inapplicable_reason: None,
        }],
        e2e_tests: vec![TestSpec {
            test_id: "E2E-P01-01".into(),
            command: vec!["node".into(), "tests/e2e.mjs".into()],
            working_directory: ".".into(),
            timeout_seconds: 30,
            resource_locks: vec![],
            expected: TestExpected {
                exit_code: 0,
                artifacts: vec![],
            },
            covers: vec!["AC-P01-01".into()],
            inapplicable_reason: None,
        }],
        manual_checks: vec![],
        rollback: RollbackSpec {
            checkpoint: "intake-baseline".into(),
            strategy: RollbackStrategy::Restore,
        },
        expected_artifacts: vec!["src/feature.ts".into()],
        prompt: "Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json. Implement only P01. Preserve unrelated work. Add/run unit, integration, and E2E tests. Return a schema-valid immutable phase-result payload.".into(),
        status: PhaseStatus::Draft,
        evidence: vec![],
    }
}

fn materialize_executor_app(
    run_id: Uuid,
    parent: &std::path::Path,
) -> (
    tiamat_lib::intake::PreflightReport,
    tiamat_lib::workspace::RunWorkspaceManifest,
) {
    let source = executor_app_dir();
    let report = intake::run_preflight(&[source.display().to_string()], IntakeLimits::default())
        .expect("preflight");
    let trusted = intake::apply_trust(report, true, true);
    assert!(trusted.can_start);

    let manifest = materialize_run_workspace(MaterializeRequest {
        run_id,
        intake: trusted.manifest.clone(),
        managed_parent: parent.to_path_buf(),
        create_internal_worktrees: false,
    })
    .expect("materialize");
    (trusted, manifest)
}

fn plan_for(run_id: Uuid, manifest: &tiamat_lib::workspace::RunWorkspaceManifest) -> ProjectPlan {
    let project = manifest.projects.first().expect("project");
    ProjectPlan {
        schema_version: 1,
        run_id,
        title: "Executor fixture".into(),
        summary: "P08 fake project".into(),
        assumptions: vec![],
        risks: vec![],
        phases: vec![sample_phase(&project.write_root, &project.project_id)],
        final_gates: vec![],
    }
}

#[test]
fn unit_decisions_prompt_recovery_and_result() {
    let phase = sample_phase(r"C:\managed\app", "app");
    let prompt = assemble_phase_prompt(&phase, "ctx");
    assert!(prompt.contains("immutable"));
    let recovery = assemble_recovery_prompt(&phase, "partial timeout");
    assert!(recovery.contains("Resume the same assigned phase"));
    assert!(matches!(
        decide_partial_recovery(true, true, RollbackStrategy::Restore, None, None),
        PartialRecoveryDecision::Resume { .. }
    ));
    let value = serde_json::json!({
        "schemaVersion": 1,
        "phaseId": "P01",
        "status": "passed",
        "summary": "ok",
        "changedFiles": ["src/feature.ts"],
        "evidenceIds": [],
        "acceptanceSatisfied": ["AC-P01-01"],
        "artifacts": [],
        "immutable": true
    });
    assert!(validate_phase_result_payload(&value).is_ok());
}

#[test]
fn fixture_success_checkpoints_only_after_all_three_gates() {
    let _lock = lock_fixtures();
    let dir = tempfile::tempdir().unwrap();
    let run_id = Uuid::new_v4();
    let (_preflight, mut workspace) = materialize_executor_app(run_id, dir.path());
    let project_id = workspace.projects[0].project_id.clone();
    // Intake baseline checkpoint so rollback tests have a parent.
    create_checkpoint(&mut workspace, &project_id, "intake-baseline").unwrap();

    let mut plan = plan_for(run_id, &workspace);
    let capability = probe_fake();
    let exe = format!("node|{}", fake_agent_js().display());

    let outcome = execute_phase(ExecutePhaseRequest {
        run_id,
        attempt_id: Some(Uuid::new_v4()),
        plan: &mut plan,
        phase_id: "P01",
        workspace: &mut workspace,
        capability: &capability,
        model_id: "composer-2.5",
        mode: ExecutionMode::Fresh,
        interruption_report: None,
        resume_chat_id: None,
        executable_override: Some(&exe),
        fake_cli_mode: Some("impl_success"),
        timeout_ms: Some(30_000),
        establish_baseline: true,
        flaky_retry: true,
        host: None,
    })
    .expect("execute");

    assert!(outcome.ok, "{}", outcome.message);
    assert_eq!(outcome.terminal_status, PhaseStatus::Passed);
    assert!(outcome.plan_projected);
    assert!(outcome.project_checkpoint.is_some());
    assert!(outcome.control_checkpoint.is_some());
    assert!(outcome.phase_result.as_ref().unwrap().immutable);
    assert!(outcome.boundary_ok);
    assert_eq!(outcome.layers.len(), 3);
    assert!(outcome.layers.iter().all(|l| l.all_required_passed()));
    assert!(plan.phases[0].status == PhaseStatus::Passed);

    // Plan projections written.
    let plan_json = fs::read_to_string(
        PathBuf::from(&workspace.control_root)
            .join(".tiamat")
            .join("plan.json"),
    )
    .unwrap();
    assert!(
        plan_json.contains("\"status\": \"passed\"") || plan_json.contains("\"status\":\"passed\"")
    );
}

#[test]
fn fixture_failed_tests_prevent_checkpoint() {
    let _lock = lock_fixtures();
    let dir = tempfile::tempdir().unwrap();
    let run_id = Uuid::new_v4();
    let (_preflight, mut workspace) = materialize_executor_app(run_id, dir.path());
    let project_id = workspace.projects[0].project_id.clone();
    create_checkpoint(&mut workspace, &project_id, "intake-baseline").unwrap();
    let checkpoints_before = workspace.checkpoints.len();

    let mut plan = plan_for(run_id, &workspace);
    let capability = probe_fake();
    let exe = format!("node|{}", fake_agent_js().display());

    let outcome = execute_phase(ExecutePhaseRequest {
        run_id,
        attempt_id: None,
        plan: &mut plan,
        phase_id: "P01",
        workspace: &mut workspace,
        capability: &capability,
        model_id: "composer-2.5",
        mode: ExecutionMode::Fresh,
        interruption_report: None,
        resume_chat_id: None,
        executable_override: Some(&exe),
        fake_cli_mode: Some("impl_fail_tests"),
        timeout_ms: Some(30_000),
        establish_baseline: false,
        flaky_retry: false,
        host: None,
    })
    .expect("execute");

    assert!(!outcome.ok);
    assert!(outcome.project_checkpoint.is_none());
    assert_eq!(outcome.terminal_status, PhaseStatus::Failed);
    assert!(
        outcome.message.to_lowercase().contains("gate")
            || outcome.message.to_lowercase().contains("verif")
    );
    // No new project pass checkpoint (control may still record failure projection).
    let project_pass_cps = workspace
        .checkpoints
        .iter()
        .filter(|c| c.project_id == project_id && c.message.contains("passed gates"))
        .count();
    assert_eq!(project_pass_cps, 0);
    assert!(workspace.checkpoints.len() >= checkpoints_before);
}

#[test]
fn fixture_escape_quarantines_without_pass() {
    let _lock = lock_fixtures();
    let dir = tempfile::tempdir().unwrap();
    let run_id = Uuid::new_v4();
    let (_preflight, mut workspace) = materialize_executor_app(run_id, dir.path());
    let project_id = workspace.projects[0].project_id.clone();
    create_checkpoint(&mut workspace, &project_id, "intake-baseline").unwrap();

    let mut plan = plan_for(run_id, &workspace);
    let capability = probe_fake();
    let exe = format!("node|{}", fake_agent_js().display());

    let outcome = execute_phase(ExecutePhaseRequest {
        run_id,
        attempt_id: None,
        plan: &mut plan,
        phase_id: "P01",
        workspace: &mut workspace,
        capability: &capability,
        model_id: "composer-2.5",
        mode: ExecutionMode::Fresh,
        interruption_report: None,
        resume_chat_id: None,
        executable_override: Some(&exe),
        fake_cli_mode: Some("impl_escape"),
        timeout_ms: Some(30_000),
        establish_baseline: false,
        flaky_retry: false,
        host: None,
    })
    .expect("execute");

    assert!(!outcome.ok);
    assert!(!outcome.boundary_ok);
    assert!(outcome.quarantined.is_some());
    assert!(outcome.project_checkpoint.is_none());
    assert!(
        outcome.message.to_lowercase().contains("quarantine")
            || outcome.message.to_lowercase().contains("bound")
    );
}

#[test]
fn fixture_timeout_partial_resume_or_rollback() {
    let _lock = lock_fixtures();
    let dir = tempfile::tempdir().unwrap();
    let run_id = Uuid::new_v4();
    let (_preflight, mut workspace) = materialize_executor_app(run_id, dir.path());
    let project_id = workspace.projects[0].project_id.clone();
    create_checkpoint(&mut workspace, &project_id, "intake-baseline").unwrap();

    let mut plan = plan_for(run_id, &workspace);
    let capability = probe_fake();
    let exe = format!("node|{}", fake_agent_js().display());

    let outcome = execute_phase(ExecutePhaseRequest {
        run_id,
        attempt_id: None,
        plan: &mut plan,
        phase_id: "P01",
        workspace: &mut workspace,
        capability: &capability,
        model_id: "composer-2.5",
        mode: ExecutionMode::Fresh,
        interruption_report: None,
        resume_chat_id: None,
        executable_override: Some(&exe),
        fake_cli_mode: Some("impl_timeout_partial"),
        timeout_ms: Some(800),
        establish_baseline: false,
        flaky_retry: false,
        host: None,
    })
    .expect("execute");

    assert!(!outcome.ok);
    assert!(outcome.project_checkpoint.is_none());
    let recovery = outcome.recovery.expect("recovery decision");
    assert!(
        recovery.decision == "resume" || recovery.decision == "rollback",
        "unexpected decision {}",
        recovery.decision
    );
    // Partial file should exist if resume; may be cleaned if rollback.
    if recovery.decision == "resume" {
        assert!(recovery.progress_useful);
        let partial = PathBuf::from(&workspace.projects[0].managed_root)
            .join("src")
            .join("partial.ts");
        assert!(
            partial.exists(),
            "partial progress should remain for resume"
        );
    }
}
