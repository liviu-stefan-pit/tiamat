//! Proves the missing scheduler↔executor link: tick → execute_phase → complete_attempt.

use std::path::PathBuf;
use std::sync::Mutex;

use tempfile::tempdir;
use tiamat_contracts::{
    AcceptanceCriterion, ModelTier, PhasePlan, PhaseStatus, ProjectPlan, RollbackSpec,
    RollbackStrategy, TestExpected, TestKind, TestSpec,
};
use tiamat_lib::cursor::probe::{invalidate_probe_cache, probe_with_deps};
use tiamat_lib::cursor::types::{CursorCapabilityReport, CursorFeatureFlags, CursorModelInfo};
use tiamat_lib::db::Store;
use tiamat_lib::executor::{execute_phase, ExecutePhaseRequest, ExecutionMode};
use tiamat_lib::intake::{self, IntakeLimits};
use tiamat_lib::process::{HostedSpawnContext, ProcessHost};
use tiamat_lib::scheduler::{
    complete_attempt, load_plan_into_scheduler, snapshot, tick, AttemptTerminalResult,
    PhaseRuntimeStatus, SchedulerConfig, MODEL_COMPOSER,
};
use tiamat_lib::workspace::{create_checkpoint, materialize_run_workspace, MaterializeRequest};
use uuid::Uuid;

static GIT_LOCK: Mutex<()> = Mutex::new(());

fn lock_fixtures() -> std::sync::MutexGuard<'static, ()> {
    GIT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn fake_agent_js() -> PathBuf {
    repo_root().join("fixtures/cursor-cli/fake-agent.mjs")
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

fn models() -> Vec<CursorModelInfo> {
    vec![CursorModelInfo {
        id: MODEL_COMPOSER.into(),
        label: MODEL_COMPOSER.into(),
    }]
}

#[test]
fn tick_execute_phase_complete_attempt_passes() {
    let _lock = lock_fixtures();
    let dir = tempdir().unwrap();
    let store_mutex = Mutex::new(Store::open_in_memory(dir.path()).unwrap());
    let run_id = Uuid::new_v4();
    {
        let store = store_mutex.lock().unwrap();
        store
            .create_run(run_id, "orchestrator loop", "executing")
            .unwrap();
    }

    let source = repo_root().join("fixtures/intake/executor-app");
    let report = intake::run_preflight(&[source.display().to_string()], IntakeLimits::default())
        .expect("preflight");
    let trusted = intake::apply_trust(report, true, true);
    assert!(trusted.can_start);

    let mut workspace = materialize_run_workspace(MaterializeRequest {
        run_id,
        intake: trusted.manifest.clone(),
        managed_parent: dir.path().to_path_buf(),
        create_internal_worktrees: false,
    })
    .expect("materialize");
    let project = workspace.projects.first().expect("project").clone();
    create_checkpoint(&mut workspace, &project.project_id, "intake-baseline").unwrap();

    let plan = ProjectPlan {
        schema_version: 1,
        run_id,
        title: "Loop fixture".into(),
        summary: "tick → execute → complete".into(),
        assumptions: vec![],
        risks: vec![],
        phases: vec![PhasePlan {
            phase_id: "P01".into(),
            title: "Feature".into(),
            objective: "Pass gates".into(),
            dependencies: vec![],
            project_ids: vec![project.project_id.clone()],
            read_roots: vec![project.write_root.clone()],
            write_roots: vec![project.write_root.clone()],
            model_tier: ModelTier::Composer,
            estimated_minutes: 5,
            acceptance_criteria: vec![AcceptanceCriterion {
                criterion_id: "AC-P01-01".into(),
                description: "done".into(),
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
            prompt: "Implement P01".into(),
            status: PhaseStatus::Draft,
            evidence: vec![],
        }],
        final_gates: vec![],
    };

    let config = SchedulerConfig {
        max_concurrent: 1,
        ..SchedulerConfig::default()
    };
    load_plan_into_scheduler(&store_mutex.lock().unwrap(), &plan, &config).unwrap();

    let started = tick(&store_mutex.lock().unwrap(), run_id, &models(), &config, &[]).unwrap();
    assert_eq!(started.started, vec!["P01".to_string()]);

    let attempt = store_mutex
        .lock()
        .unwrap()
        .list_attempts_for_phase(run_id, "P01")
        .unwrap()
        .into_iter()
        .find(|a| a.status.is_active())
        .expect("running attempt");

    let capability = probe_fake();
    let exe = format!("node|{}", fake_agent_js().display());
    let process_host = ProcessHost::new();
    let mut plan_mut = plan;
    let outcome = execute_phase(ExecutePhaseRequest {
        run_id,
        attempt_id: Some(attempt.attempt_id),
        plan: &mut plan_mut,
        phase_id: "P01",
        workspace: &mut workspace,
        capability: &capability,
        model_id: &attempt.selected_model,
        mode: ExecutionMode::Fresh,
        interruption_report: None,
        resume_chat_id: None,
        executable_override: Some(&exe),
        fake_cli_mode: Some("impl_success"),
        timeout_ms: Some(30_000),
        establish_baseline: true,
        flaky_retry: true,
        host: Some(HostedSpawnContext {
            store: &store_mutex,
            host: &process_host,
        }),
    })
    .expect("execute_phase");

    assert!(outcome.ok, "{}", outcome.message);

    complete_attempt(
        &store_mutex.lock().unwrap(),
        attempt.attempt_id,
        AttemptTerminalResult::Succeeded,
        None,
        false,
    )
    .unwrap();

    let snap = snapshot(&store_mutex.lock().unwrap(), run_id).unwrap();
    let phase = snap
        .phases
        .iter()
        .find(|p| p.phase_id == "P01")
        .expect("P01");
    assert_eq!(phase.status, PhaseRuntimeStatus::Passed);

    // Dependents would unblock on the next tick; with a single phase the run is idle.
    let after = tick(&store_mutex.lock().unwrap(), run_id, &models(), &config, &[]).unwrap();
    assert!(after.started.is_empty());
}
