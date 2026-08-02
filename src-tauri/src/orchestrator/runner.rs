//! Supervisor thread that drives a full run end-to-end.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tiamat_contracts::EventLevel;
use uuid::Uuid;

use crate::app::commands::{AppState, EVENT_CHANNEL};
use crate::cursor::{probe_cursor_capability, TimeoutSettings};
use crate::db::NewEvent;
use crate::executor::{execute_phase, ExecutePhaseRequest, ExecutionMode};
use crate::intake::{self, IntakeLimits};
use crate::planner::{run_architect_pipeline, ArchitectPipelineRequest};
use crate::process::HostedSpawnContext;
use crate::scheduler::{
    complete_attempt, load_plan_into_scheduler, tick, AttemptTerminalResult, FailureKind,
    PhaseRuntimeStatus, SchedulerConfig,
};
use crate::security::redact_line;
use crate::workspace::{materialize_run_workspace, MaterializeRequest};

use super::types::{RunStatusSnapshot, StartRunRequest, StartRunResult};

/// Tracks the active supervisor so cancel/status can find it.
pub struct OrchestratorHandle {
    cancel: Arc<AtomicBool>,
    run_id: Uuid,
    join: Option<JoinHandle<()>>,
}

impl OrchestratorHandle {
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Shared slot on AppState for the active orchestrator.
pub type OrchestratorSlot = Mutex<Option<OrchestratorHandle>>;

/// Start a full run on a background supervisor thread.
pub fn start_run(app: AppHandle, request: StartRunRequest) -> Result<StartRunResult, String> {
    if request.input_paths.is_empty() {
        return Err("input_paths must not be empty".into());
    }
    let output_dir = PathBuf::from(request.output_dir.trim());
    if output_dir.as_os_str().is_empty() {
        return Err("output_dir must not be empty".into());
    }
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("cannot create output_dir: {e}"))?;

    let state = app.state::<AppState>();
    {
        let mut slot = state.orchestrator.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = slot.as_ref() {
            // Allow restart only when the previous supervisor has finished.
            if existing.join.as_ref().is_some_and(|j| !j.is_finished()) {
                return Err(format!("a run is already active ({})", existing.run_id()));
            }
        }
        *slot = None;
    }

    // Preflight on the calling thread so we can fail fast before spawning.
    let preflight = intake::run_preflight(&request.input_paths, IntakeLimits::default())
        .map_err(|e| e.to_string())?;
    if !preflight.blockers.is_empty() && !preflight.can_start {
        // Trust not yet acknowledged — still allow start if the UI confirmed trust.
        // Callers that need trust must call confirm_intake_trust first.
    }
    *state.last_preflight.lock().map_err(|e| e.to_string())? = Some(preflight.clone());

    let run_id = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let id = Uuid::new_v4();
        store
            .create_run(id, "tiamat-run", "created")
            .map_err(|e| e.to_string())?;
        id
    };

    *state.workspace_parent.lock().map_err(|e| e.to_string())? = Some(output_dir.clone());

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = Arc::clone(&cancel);
    let app_for_thread = app.clone();
    let fake_mode = request.fake_cli_mode.clone();
    let max_concurrent = request.max_concurrent.unwrap_or(2).clamp(1, 4);

    let join = thread::Builder::new()
        .name(format!("tiamat-run-{run_id}"))
        .spawn(move || {
            if let Err(err) = run_supervisor(
                app_for_thread,
                run_id,
                preflight,
                output_dir,
                max_concurrent,
                fake_mode,
                cancel_for_thread,
            ) {
                eprintln!("tiamat orchestrator failed: {err}");
            }
        })
        .map_err(|e| format!("failed to spawn orchestrator: {e}"))?;

    *state.orchestrator.lock().map_err(|e| e.to_string())? = Some(OrchestratorHandle {
        cancel,
        run_id,
        join: Some(join),
    });

    Ok(StartRunResult {
        run_id,
        status: "started".into(),
        message: "Run started; architect and phase agents will execute in order".into(),
        managed_run_root: None,
    })
}

pub fn cancel_active_run(app: &AppHandle) -> Result<RunStatusSnapshot, String> {
    let state = app.state::<AppState>();
    let mut slot = state.orchestrator.lock().map_err(|e| e.to_string())?;
    let Some(handle) = slot.as_mut() else {
        return Ok(RunStatusSnapshot {
            run_id: None,
            status: "idle".into(),
            phase: None,
            message: "no active run".into(),
            active_attempts: 0,
            completed_phases: 0,
            total_phases: 0,
            managed_run_root: None,
        });
    };
    handle.request_cancel();
    let run_id = handle.run_id();
    drop(slot);

    let store = state.store.lock().map_err(|e| e.to_string())?;
    let _ = store.set_run_status(run_id, "cancelling");
    state.process_host.cancel_all_for_run(run_id, false);

    snapshot_from_store(&store, run_id, &state)
}

pub fn get_run_status(app: &AppHandle) -> Result<RunStatusSnapshot, String> {
    let state = app.state::<AppState>();
    let run_id = {
        let slot = state.orchestrator.lock().map_err(|e| e.to_string())?;
        slot.as_ref().map(|h| h.run_id())
    };
    let store = state.store.lock().map_err(|e| e.to_string())?;
    if let Some(run_id) = run_id {
        return snapshot_from_store(&store, run_id, &state);
    }
    // Fall back to the most recent non-terminal or latest run.
    let runs = store.list_runs().map_err(|e| e.to_string())?;
    if let Some(run) = runs.into_iter().next_back() {
        return snapshot_from_store(&store, run.run_id, &state);
    }
    Ok(RunStatusSnapshot {
        run_id: None,
        status: "idle".into(),
        phase: None,
        message: "no runs".into(),
        active_attempts: 0,
        completed_phases: 0,
        total_phases: 0,
        managed_run_root: None,
    })
}

fn snapshot_from_store(
    store: &crate::db::Store,
    run_id: Uuid,
    state: &AppState,
) -> Result<RunStatusSnapshot, String> {
    let run = store
        .get_run(run_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("run {run_id} missing"))?;
    let phases = store.list_phases(run_id).unwrap_or_default();
    let completed = phases
        .iter()
        .filter(|p| {
            matches!(
                p.status,
                PhaseRuntimeStatus::Passed
                    | PhaseRuntimeStatus::Cancelled
                    | PhaseRuntimeStatus::Skipped
            )
        })
        .count() as u32;
    let active = store.active_attempt_count(run_id).unwrap_or(0);
    let current = phases
        .iter()
        .find(|p| p.status == PhaseRuntimeStatus::Running)
        .map(|p| p.phase_id.clone());
    let managed = state
        .last_workspace
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|w| w.managed_run_root.clone()));
    Ok(RunStatusSnapshot {
        run_id: Some(run_id),
        status: run.status,
        phase: current,
        message: run.title,
        active_attempts: active,
        completed_phases: completed,
        total_phases: phases.len() as u32,
        managed_run_root: managed,
    })
}

fn run_supervisor(
    app: AppHandle,
    run_id: Uuid,
    preflight: crate::intake::PreflightReport,
    output_dir: PathBuf,
    max_concurrent: u32,
    fake_cli_mode: Option<String>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    emit_run_event(
        &app,
        run_id,
        "run.started",
        EventLevel::Info,
        "Orchestrator started".into(),
        serde_json::json!({ "runId": run_id, "outputDir": output_dir }),
    )?;

    if cancel.load(Ordering::SeqCst) {
        return mark_cancelled(&app, run_id);
    }

    // --- Materialize into the user-chosen output directory ---
    set_run_status(&app, run_id, "preflighting")?;
    let mut workspace = {
        let state = app.state::<AppState>();
        let manifest = materialize_run_workspace(MaterializeRequest {
            run_id,
            intake: preflight.manifest.clone(),
            managed_parent: output_dir,
            create_internal_worktrees: true,
        })
        .map_err(|e| e.to_string())?;
        *state.last_workspace.lock().map_err(|e| e.to_string())? = Some(manifest.clone());
        emit_run_event(
            &app,
            run_id,
            "workspace.materialized",
            EventLevel::Info,
            format!("Workspace ready at {}", manifest.managed_run_root),
            serde_json::json!({ "managedRunRoot": manifest.managed_run_root }),
        )?;
        manifest
    };

    if cancel.load(Ordering::SeqCst) {
        return mark_cancelled(&app, run_id);
    }

    // --- Architect ---
    set_run_status(&app, run_id, "planning")?;
    let capability = {
        let state = app.state::<AppState>();
        let report = {
            let guard = state.last_cursor.lock().map_err(|e| e.to_string())?;
            if let Some(cached) = guard.clone() {
                cached
            } else {
                drop(guard);
                let report = probe_cursor_capability();
                *state.last_cursor.lock().map_err(|e| e.to_string())? = Some(report.clone());
                report
            }
        };
        report
    };

    let architect = {
        let state = app.state::<AppState>();
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let result = run_architect_pipeline(ArchitectPipelineRequest {
            run_id,
            preflight: &preflight,
            workspace: &mut workspace,
            capability: &capability,
            executable_override: None,
            fake_cli_mode: None,
            host: Some(HostedSpawnContext {
                store: &store,
                host: &state.process_host,
            }),
        });
        drop(store);
        *state.last_workspace.lock().map_err(|e| e.to_string())? = Some(workspace.clone());
        *state.last_architect.lock().map_err(|e| e.to_string())? = Some(result.clone());
        if let Some(plan) = result.plan.clone() {
            *state.last_plan.lock().map_err(|e| e.to_string())? = Some(plan);
        }
        result
    };

    if !architect.ok {
        set_run_status(&app, run_id, "failed")?;
        emit_run_event(
            &app,
            run_id,
            "plan.failed",
            EventLevel::Error,
            architect
                .error
                .clone()
                .unwrap_or_else(|| "architect failed".into()),
            serde_json::json!({ "runId": run_id }),
        )?;
        return Err(architect.error.unwrap_or_else(|| "architect failed".into()));
    }

    let plan = architect
        .plan
        .clone()
        .ok_or_else(|| "architect succeeded without a plan".to_string())?;
    emit_run_event(
        &app,
        run_id,
        "plan.compiled",
        EventLevel::Info,
        format!("Plan compiled with {} phase(s)", plan.phases.len()),
        serde_json::json!({ "phaseCount": plan.phases.len() }),
    )?;

    if cancel.load(Ordering::SeqCst) {
        return mark_cancelled(&app, run_id);
    }

    // --- Load scheduler + execute loop ---
    set_run_status(&app, run_id, "executing")?;
    let available_models = capability.models.clone();
    let config = SchedulerConfig {
        max_concurrent,
        ..SchedulerConfig::default()
    };
    let final_review: Vec<String> = plan.final_gates.iter().map(|g| g.gate_id.clone()).collect();
    let timeouts = TimeoutSettings::from_env();
    {
        let state = app.state::<AppState>();
        let store = state.store.lock().map_err(|e| e.to_string())?;
        load_plan_into_scheduler(&store, &plan, &config).map_err(|e| e.to_string())?;
        let snap = crate::scheduler::snapshot(&store, run_id).map_err(|e| e.to_string())?;
        *state.last_scheduler.lock().map_err(|e| e.to_string())? = Some(snap);
    }

    // attempt_id -> worker handle
    let mut workers: HashMap<Uuid, JoinHandle<WorkerOutcome>> = HashMap::new();

    loop {
        if cancel.load(Ordering::SeqCst) {
            // Cancel in-flight workers by aborting the process host, then join.
            {
                let state = app.state::<AppState>();
                state.process_host.cancel_all_for_run(run_id, false);
            }
            for (_, handle) in workers.drain() {
                let _ = handle.join();
            }
            return mark_cancelled(&app, run_id);
        }

        // Harvest finished workers.
        let finished: Vec<Uuid> = workers
            .iter()
            .filter(|(_, h)| h.is_finished())
            .map(|(id, _)| *id)
            .collect();
        for attempt_id in finished {
            if let Some(handle) = workers.remove(&attempt_id) {
                match handle.join() {
                    Ok(outcome) => apply_worker_outcome(&app, outcome)?,
                    Err(_) => {
                        let _ = complete_attempt_safe(
                            &app,
                            attempt_id,
                            AttemptTerminalResult::Failed,
                            Some(FailureKind::Other),
                            false,
                        );
                    }
                }
            }
        }

        // Tick the scheduler for newly-ready phases.
        let started_phases = {
            let state = app.state::<AppState>();
            let store = state.store.lock().map_err(|e| e.to_string())?;
            let tick_result = tick(&store, run_id, &available_models, &config, &final_review)
                .map_err(|e| e.to_string())?;
            let snap = crate::scheduler::snapshot(&store, run_id).map_err(|e| e.to_string())?;
            *state.last_scheduler.lock().map_err(|e| e.to_string())? = Some(snap);
            emit_run_event(
                &app,
                run_id,
                "scheduler.tick",
                EventLevel::Info,
                tick_result.message.clone(),
                serde_json::json!({
                    "started": tick_result.started,
                    "blocked": tick_result.blocked,
                }),
            )?;
            tick_result.started
        };

        // Spawn workers for newly started phases that we are not already tracking.
        for phase_id in started_phases {
            let (attempt_id, model_id) = {
                let state = app.state::<AppState>();
                let store = state.store.lock().map_err(|e| e.to_string())?;
                let attempts = store
                    .list_attempts_for_phase(run_id, &phase_id)
                    .map_err(|e| e.to_string())?;
                let active = attempts
                    .into_iter()
                    .find(|a| a.status.is_active())
                    .ok_or_else(|| format!("no active attempt for phase {phase_id}"))?;
                (active.attempt_id, active.selected_model.clone())
            };

            if workers.contains_key(&attempt_id) {
                continue;
            }

            let app_w = app.clone();
            let (plan_w, workspace_w) = {
                let state = app.state::<AppState>();
                let plan = state
                    .last_plan
                    .lock()
                    .map_err(|e| e.to_string())?
                    .clone()
                    .ok_or_else(|| "plan missing".to_string())?;
                let workspace = state
                    .last_workspace
                    .lock()
                    .map_err(|e| e.to_string())?
                    .clone()
                    .ok_or_else(|| "workspace missing".to_string())?;
                (plan, workspace)
            };
            let capability_w = capability.clone();
            let fake = fake_cli_mode.clone();
            let timeout_ms = timeouts.phase_timeout_ms;

            let handle = thread::Builder::new()
                .name(format!("tiamat-phase-{phase_id}"))
                .spawn(move || {
                    run_phase_worker(
                        app_w,
                        run_id,
                        phase_id,
                        attempt_id,
                        model_id,
                        plan_w,
                        workspace_w,
                        capability_w,
                        fake,
                        timeout_ms,
                    )
                })
                .map_err(|e| format!("spawn phase worker: {e}"))?;
            workers.insert(attempt_id, handle);
        }

        // Terminal?
        let (all_done, any_failed) = {
            let state = app.state::<AppState>();
            let store = state.store.lock().map_err(|e| e.to_string())?;
            let phases = store.list_phases(run_id).map_err(|e| e.to_string())?;
            if phases.is_empty() {
                (true, false)
            } else {
                let pending = phases.iter().any(|p| {
                    matches!(
                        p.status,
                        PhaseRuntimeStatus::Draft
                            | PhaseRuntimeStatus::Ready
                            | PhaseRuntimeStatus::Queued
                            | PhaseRuntimeStatus::Running
                            | PhaseRuntimeStatus::Verifying
                    )
                });
                let failed = phases
                    .iter()
                    .any(|p| p.status == PhaseRuntimeStatus::Failed);
                (!pending && workers.is_empty(), failed)
            }
        };

        if all_done {
            let status = if any_failed { "failed" } else { "completed" };
            set_run_status(&app, run_id, status)?;
            emit_run_event(
                &app,
                run_id,
                if any_failed {
                    "run.failed"
                } else {
                    "run.completed"
                },
                if any_failed {
                    EventLevel::Error
                } else {
                    EventLevel::Info
                },
                format!("Run {status}"),
                serde_json::json!({ "runId": run_id }),
            )?;
            break;
        }

        // Idle briefly when nothing is finishing and nothing new started.
        if workers.is_empty() {
            // No active workers but not all done — scheduler may be paused/blocked.
            thread::sleep(Duration::from_millis(200));
        } else {
            thread::sleep(Duration::from_millis(50));
        }
    }

    Ok(())
}

struct WorkerOutcome {
    attempt_id: Uuid,
    phase_id: String,
    success: bool,
    failure_kind: Option<FailureKind>,
    progress_useful: bool,
    message: String,
}

#[allow(clippy::too_many_arguments)]
fn run_phase_worker(
    app: AppHandle,
    run_id: Uuid,
    phase_id: String,
    attempt_id: Uuid,
    model_id: String,
    mut plan: tiamat_contracts::ProjectPlan,
    mut workspace: crate::workspace::RunWorkspaceManifest,
    capability: crate::cursor::CursorCapabilityReport,
    fake_cli_mode: Option<String>,
    timeout_ms: u64,
) -> WorkerOutcome {
    let _ = emit_run_event(
        &app,
        run_id,
        "attempt.started",
        EventLevel::Info,
        format!("Executing phase {phase_id} with {model_id}"),
        serde_json::json!({
            "phaseId": phase_id,
            "attemptId": attempt_id,
            "model": model_id,
        }),
    );

    let state = app.state::<AppState>();
    let outcome = {
        let store = match state.store.lock() {
            Ok(s) => s,
            Err(e) => {
                return WorkerOutcome {
                    attempt_id,
                    phase_id,
                    success: false,
                    failure_kind: Some(FailureKind::Other),
                    progress_useful: false,
                    message: format!("store lock poisoned: {e}"),
                };
            }
        };
        execute_phase(ExecutePhaseRequest {
            run_id,
            phase_id: &phase_id,
            attempt_id: Some(attempt_id),
            plan: &mut plan,
            workspace: &mut workspace,
            capability: &capability,
            model_id: &model_id,
            mode: ExecutionMode::Fresh,
            resume_chat_id: None,
            interruption_report: None,
            timeout_ms: Some(timeout_ms),
            executable_override: None,
            fake_cli_mode: fake_cli_mode.as_deref(),
            establish_baseline: true,
            flaky_retry: true,
            host: Some(HostedSpawnContext {
                store: &store,
                host: &state.process_host,
            }),
        })
    };

    match outcome {
        Ok(result) => {
            let _ = state
                .last_executor
                .lock()
                .map(|mut g| *g = Some(result.clone()));
            let _ = state.last_plan.lock().map(|mut g| *g = Some(plan));
            let _ = state
                .last_workspace
                .lock()
                .map(|mut g| *g = Some(workspace));
            WorkerOutcome {
                attempt_id,
                phase_id,
                success: result.ok,
                failure_kind: if result.ok {
                    None
                } else if result
                    .evidence_notes
                    .iter()
                    .any(|n| n.to_ascii_lowercase().contains("timeout"))
                {
                    Some(FailureKind::Timeout)
                } else if !result.layers.is_empty()
                    && result.layers.iter().any(|l| !l.all_required_passed())
                {
                    Some(FailureKind::TestFailure)
                } else {
                    Some(FailureKind::Other)
                },
                progress_useful: result
                    .recovery
                    .as_ref()
                    .map(|r| r.progress_useful)
                    .unwrap_or(!result.changed_files.is_empty()),
                message: result.message,
            }
        }
        Err(err) => WorkerOutcome {
            attempt_id,
            phase_id,
            success: false,
            failure_kind: Some(FailureKind::Other),
            progress_useful: false,
            message: err.to_string(),
        },
    }
}

fn apply_worker_outcome(app: &AppHandle, outcome: WorkerOutcome) -> Result<(), String> {
    let terminal = if outcome.success {
        AttemptTerminalResult::Succeeded
    } else {
        AttemptTerminalResult::Failed
    };
    complete_attempt_safe(
        app,
        outcome.attempt_id,
        terminal,
        outcome.failure_kind,
        outcome.progress_useful,
    )?;
    let level = if outcome.success {
        EventLevel::Info
    } else {
        EventLevel::Warning
    };
    let event_type = if outcome.success {
        "attempt.succeeded"
    } else {
        "attempt.failed"
    };
    let run_id = {
        let state = app.state::<AppState>();
        let store = state.store.lock().map_err(|e| e.to_string())?;
        // Find run via attempt.
        let mut found = None;
        for run in store.list_runs().map_err(|e| e.to_string())? {
            if store
                .list_attempts(run.run_id)
                .unwrap_or_default()
                .iter()
                .any(|a| a.attempt_id == outcome.attempt_id)
            {
                found = Some(run.run_id);
                break;
            }
        }
        found
    };
    if let Some(run_id) = run_id {
        emit_run_event(
            app,
            run_id,
            event_type,
            level,
            format!("Phase {}: {}", outcome.phase_id, outcome.message),
            serde_json::json!({
                "phaseId": outcome.phase_id,
                "attemptId": outcome.attempt_id,
                "success": outcome.success,
            }),
        )?;
    }
    Ok(())
}

fn complete_attempt_safe(
    app: &AppHandle,
    attempt_id: Uuid,
    result: AttemptTerminalResult,
    failure_kind: Option<FailureKind>,
    progress_useful: bool,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let store = state.store.lock().map_err(|e| e.to_string())?;
    complete_attempt(&store, attempt_id, result, failure_kind, progress_useful)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn set_run_status(app: &AppHandle, run_id: Uuid, status: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store
        .set_run_status(run_id, status)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn mark_cancelled(app: &AppHandle, run_id: Uuid) -> Result<(), String> {
    set_run_status(app, run_id, "cancelled")?;
    emit_run_event(
        app,
        run_id,
        "run.cancelled",
        EventLevel::Warning,
        "Run cancelled".into(),
        serde_json::json!({ "runId": run_id }),
    )?;
    Ok(())
}

fn emit_run_event(
    app: &AppHandle,
    run_id: Uuid,
    event_type: &str,
    level: EventLevel,
    message: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let event = NewEvent {
        event_id: Uuid::new_v4(),
        run_id,
        project_id: Some("tiamat".into()),
        phase_id: None,
        attempt_id: None,
        process_id: None,
        event_type: event_type.into(),
        level,
        timestamp_utc: chrono::Utc::now(),
        message: redact_line(&message),
        payload,
    };
    let envelope = store
        .append_event_atomic(None, event)
        .map_err(|e| e.to_string())?;
    let _ = store.mark_outbox_delivered(&[envelope.event_id]);
    let _ = app.emit(EVENT_CHANNEL, &envelope);
    Ok(())
}
