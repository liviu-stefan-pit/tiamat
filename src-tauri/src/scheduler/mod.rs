//! Durable DAG scheduler, locks, model router, attempts, and pause/resume.

mod dag;
mod engine;
mod error;
mod router;
mod types;

pub use dag::{
    compute_critical_path_lengths, evaluate_readiness, sort_ready_phases, sorted_lock_names,
    validate_dag, Readiness,
};
pub use engine::{
    complete_attempt, load_plan_into_scheduler, pause_scheduling, resume_scheduling,
    run_fake_agents_concurrent, snapshot, tick, OverlapDetector,
};
pub use error::{SchedulerError, SchedulerResult};
pub use router::{decide_retry, route_model, RetryDecision};
pub use types::*;

/// Back-compat status surface used by P00 shell; now reports the real scheduler mode.
#[derive(Debug, Clone)]
pub struct OrchestratorStatus {
    pub mode: String,
    pub active_runs: u32,
    pub message: String,
}

pub struct DagOrchestrator;

impl DagOrchestrator {
    pub const MODE: &'static str = ORCHESTRATOR_MODE;

    pub fn status() -> OrchestratorStatus {
        Self::status_with(0, false)
    }

    pub fn status_with(active_runs: u32, paused: bool) -> OrchestratorStatus {
        OrchestratorStatus {
            mode: Self::MODE.to_string(),
            active_runs,
            message: if paused {
                "DAG scheduler paused; active attempts retained.".to_string()
            } else {
                "Durable DAG scheduler ready.".to_string()
            },
        }
    }
}

/// Deprecated alias kept so older call sites compile during the P06 cutover.
pub type FakeOrchestrator = DagOrchestrator;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cursor::CursorModelInfo;
    use crate::db::Store;
    use tempfile::tempdir;
    use tiamat_contracts::{
        AcceptanceCriterion, FinalGate, ModelTier, PhasePlan, PhaseStatus, ProjectPlan,
        RollbackSpec, RollbackStrategy, TestExpected, TestKind, TestSpec,
    };
    use uuid::Uuid;

    fn model(id: &str) -> CursorModelInfo {
        CursorModelInfo {
            id: id.into(),
            label: id.into(),
        }
    }

    fn available() -> Vec<CursorModelInfo> {
        vec![
            model(MODEL_COMPOSER),
            model(MODEL_GROK_LOW),
            model(MODEL_GROK_MEDIUM),
            model(MODEL_GROK_HIGH),
            model(MODEL_SOL),
        ]
    }

    fn sample_plan(run_id: Uuid) -> ProjectPlan {
        let phase = |id: &str, deps: &[&str], root: &str, tier: ModelTier| PhasePlan {
            phase_id: id.into(),
            title: id.into(),
            objective: format!("objective {id}"),
            dependencies: deps.iter().map(|s| (*s).to_string()).collect(),
            project_ids: vec![root.into()],
            read_roots: vec![format!("C:\\managed\\{root}")],
            write_roots: vec![format!("C:\\managed\\{root}")],
            model_tier: tier,
            estimated_minutes: 10,
            acceptance_criteria: vec![AcceptanceCriterion {
                criterion_id: format!("AC-{id}-01"),
                description: "ok".into(),
                required_evidence_kinds: vec![TestKind::Unit],
            }],
            unit_tests: vec![TestSpec {
                test_id: format!("UT-{id}-01"),
                command: vec!["npm".into(), "test".into()],
                working_directory: ".".into(),
                timeout_seconds: 60,
                resource_locks: if id == "P01" {
                    vec!["port:3000".into()]
                } else {
                    vec![]
                },
                expected: TestExpected {
                    exit_code: 0,
                    artifacts: vec![],
                },
                covers: vec![format!("AC-{id}-01")],
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
            prompt: format!("Implement {id}"),
            status: PhaseStatus::Draft,
            evidence: vec![],
        };

        ProjectPlan {
            schema_version: 1,
            run_id,
            title: "Scheduler fixture".into(),
            summary: "multi-repo schedule".into(),
            assumptions: vec![],
            risks: vec![],
            phases: vec![
                phase("P01", &[], "repo-a", ModelTier::Composer),
                phase("P02", &[], "repo-b", ModelTier::Composer),
                phase("P03", &["P01"], "repo-a", ModelTier::GrokLow),
                phase("P04", &["P02"], "repo-b", ModelTier::Composer),
            ],
            final_gates: vec![FinalGate {
                gate_id: "FG-01".into(),
                description: "review".into(),
                dependencies: vec!["P03".into(), "P04".into()],
                required_evidence_kinds: vec![TestKind::Review],
            }],
        }
    }

    #[test]
    fn loads_plan_marks_roots_ready_and_respects_concurrency() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("artifacts")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "sched", "executing").unwrap();
        let plan = sample_plan(run_id);
        let config = SchedulerConfig {
            max_concurrent: 2,
            ..SchedulerConfig::default()
        };

        let snap = load_plan_into_scheduler(&store, &plan, &config).unwrap();
        assert_eq!(snap.phases.len(), 4);
        assert!(snap
            .phases
            .iter()
            .filter(|p| p.phase_id == "P01" || p.phase_id == "P02")
            .all(|p| p.status == PhaseRuntimeStatus::Ready));

        let tick1 = tick(&store, run_id, &available(), &config, &[]).unwrap();
        assert_eq!(tick1.started.len(), 2);
        assert!(tick1.started.contains(&"P01".into()));
        assert!(tick1.started.contains(&"P02".into()));

        let snap = snapshot(&store, run_id).unwrap();
        assert_eq!(snap.active_attempts, 2);
        let locks = snap.held_locks;
        assert!(locks.iter().any(|l| l.contains("repo-a")));
        assert!(locks.iter().any(|l| l.contains("repo-b")));
        assert!(locks.iter().any(|l| l.contains("port:3000")));
    }

    #[test]
    fn same_repo_writers_do_not_overlap() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("artifacts")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "sched", "executing").unwrap();
        let mut plan = sample_plan(run_id);
        // Force P02 onto same write root as P01.
        plan.phases[1].write_roots = plan.phases[0].write_roots.clone();
        plan.phases[1].project_ids = plan.phases[0].project_ids.clone();
        let config = SchedulerConfig {
            max_concurrent: 4,
            ..SchedulerConfig::default()
        };

        load_plan_into_scheduler(&store, &plan, &config).unwrap();
        let tick1 = tick(&store, run_id, &available(), &config, &[]).unwrap();
        assert_eq!(tick1.started.len(), 1);
        let snap = snapshot(&store, run_id).unwrap();
        assert_eq!(snap.active_attempts, 1);
    }

    #[test]
    fn pause_blocks_new_starts_resume_continues() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("artifacts")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "sched", "executing").unwrap();
        let plan = sample_plan(run_id);
        let config = SchedulerConfig {
            max_concurrent: 1,
            ..SchedulerConfig::default()
        };
        load_plan_into_scheduler(&store, &plan, &config).unwrap();
        tick(&store, run_id, &available(), &config, &[]).unwrap();
        pause_scheduling(&store, run_id).unwrap();
        let paused_tick = tick(&store, run_id, &available(), &config, &[]).unwrap();
        assert!(paused_tick.skipped_due_to_pause);
        assert!(paused_tick.started.is_empty());
        resume_scheduling(&store, run_id).unwrap();
        // Complete the active one so capacity frees.
        let active = store
            .list_attempts(run_id)
            .unwrap()
            .into_iter()
            .find(|a| a.status == AttemptStatus::Running)
            .unwrap();
        complete_attempt(
            &store,
            active.attempt_id,
            AttemptTerminalResult::Succeeded,
            None,
            false,
        )
        .unwrap();
        let resumed = tick(&store, run_id, &available(), &config, &[]).unwrap();
        assert!(!resumed.skipped_due_to_pause);
        assert_eq!(resumed.started.len(), 1);
    }

    #[test]
    fn failed_dependency_blocks_dependents() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("artifacts")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "sched", "executing").unwrap();
        let plan = sample_plan(run_id);
        let config = SchedulerConfig {
            max_concurrent: 2,
            ..SchedulerConfig::default()
        };
        load_plan_into_scheduler(&store, &plan, &config).unwrap();
        tick(&store, run_id, &available(), &config, &[]).unwrap();
        let p01 = store
            .list_attempts(run_id)
            .unwrap()
            .into_iter()
            .find(|a| a.phase_id == "P01")
            .unwrap();
        complete_attempt(
            &store,
            p01.attempt_id,
            AttemptTerminalResult::Failed,
            Some(FailureKind::Policy),
            false,
        )
        .unwrap();
        // Policy failure should not retry — phase stays failed; P03 blocked.
        let snap = snapshot(&store, run_id).unwrap();
        let p01_phase = snap.phases.iter().find(|p| p.phase_id == "P01").unwrap();
        assert_eq!(p01_phase.status, PhaseRuntimeStatus::Failed);
        let p03 = snap.phases.iter().find(|p| p.phase_id == "P03").unwrap();
        assert_eq!(p03.status, PhaseRuntimeStatus::Blocked);
    }

    #[test]
    fn escalation_persists_model_reason() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("artifacts")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "sched", "executing").unwrap();
        let mut plan = sample_plan(run_id);
        plan.phases.truncate(1);
        let config = SchedulerConfig::default();
        load_plan_into_scheduler(&store, &plan, &config).unwrap();
        tick(&store, run_id, &available(), &config, &[]).unwrap();
        let a1 = store.list_attempts(run_id).unwrap().remove(0);
        assert_eq!(a1.selected_model, MODEL_COMPOSER);
        complete_attempt(
            &store,
            a1.attempt_id,
            AttemptTerminalResult::TimedOut,
            Some(FailureKind::Timeout),
            false,
        )
        .unwrap();
        tick(&store, run_id, &available(), &config, &[]).unwrap();
        let attempts = store.list_attempts(run_id).unwrap();
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[1].selected_model, MODEL_GROK_LOW);
        assert!(attempts[1].selection_reason.contains("escalat"));
    }

    #[test]
    fn restart_does_not_duplicate_active_attempt() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db");
        let artifacts = dir.path().join("artifacts");
        let run_id = Uuid::new_v4();
        let plan = sample_plan(run_id);
        let config = SchedulerConfig {
            max_concurrent: 2,
            ..SchedulerConfig::default()
        };

        {
            let store = Store::open(&db, &artifacts).unwrap();
            store.create_run(run_id, "sched", "executing").unwrap();
            load_plan_into_scheduler(&store, &plan, &config).unwrap();
            tick(&store, run_id, &available(), &config, &[]).unwrap();
            assert_eq!(store.active_attempt_count(run_id).unwrap(), 2);
        }

        let store = Store::open(&db, &artifacts).unwrap();
        load_plan_into_scheduler(&store, &plan, &config).unwrap();
        tick(&store, run_id, &available(), &config, &[]).unwrap();
        assert_eq!(store.list_attempts(run_id).unwrap().len(), 2);
        assert_eq!(store.active_attempt_count(run_id).unwrap(), 2);
    }

    #[test]
    fn orchestrator_mode_is_dag_scheduler() {
        let status = DagOrchestrator::status_with(0, false);
        assert_eq!(status.mode, ORCHESTRATOR_MODE);
    }
}
