//! Concurrent fake-agent integration tests for the durable DAG scheduler.

use std::sync::Arc;

use tempfile::tempdir;
use tiamat_contracts::{
    AcceptanceCriterion, FinalGate, ModelTier, PhasePlan, PhaseStatus, ProjectPlan, RollbackSpec,
    RollbackStrategy, TestExpected, TestKind, TestSpec,
};
use tiamat_lib::cursor::CursorModelInfo;
use tiamat_lib::db::Store;
use tiamat_lib::scheduler::{
    complete_attempt, load_plan_into_scheduler, pause_scheduling, resume_scheduling,
    run_fake_agents_concurrent, snapshot, tick, AttemptTerminalResult, FailureKind,
    OverlapDetector, PhaseRuntimeStatus, SchedulerConfig, MODEL_COMPOSER, MODEL_GROK_HIGH,
    MODEL_GROK_LOW, MODEL_GROK_MEDIUM, MODEL_SOL,
};
use uuid::Uuid;

fn models() -> Vec<CursorModelInfo> {
    [
        MODEL_COMPOSER,
        MODEL_GROK_LOW,
        MODEL_GROK_MEDIUM,
        MODEL_GROK_HIGH,
        MODEL_SOL,
    ]
    .into_iter()
    .map(|id| CursorModelInfo {
        id: id.into(),
        label: id.into(),
    })
    .collect()
}

fn multi_repo_plan(run_id: Uuid) -> ProjectPlan {
    let phase = |id: &str, deps: &[&str], root: &str, tier: ModelTier| PhasePlan {
        phase_id: id.into(),
        title: id.into(),
        objective: format!("{id} objective"),
        dependencies: deps.iter().map(|s| (*s).to_string()).collect(),
        project_ids: vec![root.into()],
        read_roots: vec![format!("C:\\managed\\{root}")],
        write_roots: vec![format!("C:\\managed\\{root}")],
        model_tier: tier,
        estimated_minutes: 5,
        acceptance_criteria: vec![AcceptanceCriterion {
            criterion_id: format!("AC-{id}"),
            description: "pass".into(),
            required_evidence_kinds: vec![TestKind::Unit],
        }],
        unit_tests: vec![TestSpec {
            test_id: format!("UT-{id}"),
            command: vec!["echo".into(), "ok".into()],
            working_directory: ".".into(),
            timeout_seconds: 30,
            resource_locks: vec![],
            expected: TestExpected {
                exit_code: 0,
                artifacts: vec![],
            },
            covers: vec![format!("AC-{id}")],
            inapplicable_reason: None,
        }],
        integration_tests: vec![],
        e2e_tests: vec![],
        manual_checks: vec![],
        rollback: RollbackSpec {
            checkpoint: "baseline".into(),
            strategy: RollbackStrategy::Restore,
        },
        expected_artifacts: vec![],
        prompt: format!("Do {id}"),
        status: PhaseStatus::Draft,
        evidence: vec![],
    };

    ProjectPlan {
        schema_version: 1,
        run_id,
        title: "Concurrent scheduler fixture".into(),
        summary: "parallel across repos".into(),
        assumptions: vec![],
        risks: vec![],
        phases: vec![
            phase("P01", &[], "repo-a", ModelTier::Composer),
            phase("P02", &[], "repo-b", ModelTier::Composer),
            phase("P03", &["P01"], "repo-a", ModelTier::GrokMedium),
            phase("P04", &[], "repo-c", ModelTier::Composer),
        ],
        final_gates: vec![FinalGate {
            gate_id: "FG-01".into(),
            description: "final".into(),
            dependencies: vec!["P03".into(), "P04".into()],
            required_evidence_kinds: vec![TestKind::Review],
        }],
    }
}

#[test]
fn concurrent_fake_agents_no_write_root_overlap() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("t.db"), dir.path().join("a")).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "concurrent", "executing").unwrap();
    let plan = multi_repo_plan(run_id);
    let config = SchedulerConfig {
        max_concurrent: 3,
        ..SchedulerConfig::default()
    };
    load_plan_into_scheduler(&store, &plan, &config).unwrap();
    let tick_result = tick(&store, run_id, &models(), &config, &[]).unwrap();
    assert!(tick_result.started.len() >= 2);
    assert!(tick_result.started.contains(&"P01".into()));
    assert!(tick_result.started.contains(&"P02".into()));

    let detector = Arc::new(OverlapDetector::new());
    run_fake_agents_concurrent(&store, run_id, Arc::clone(&detector), true, 40).unwrap();
    assert!(
        detector.violations().is_empty(),
        "overlap violations: {:?}",
        detector.violations()
    );

    let snap = snapshot(&store, run_id).unwrap();
    assert_eq!(snap.active_attempts, 0);
    assert!(snap.held_locks.is_empty());
    assert!(snap
        .phases
        .iter()
        .filter(|p| p.phase_id == "P01" || p.phase_id == "P02")
        .all(|p| p.status == PhaseRuntimeStatus::Passed));
}

#[test]
fn restart_idempotency_does_not_duplicate_attempts() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("t.db");
    let artifacts = dir.path().join("a");
    let run_id = Uuid::new_v4();
    let plan = multi_repo_plan(run_id);
    let config = SchedulerConfig {
        max_concurrent: 3,
        ..SchedulerConfig::default()
    };

    {
        let store = Store::open(&db, &artifacts).unwrap();
        store.create_run(run_id, "idempotent", "executing").unwrap();
        load_plan_into_scheduler(&store, &plan, &config).unwrap();
        tick(&store, run_id, &models(), &config, &[]).unwrap();
        assert_eq!(store.list_attempts(run_id).unwrap().len(), 3);
    }

    // Simulate process restart: reopen store, reload plan, tick again.
    let store = Store::open(&db, &artifacts).unwrap();
    load_plan_into_scheduler(&store, &plan, &config).unwrap();
    tick(&store, run_id, &models(), &config, &[]).unwrap();
    assert_eq!(
        store.list_attempts(run_id).unwrap().len(),
        3,
        "restart must not duplicate active attempts"
    );
    assert_eq!(store.active_attempt_count(run_id).unwrap(), 3);
}

#[test]
fn pause_resume_and_blocked_escalated_states() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("t.db"), dir.path().join("a")).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "states", "executing").unwrap();
    let plan = multi_repo_plan(run_id);
    let config = SchedulerConfig {
        max_concurrent: 2,
        ..SchedulerConfig::default()
    };
    load_plan_into_scheduler(&store, &plan, &config).unwrap();

    // Parallel start across distinct repos.
    let t1 = tick(&store, run_id, &models(), &config, &[]).unwrap();
    assert_eq!(t1.started.len(), 2);
    assert!(t1.started.contains(&"P01".into()));
    assert!(t1.started.contains(&"P02".into()));

    // Pause prevents additional starts.
    pause_scheduling(&store, run_id).unwrap();
    let paused = tick(&store, run_id, &models(), &config, &[]).unwrap();
    assert!(paused.skipped_due_to_pause);
    resume_scheduling(&store, run_id).unwrap();

    // Finish P02 successfully so it does not consume capacity during P01 escalation.
    let p02 = store
        .list_attempts(run_id)
        .unwrap()
        .into_iter()
        .find(|a| a.phase_id == "P02")
        .unwrap();
    complete_attempt(
        &store,
        p02.attempt_id,
        AttemptTerminalResult::Succeeded,
        None,
        false,
    )
    .unwrap();

    // Fail P01 with timeout → next tick escalates Composer → Grok Low.
    let p01 = store
        .list_attempts(run_id)
        .unwrap()
        .into_iter()
        .find(|a| a.phase_id == "P01")
        .unwrap();
    complete_attempt(
        &store,
        p01.attempt_id,
        AttemptTerminalResult::TimedOut,
        Some(FailureKind::Timeout),
        false,
    )
    .unwrap();

    tick(&store, run_id, &models(), &config, &[]).unwrap();
    let attempts = store.list_attempts_for_phase(run_id, "P01").unwrap();
    assert!(
        attempts.len() >= 2,
        "expected escalated retry, got {} attempts",
        attempts.len()
    );
    assert_eq!(attempts[1].selected_model, MODEL_GROK_LOW);
    assert!(
        attempts[1]
            .selection_reason
            .to_ascii_lowercase()
            .contains("escalat"),
        "reason={}",
        attempts[1].selection_reason
    );

    // Deterministic policy failure ends retries and blocks dependents.
    let p01b = attempts
        .iter()
        .find(|a| a.status == tiamat_lib::scheduler::AttemptStatus::Running)
        .expect("escalated attempt should be running");
    complete_attempt(
        &store,
        p01b.attempt_id,
        AttemptTerminalResult::Failed,
        Some(FailureKind::Policy),
        false,
    )
    .unwrap();
    let snap = snapshot(&store, run_id).unwrap();
    assert_eq!(
        snap.phases
            .iter()
            .find(|p| p.phase_id == "P01")
            .unwrap()
            .status,
        PhaseRuntimeStatus::Failed
    );
    assert_eq!(
        snap.phases
            .iter()
            .find(|p| p.phase_id == "P03")
            .unwrap()
            .status,
        PhaseRuntimeStatus::Blocked
    );
}
