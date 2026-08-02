use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tiamat_contracts::ProjectPlan;
use uuid::Uuid;

use crate::cursor::CursorModelInfo;
use crate::db::Store;
use crate::scheduler::dag::{
    compute_critical_path_lengths, evaluate_readiness, sort_ready_phases, sorted_lock_names,
    validate_dag, Readiness,
};
use crate::scheduler::error::{SchedulerError, SchedulerResult};
use crate::scheduler::router::{decide_retry, route_model};
use crate::scheduler::types::{
    AttemptRecord, AttemptStatus, AttemptTerminalResult, FailureKind, PhaseRecord,
    PhaseRuntimeStatus, SchedulerConfig, SchedulerSnapshot, TickResult, ORCHESTRATOR_MODE,
};

/// In-memory overlap detector used by concurrent fake-agent tests.
#[derive(Debug, Default)]
pub struct OverlapDetector {
    active_roots: Mutex<HashMap<String, String>>,
    violations: Mutex<Vec<String>>,
}

impl OverlapDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter(&self, phase_id: &str, write_roots: &[String]) -> Result<(), String> {
        let mut guard = self.active_roots.lock().expect("overlap lock");
        for root in write_roots {
            let key = root.to_ascii_lowercase();
            if let Some(other) = guard.get(&key) {
                let msg = format!("write-root overlap: {phase_id} and {other} both hold {root}");
                self.violations
                    .lock()
                    .expect("violations")
                    .push(msg.clone());
                return Err(msg);
            }
        }
        for root in write_roots {
            guard.insert(root.to_ascii_lowercase(), phase_id.to_string());
        }
        Ok(())
    }

    pub fn leave(&self, write_roots: &[String]) {
        let mut guard = self.active_roots.lock().expect("overlap lock");
        for root in write_roots {
            guard.remove(&root.to_ascii_lowercase());
        }
    }

    pub fn violations(&self) -> Vec<String> {
        self.violations.lock().expect("violations").clone()
    }
}

/// Load a validated plan into durable phase rows (idempotent upsert).
pub fn load_plan_into_scheduler(
    store: &Store,
    plan: &ProjectPlan,
    config: &SchedulerConfig,
) -> SchedulerResult<SchedulerSnapshot> {
    validate_dag(plan)?;

    let mut phases: Vec<PhaseRecord> = plan
        .phases
        .iter()
        .map(|p| {
            let mut resource_locks = Vec::new();
            for test in p
                .unit_tests
                .iter()
                .chain(p.integration_tests.iter())
                .chain(p.e2e_tests.iter())
            {
                resource_locks.extend(test.resource_locks.clone());
            }
            resource_locks.sort();
            resource_locks.dedup();
            PhaseRecord {
                run_id: plan.run_id,
                phase_id: p.phase_id.clone(),
                title: p.title.clone(),
                status: PhaseRuntimeStatus::Draft,
                project_ids: p.project_ids.clone(),
                write_roots: p.write_roots.clone(),
                resource_locks,
                dependencies: p.dependencies.clone(),
                model_tier: p.model_tier.clone(),
                estimated_minutes: p.estimated_minutes,
                critical_path_length: 0,
                ready_at_utc: None,
                queued_at_utc: None,
                attempt_count: 0,
                last_failure_kind: None,
            }
        })
        .collect();

    let cpl = compute_critical_path_lengths(&phases);
    for phase in &mut phases {
        phase.critical_path_length = *cpl.get(&phase.phase_id).unwrap_or(&1);
        // Preserve existing runtime status if already loaded.
        if let Ok(Some(existing)) = store.get_phase(plan.run_id, &phase.phase_id) {
            phase.status = existing.status;
            phase.ready_at_utc = existing.ready_at_utc;
            phase.queued_at_utc = existing.queued_at_utc;
            phase.attempt_count = existing.attempt_count;
            phase.last_failure_kind = existing.last_failure_kind;
        }
        store.upsert_phase(phase)?;
    }

    let _lease = store.renew_scheduler_lease(
        plan.run_id,
        &config.lease_holder,
        config.max_concurrent.clamp(1, 4),
        Some(false),
    )?;

    // Move draft → ready/blocked based on deps.
    refresh_readiness(store, plan.run_id)?;

    snapshot(store, plan.run_id)
}

fn refresh_readiness(store: &Store, run_id: Uuid) -> SchedulerResult<()> {
    let phases = store.list_phases(run_id)?;
    let by_id: HashMap<String, PhaseRecord> = phases
        .iter()
        .map(|p| (p.phase_id.clone(), p.clone()))
        .collect();
    let now = chrono::Utc::now().to_rfc3339();

    for phase in &phases {
        if phase.status.is_terminal() || phase.status.is_active() {
            continue;
        }
        match evaluate_readiness(phase, &by_id) {
            Readiness::Ready => {
                if phase.status != PhaseRuntimeStatus::Ready {
                    store.update_phase_status(
                        run_id,
                        &phase.phase_id,
                        PhaseRuntimeStatus::Ready,
                        Some(&now),
                        None,
                        None,
                    )?;
                }
            }
            Readiness::Blocked { reason } => {
                store.update_phase_status(
                    run_id,
                    &phase.phase_id,
                    PhaseRuntimeStatus::Blocked,
                    None,
                    None,
                    Some(&reason),
                )?;
            }
            Readiness::Waiting => {
                if !matches!(
                    phase.status,
                    PhaseRuntimeStatus::Draft | PhaseRuntimeStatus::Blocked
                ) {
                    // Keep draft while waiting on in-flight deps.
                    store.update_phase_status(
                        run_id,
                        &phase.phase_id,
                        PhaseRuntimeStatus::Draft,
                        None,
                        None,
                        None,
                    )?;
                }
            }
            Readiness::Terminal => {}
        }
    }
    Ok(())
}

/// One durable scheduling epoch: renew lease, update readiness, start eligible phases.
pub fn tick(
    store: &Store,
    run_id: Uuid,
    available_models: &[CursorModelInfo],
    config: &SchedulerConfig,
    final_review_phases: &[String],
) -> SchedulerResult<TickResult> {
    let run = store
        .get_run(run_id)?
        .ok_or_else(|| SchedulerError::InvalidState(format!("run {run_id} missing")))?;

    if matches!(
        run.status.as_str(),
        "cancelled" | "cancelling" | "failed" | "completed"
    ) {
        return Ok(TickResult {
            epoch: store
                .get_scheduler_lease(run_id)?
                .map(|l| l.epoch)
                .unwrap_or(0),
            started: vec![],
            blocked: vec![],
            skipped_due_to_pause: false,
            skipped_due_to_capacity: false,
            message: format!("scheduling idle; run status={}", run.status),
        });
    }

    let lease = store.renew_scheduler_lease(
        run_id,
        &config.lease_holder,
        config.max_concurrent.clamp(1, 4),
        None,
    )?;

    if lease.paused || run.status == "paused" {
        refresh_readiness(store, run_id)?;
        return Ok(TickResult {
            epoch: lease.epoch,
            started: vec![],
            blocked: vec![],
            skipped_due_to_pause: true,
            skipped_due_to_capacity: false,
            message: "scheduling paused; active attempts retained".into(),
        });
    }

    if lease.cleanup_incomplete || lease.low_disk {
        return Ok(TickResult {
            epoch: lease.epoch,
            started: vec![],
            blocked: vec![],
            skipped_due_to_pause: false,
            skipped_due_to_capacity: false,
            message: "scheduling blocked: cleanup incomplete or low disk".into(),
        });
    }

    refresh_readiness(store, run_id)?;

    let phases = store.list_phases(run_id)?;
    let ready: Vec<PhaseRecord> = phases
        .iter()
        .filter(|p| p.status == PhaseRuntimeStatus::Ready)
        .cloned()
        .collect();
    let ready = sort_ready_phases(ready);

    let mut started = Vec::new();
    let mut skipped_due_to_capacity = false;

    for phase in ready {
        let active = store.active_attempt_count(run_id)?;
        if active >= lease.max_concurrent {
            skipped_due_to_capacity = true;
            break;
        }

        match try_start_phase(
            store,
            &phase,
            available_models,
            config,
            final_review_phases.contains(&phase.phase_id),
        ) {
            Ok(Some(phase_id)) => started.push(phase_id),
            Ok(None) => {}
            Err(SchedulerError::Lock(_)) => {
                // Contended write/resource lock — leave ready for a later epoch.
            }
            Err(err) => return Err(err),
        }
    }

    refresh_readiness(store, run_id)?;
    let blocked = store
        .list_phases(run_id)?
        .into_iter()
        .filter(|p| p.status == PhaseRuntimeStatus::Blocked)
        .map(|p| p.phase_id)
        .collect();

    Ok(TickResult {
        epoch: lease.epoch,
        started,
        blocked,
        skipped_due_to_pause: false,
        skipped_due_to_capacity,
        message: "scheduling epoch complete".into(),
    })
}

fn try_start_phase(
    store: &Store,
    phase: &PhaseRecord,
    available_models: &[CursorModelInfo],
    config: &SchedulerConfig,
    is_final_review: bool,
) -> SchedulerResult<Option<String>> {
    // Restart idempotency: refuse if an active attempt already exists.
    let existing = store.list_attempts_for_phase(phase.run_id, &phase.phase_id)?;
    if existing.iter().any(|a| a.status.is_active()) {
        return Ok(None);
    }

    let prior_failure = phase.last_failure_kind.as_deref().map(FailureKind::parse);
    let already_same_tier = existing
        .iter()
        .any(|a| a.selection_reason.contains("same-tier resume") && a.terminal_result.is_some());

    let mut requested_tier = phase.model_tier.clone();
    let mut same_tier_resume = false;
    let mut escalation_note: Option<String> = None;
    if let Some(failure) = prior_failure {
        let last_tier = existing
            .last()
            .map(|a| a.requested_tier.clone())
            .unwrap_or_else(|| phase.model_tier.clone());
        let last_selected = existing
            .last()
            .map(|a| {
                crate::scheduler::types::parse_model_tier(if a.selected_model.contains("high") {
                    "grok-high"
                } else if a.selected_model.contains("medium") {
                    "grok-medium"
                } else if a.selected_model.contains("low") {
                    "grok-low"
                } else {
                    crate::scheduler::types::model_tier_str(&a.requested_tier)
                })
            })
            .unwrap_or(last_tier);

        let decision = decide_retry(
            &phase.model_tier,
            &last_selected,
            phase.attempt_count,
            config.max_attempts,
            failure,
            existing.last().map(|a| a.progress_useful).unwrap_or(false),
            already_same_tier,
        );
        if !decision.allow {
            store.update_phase_status(
                phase.run_id,
                &phase.phase_id,
                PhaseRuntimeStatus::Failed,
                None,
                None,
                Some(&decision.reason),
            )?;
            return Ok(None);
        }
        requested_tier = decision.next_tier;
        same_tier_resume = decision.same_tier_resume;
        escalation_note = Some(decision.reason);
    }

    // Escalation is applied above; do not escalate again inside the router.
    let mut selection = route_model(
        &requested_tier,
        available_models,
        phase.attempt_count,
        None,
        is_final_review,
        config.allow_downgrade_before_first_attempt,
        same_tier_resume,
    )?;
    if let Some(note) = escalation_note {
        selection.escalated = true;
        selection.selection_reason = format!("{note}; {}", selection.selection_reason);
    }

    let attempt_number = phase.attempt_count + 1;
    let attempt_id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let parent = existing.last().map(|a| a.attempt_id);

    let locks = sorted_lock_names(&phase.write_roots, &phase.resource_locks);
    // Insert attempt first so unique active constraint guards races, then acquire locks.
    let attempt = AttemptRecord {
        attempt_id,
        run_id: phase.run_id,
        phase_id: phase.phase_id.clone(),
        attempt_number,
        status: AttemptStatus::Starting,
        terminal_result: None,
        requested_tier: selection.requested_tier.clone(),
        requested_model: selection.requested_model.clone(),
        selected_model: selection.selected_model.clone(),
        selection_reason: selection.selection_reason.clone(),
        availability: selection.available_models.clone(),
        resume_parent_attempt_id: parent,
        progress_useful: false,
        failure_kind: None,
        started_at_utc: Some(now.clone()),
        finished_at_utc: None,
    };

    match store.insert_attempt(&attempt) {
        Ok(()) => {}
        Err(crate::db::DbError::Integrity(_)) => {
            // Concurrent start lost the race — idempotent no-op.
            return Ok(None);
        }
        Err(err) => return Err(SchedulerError::Db(err)),
    }

    if let Err(err) = store.acquire_locks(phase.run_id, &phase.phase_id, attempt_id, &locks) {
        // Roll back attempt to completed/cancelled without holding locks.
        let mut failed = attempt.clone();
        failed.status = AttemptStatus::Completed;
        failed.terminal_result = Some(AttemptTerminalResult::Cancelled);
        failed.failure_kind = Some(FailureKind::Other);
        failed.finished_at_utc = Some(chrono::Utc::now().to_rfc3339());
        failed.selection_reason = format!("lock acquisition failed: {err}");
        let _ = store.update_attempt(&failed);
        return Err(SchedulerError::Lock(err.to_string()));
    }

    let mut running = attempt;
    running.status = AttemptStatus::Running;
    store.update_attempt(&running)?;
    store.update_phase_status(
        phase.run_id,
        &phase.phase_id,
        PhaseRuntimeStatus::Running,
        None,
        Some(&now),
        None,
    )?;

    Ok(Some(phase.phase_id.clone()))
}

/// Complete an attempt (fake or real agent). Releases locks and updates phase status.
pub fn complete_attempt(
    store: &Store,
    attempt_id: Uuid,
    result: AttemptTerminalResult,
    failure_kind: Option<FailureKind>,
    progress_useful: bool,
) -> SchedulerResult<PhaseRecord> {
    let attempts = {
        // Scan by listing all runs' attempts would be heavy; look up via SQL helper.
        // We store attempt_id uniquely — fetch via list on all phases is ok for v1 tests.
        let runs = store.list_runs()?;
        let mut found = None;
        for run in runs {
            for attempt in store.list_attempts(run.run_id)? {
                if attempt.attempt_id == attempt_id {
                    found = Some(attempt);
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    };
    let mut attempt = attempts
        .ok_or_else(|| SchedulerError::Attempt(format!("attempt {attempt_id} not found")))?;

    if attempt.status == AttemptStatus::Completed {
        // Idempotent completion.
        return store
            .get_phase(attempt.run_id, &attempt.phase_id)?
            .ok_or_else(|| SchedulerError::InvalidState("phase missing".into()));
    }

    let now = chrono::Utc::now().to_rfc3339();
    attempt.status = AttemptStatus::Completed;
    attempt.terminal_result = Some(result);
    attempt.failure_kind = failure_kind;
    attempt.progress_useful = progress_useful;
    attempt.finished_at_utc = Some(now);
    store.update_attempt(&attempt)?;
    store.release_locks_for_attempt(attempt_id)?;

    let phase_status = match result {
        AttemptTerminalResult::Succeeded => PhaseRuntimeStatus::Passed,
        AttemptTerminalResult::Cancelled => PhaseRuntimeStatus::Cancelled,
        _ => PhaseRuntimeStatus::Failed,
    };
    let failure_str = failure_kind.map(|k| k.as_str().to_string());
    store.update_phase_status(
        attempt.run_id,
        &attempt.phase_id,
        phase_status,
        None,
        None,
        failure_str.as_deref(),
    )?;

    // If failed but retries remain, move back to ready so the next tick can escalate.
    if phase_status == PhaseRuntimeStatus::Failed {
        let phase = store
            .get_phase(attempt.run_id, &attempt.phase_id)?
            .ok_or_else(|| SchedulerError::InvalidState("phase missing".into()))?;
        let last_selected = if attempt.selected_model.contains("high") {
            tiamat_contracts::ModelTier::GrokHigh
        } else if attempt.selected_model.contains("medium") {
            tiamat_contracts::ModelTier::GrokMedium
        } else if attempt.selected_model.contains("low") {
            tiamat_contracts::ModelTier::GrokLow
        } else {
            attempt.requested_tier.clone()
        };
        let decision = decide_retry(
            &phase.model_tier,
            &last_selected,
            phase.attempt_count,
            crate::scheduler::types::DEFAULT_MAX_ATTEMPTS,
            failure_kind.unwrap_or(FailureKind::Other),
            progress_useful,
            attempt.selection_reason.contains("same-tier resume"),
        );
        if decision.allow {
            let ready_at = chrono::Utc::now().to_rfc3339();
            store.update_phase_status(
                attempt.run_id,
                &attempt.phase_id,
                PhaseRuntimeStatus::Ready,
                Some(&ready_at),
                None,
                failure_str.as_deref(),
            )?;
        }
    }

    refresh_readiness(store, attempt.run_id)?;

    store
        .get_phase(attempt.run_id, &attempt.phase_id)?
        .ok_or_else(|| SchedulerError::InvalidState("phase missing after complete".into()))
}

pub fn pause_scheduling(store: &Store, run_id: Uuid) -> SchedulerResult<SchedulerSnapshot> {
    store.set_scheduler_paused(run_id, true)?;
    let _ = store.append_event_atomic(
        Some("paused"),
        crate::db::NewEvent {
            event_id: Uuid::new_v4(),
            run_id,
            project_id: None,
            phase_id: None,
            attempt_id: None,
            process_id: None,
            event_type: "scheduler.paused".into(),
            level: tiamat_contracts::EventLevel::Info,
            timestamp_utc: chrono::Utc::now(),
            message: "Scheduling paused; active attempts retained".into(),
            payload: serde_json::json!({ "paused": true }),
        },
    )?;
    snapshot(store, run_id)
}

pub fn resume_scheduling(store: &Store, run_id: Uuid) -> SchedulerResult<SchedulerSnapshot> {
    store.set_scheduler_paused(run_id, false)?;
    let _ = store.append_event_atomic(
        Some("executing"),
        crate::db::NewEvent {
            event_id: Uuid::new_v4(),
            run_id,
            project_id: None,
            phase_id: None,
            attempt_id: None,
            process_id: None,
            event_type: "scheduler.resumed".into(),
            level: tiamat_contracts::EventLevel::Info,
            timestamp_utc: chrono::Utc::now(),
            message: "Scheduling resumed".into(),
            payload: serde_json::json!({ "paused": false }),
        },
    )?;
    snapshot(store, run_id)
}

pub fn snapshot(store: &Store, run_id: Uuid) -> SchedulerResult<SchedulerSnapshot> {
    let lease = store.get_scheduler_lease(run_id)?;
    Ok(SchedulerSnapshot {
        run_id,
        mode: ORCHESTRATOR_MODE.into(),
        paused: lease.as_ref().map(|l| l.paused).unwrap_or(false),
        epoch: lease.as_ref().map(|l| l.epoch).unwrap_or(0),
        max_concurrent: lease.as_ref().map(|l| l.max_concurrent).unwrap_or(3),
        active_attempts: store.active_attempt_count(run_id)?,
        phases: store.list_phases(run_id)?,
        attempts: store.list_attempts(run_id)?,
        held_locks: store.list_held_locks(run_id)?,
    })
}

/// Run fake agents concurrently for currently running attempts (integration tests).
pub fn run_fake_agents_concurrent(
    store: &Store,
    run_id: Uuid,
    detector: Arc<OverlapDetector>,
    succeed: bool,
    hold_ms: u64,
) -> SchedulerResult<Vec<Uuid>> {
    let attempts: Vec<_> = store
        .list_attempts(run_id)?
        .into_iter()
        .filter(|a| a.status == AttemptStatus::Running)
        .collect();
    let phases: HashMap<_, _> = store
        .list_phases(run_id)?
        .into_iter()
        .map(|p| (p.phase_id.clone(), p))
        .collect();

    let mut handles = Vec::new();
    let completed = Arc::new(Mutex::new(Vec::new()));

    for attempt in attempts {
        let phase = phases
            .get(&attempt.phase_id)
            .cloned()
            .ok_or_else(|| SchedulerError::InvalidState("phase missing for attempt".into()))?;
        let detector = Arc::clone(&detector);
        let completed = Arc::clone(&completed);
        let attempt_id = attempt.attempt_id;
        let roots = phase.write_roots.clone();
        let phase_id = phase.phase_id.clone();

        handles.push(std::thread::spawn(move || {
            detector.enter(&phase_id, &roots).expect("no overlap");
            std::thread::sleep(std::time::Duration::from_millis(hold_ms));
            detector.leave(&roots);
            completed.lock().expect("completed").push(attempt_id);
        }));
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| SchedulerError::InvalidState("fake agent thread panicked".into()))?;
    }

    let ids = completed.lock().expect("completed").clone();
    for id in &ids {
        let result = if succeed {
            AttemptTerminalResult::Succeeded
        } else {
            AttemptTerminalResult::Failed
        };
        let failure = if succeed {
            None
        } else {
            Some(FailureKind::TestFailure)
        };
        complete_attempt(store, *id, result, failure, false)?;
    }
    Ok(ids)
}
