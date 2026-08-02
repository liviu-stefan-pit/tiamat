use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tiamat_contracts::{compile_schema, schema_path, validate_json_str, EventEnvelope};
use uuid::Uuid;

use crate::db::{self, ArtifactRecord, NewEvent, RunRecord, Store};
use crate::intake::{self, IntakeLimits, PreflightReport};
use crate::scheduler::{self, DagOrchestrator, SchedulerConfig, SchedulerSnapshot, TickResult};
use crate::security::{redact_for_persistence, redact_line, FORBIDDEN_FIXTURE_SECRETS};
use tiamat_contracts::EventLevel;

pub const EVENT_CHANNEL: &str = "tiamat://events";

pub struct AppState {
    pub store: Mutex<Store>,
    pub last_preflight: Mutex<Option<PreflightReport>>,
    pub last_cursor: Mutex<Option<crate::cursor::CursorCapabilityReport>>,
    pub last_workspace: Mutex<Option<crate::workspace::RunWorkspaceManifest>>,
    pub last_plan: Mutex<Option<tiamat_contracts::ProjectPlan>>,
    pub last_architect: Mutex<Option<crate::planner::ArchitectRunResult>>,
    pub last_scheduler: Mutex<Option<SchedulerSnapshot>>,
    pub last_executor: Mutex<Option<crate::executor::PhaseExecutionOutcome>>,
    pub last_recovery: Mutex<Option<crate::recovery::RecoveryScanReport>>,
    pub workspace_parent: Mutex<Option<PathBuf>>,
    pub process_host: crate::process::ProcessHost,
    pub abort: crate::process::AbortController,
    /// Active orchestrator supervisor (architect + DAG phase execution).
    pub orchestrator: crate::orchestrator::OrchestratorSlot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub schema_version: u32,
    pub orchestrator_mode: String,
    pub store_schema_version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractValidationResult {
    pub valid: bool,
    pub schema_name: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorStatus {
    pub mode: String,
    pub active_runs: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoRunSnapshot {
    pub run: RunRecord,
    pub events: Vec<EventEnvelope>,
    pub artifacts: Vec<ArtifactRecord>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionResult {
    pub run: RunRecord,
    pub event: EventEnvelope,
}

#[tauri::command]
pub fn get_app_info(state: State<'_, AppState>) -> Result<AppInfo, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    Ok(AppInfo {
        name: "Tiamat".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
        orchestrator_mode: crate::scheduler::ORCHESTRATOR_MODE.to_string(),
        store_schema_version: store.schema_version().map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
pub fn validate_contract_json(schema_name: String, json_text: String) -> ContractValidationResult {
    let schema_file = match schema_name.as_str() {
        "intake-manifest" => schema_path("intake-manifest.schema.json"),
        "event-envelope" => schema_path("event-envelope.schema.json"),
        "project-plan" => schema_path("project-plan.schema.json"),
        "phase-result" => schema_path("phase-result.schema.json"),
        _ => {
            return ContractValidationResult {
                valid: false,
                schema_name,
                error: Some("unsupported schema name".to_string()),
            };
        }
    };

    let result =
        compile_schema(&schema_file).and_then(|schema| validate_json_str(&schema, &json_text));

    match result {
        Ok(_) => ContractValidationResult {
            valid: true,
            schema_name,
            error: None,
        },
        Err(err) => ContractValidationResult {
            valid: false,
            schema_name,
            error: Some(err.to_string()),
        },
    }
}

#[tauri::command]
pub fn orchestrator_status(state: State<'_, AppState>) -> Result<OrchestratorStatus, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let snap = state
        .last_scheduler
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let paused = snap.as_ref().map(|s| s.paused).unwrap_or(false);
    let active = snap.as_ref().map(|s| s.active_attempts).unwrap_or(0);
    drop(store);
    let status = DagOrchestrator::status_with(active, paused);
    Ok(OrchestratorStatus {
        mode: status.mode,
        active_runs: status.active_runs,
        message: status.message,
    })
}

#[tauri::command]
pub fn ensure_demo_run(state: State<'_, AppState>) -> Result<DemoRunSnapshot, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let (run, events) = db::ensure_demo_run(&store).map_err(|e| e.to_string())?;
    let artifacts = store.list_artifacts().map_err(|e| e.to_string())?;
    Ok(DemoRunSnapshot {
        run,
        events,
        artifacts,
    })
}

#[tauri::command]
pub fn list_runs(state: State<'_, AppState>) -> Result<Vec<RunRecord>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.list_runs().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn replay_events(
    state: State<'_, AppState>,
    run_id: String,
    after_sequence: u64,
) -> Result<Vec<EventEnvelope>, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store
        .replay_events(run_id, after_sequence)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_artifacts(state: State<'_, AppState>) -> Result<Vec<ArtifactRecord>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.list_artifacts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn transition_run_status(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    new_status: String,
    message: String,
    event_type: Option<String>,
) -> Result<TransitionResult, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;

    // Terminal statuses require empty registry + cleanup proof when hosted processes existed.
    if matches!(new_status.as_str(), "completed" | "failed" | "cancelled") {
        store
            .assert_run_may_become_terminal(run_id)
            .map_err(|e| e.to_string())?;
        // DATA-002: load workspace manifest from disk (do not rely solely on AppState cache).
        let mut search_roots: Vec<PathBuf> = Vec::new();
        if let Ok(parent) = state.workspace_parent.lock() {
            if let Some(p) = parent.clone() {
                search_roots.push(p);
            }
        }
        if let Ok(guard) = state.last_workspace.lock() {
            if let Some(ws) = guard.as_ref() {
                search_roots.push(PathBuf::from(&ws.managed_run_root));
                if let Some(parent) = Path::new(&ws.managed_run_root).parent() {
                    search_roots.push(parent.to_path_buf());
                }
            }
        }
        if let Ok(procs) = store.list_processes_for_run(run_id) {
            for proc in procs {
                if let Some(ws) = proc.workspace {
                    search_roots.push(PathBuf::from(ws));
                }
            }
        }
        // Deduplicate while preserving order.
        search_roots.sort();
        search_roots.dedup();
        let refreshed =
            crate::workspace::recheck_source_fingerprints_for_run(run_id, &search_roots).map_err(
                |e| format!("source fingerprint re-check blocked terminal transition: {e}"),
            )?;
        if let Some(manifest) = refreshed {
            if let Ok(mut guard) = state.last_workspace.lock() {
                *guard = Some(manifest);
            }
        }
    }

    let event = NewEvent {
        event_id: Uuid::new_v4(),
        run_id,
        project_id: Some("tiamat".into()),
        phase_id: None,
        attempt_id: None,
        process_id: None,
        event_type: event_type.unwrap_or_else(|| "run.status_changed".into()),
        level: EventLevel::Info,
        timestamp_utc: chrono::Utc::now(),
        message,
        payload: serde_json::json!({ "status": new_status }),
    };

    let envelope = store
        .append_event_atomic(Some(&new_status), event)
        .map_err(|e| e.to_string())?;
    store
        .mark_outbox_delivered(&[envelope.event_id])
        .map_err(|e| e.to_string())?;

    let run = store
        .get_run(run_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("run not found: {run_id}"))?;

    let _ = app.emit(EVENT_CHANNEL, &envelope);

    Ok(TransitionResult {
        run,
        event: envelope,
    })
}

#[tauri::command]
pub fn pick_intake_paths(kind: String) -> Result<Vec<String>, String> {
    match kind.as_str() {
        "file" | "files" => {
            let files = rfd::FileDialog::new()
                .set_title("Select files for Tiamat intake")
                .pick_files()
                .unwrap_or_default();
            Ok(files
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect())
        }
        "folder" | "directory" => {
            let folder = rfd::FileDialog::new()
                .set_title("Select folder for Tiamat intake")
                .pick_folder();
            Ok(folder
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect())
        }
        _ => Err(format!("unsupported pick kind: {kind}")),
    }
}

/// Pick the directory where the run will be materialized and built.
#[tauri::command]
pub fn pick_output_dir() -> Result<Option<String>, String> {
    let folder = rfd::FileDialog::new()
        .set_title("Select output folder for Tiamat build")
        .pick_folder();
    Ok(folder.map(|p| p.to_string_lossy().to_string()))
}

/// Start a full orchestrated run: materialize → architect → DAG phase execution.
#[tauri::command]
pub fn start_run(
    app: AppHandle,
    input_paths: Vec<String>,
    output_dir: String,
    max_concurrent: Option<u32>,
    fake_cli_mode: Option<String>,
) -> Result<crate::orchestrator::StartRunResult, String> {
    crate::orchestrator::start_run(
        app,
        crate::orchestrator::StartRunRequest {
            input_paths,
            output_dir,
            max_concurrent,
            fake_cli_mode,
        },
    )
}

#[tauri::command]
pub fn cancel_run(app: AppHandle) -> Result<crate::orchestrator::RunStatusSnapshot, String> {
    crate::orchestrator::cancel_active_run(&app)
}

#[tauri::command]
pub fn get_run_status(app: AppHandle) -> Result<crate::orchestrator::RunStatusSnapshot, String> {
    crate::orchestrator::get_run_status(&app)
}

#[tauri::command]
pub fn run_intake_preflight(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<PreflightReport, String> {
    let configured = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .get_app_settings()
            .map_err(|e| e.to_string())?
            .cursor_cli_path
    };
    let report = intake::run_preflight_with_configured(
        &paths,
        IntakeLimits::default(),
        configured.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    emit_intake_event(
        &app,
        &state,
        "intake.preflight_completed",
        if report.blockers.is_empty() {
            EventLevel::Info
        } else {
            EventLevel::Warning
        },
        redact_line(&format!(
            "Preflight scanned {} source(s); {} warning(s); {} blocker(s); {} secret-risk marker(s)",
            report.manifest.sources.len(),
            report.warnings.len(),
            report.blockers.len(),
            report.secret_risks.len()
        )),
        serde_json::json!({
            "intakeId": report.manifest.intake_id,
            "sourceCount": report.manifest.sources.len(),
            "projectCount": report.manifest.projects.len(),
            "warningCount": report.warnings.len(),
            "blockerCount": report.blockers.len(),
            "secretRiskCount": report.secret_risks.len(),
            "canStart": report.can_start,
            "inventoryArtifact": report.manifest.inventory_artifact,
        }),
    )?;
    *state.last_preflight.lock().map_err(|e| e.to_string())? = Some(report.clone());
    Ok(report)
}

#[tauri::command]
pub fn confirm_intake_trust(
    app: AppHandle,
    state: State<'_, AppState>,
    intake_id: String,
    acknowledged_untrusted: bool,
    acknowledged_execution_risk: bool,
) -> Result<PreflightReport, String> {
    let mut guard = state.last_preflight.lock().map_err(|e| e.to_string())?;
    let current = guard
        .as_ref()
        .ok_or_else(|| "no preflight report to trust".to_string())?;
    if current.manifest.intake_id.to_string() != intake_id {
        return Err("intakeId does not match the latest preflight".into());
    }
    let report = intake::apply_trust(
        current.clone(),
        acknowledged_untrusted,
        acknowledged_execution_risk,
    );
    *guard = Some(report.clone());
    drop(guard);

    emit_intake_event(
        &app,
        &state,
        "intake.trust_updated",
        EventLevel::Info,
        format!(
            "Trust confirmation {} (canStart={})",
            if report.trust.confirmed {
                "accepted"
            } else {
                "incomplete"
            },
            report.can_start
        ),
        serde_json::json!({
            "intakeId": report.manifest.intake_id,
            "confirmed": report.trust.confirmed,
            "canStart": report.can_start,
        }),
    )?;
    Ok(report)
}

#[tauri::command]
pub fn get_intake_preflight(state: State<'_, AppState>) -> Result<Option<PreflightReport>, String> {
    Ok(state
        .last_preflight
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

#[tauri::command]
pub fn probe_cursor_capability(
    state: State<'_, AppState>,
) -> Result<crate::cursor::CursorCapabilityReport, String> {
    crate::cursor::invalidate_probe_cache();
    let configured = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .get_app_settings()
            .map_err(|e| e.to_string())?
            .cursor_cli_path
    };
    let report = crate::cursor::probe_cursor_capability_with_configured(configured.as_deref());
    *state.last_cursor.lock().map_err(|e| e.to_string())? = Some(report.clone());
    Ok(report)
}

#[tauri::command]
pub fn get_cursor_capability(
    state: State<'_, AppState>,
) -> Result<crate::cursor::CursorCapabilityReport, String> {
    {
        let guard = state.last_cursor.lock().map_err(|e| e.to_string())?;
        if let Some(cached) = guard.as_ref() {
            return Ok(cached.clone());
        }
    }
    probe_cursor_capability(state)
}

#[tauri::command]
pub fn get_app_settings(
    state: State<'_, AppState>,
) -> Result<crate::packaging::AppSettings, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.get_app_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_cursor_cli_path(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<crate::packaging::AppSettings, String> {
    crate::cursor::invalidate_probe_cache();
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.set_cursor_cli_path(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_cursor_models() -> Result<crate::cursor::CursorModelsReport, String> {
    Ok(crate::cursor::list_cursor_models())
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCursorArgs {
    pub workspace: String,
    pub prompt: String,
    pub model: Option<String>,
    pub resume_chat_id: Option<String>,
    pub force: Option<bool>,
    pub trust: Option<bool>,
    pub plan_mode: Option<bool>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[tauri::command]
pub fn preview_cursor_command(
    state: State<'_, AppState>,
    args: PreviewCursorArgs,
) -> Result<crate::cursor::CursorCommandPreview, String> {
    let capability = {
        let guard = state.last_cursor.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    let capability = if let Some(cached) = capability {
        cached
    } else {
        let report = crate::cursor::probe_cursor_capability();
        *state.last_cursor.lock().map_err(|e| e.to_string())? = Some(report.clone());
        report
    };

    let executable = capability
        .executable
        .clone()
        .ok_or_else(|| "Cursor CLI executable is not available".to_string())?;

    let request = crate::cursor::CursorInvokeRequest {
        workspace: args.workspace,
        model: args.model,
        prompt: args.prompt,
        output_format: Some("stream-json".into()),
        resume_chat_id: args.resume_chat_id,
        force: args.force.unwrap_or(true),
        trust: args.trust.unwrap_or(true),
        auto_review: false,
        plan_mode: args.plan_mode.unwrap_or(false),
        api_key: args.api_key,
        timeout_ms: args.timeout_ms,
    };

    let built =
        crate::cursor::build_cursor_command(&executable, &capability.features, &request, None)
            .map_err(|e| e.to_string())?;
    Ok(crate::cursor::preview_built_command(&built, &[]))
}

#[tauri::command]
pub fn materialize_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    create_internal_worktrees: Option<bool>,
) -> Result<crate::workspace::RunWorkspaceManifest, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let preflight = state
        .last_preflight
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no trusted preflight available".to_string())?;
    if !preflight.can_start {
        return Err("preflight cannot start; trust and blockers must clear first".into());
    }

    let managed_parent = {
        let guard = state.workspace_parent.lock().map_err(|e| e.to_string())?;
        if let Some(parent) = guard.clone() {
            parent
        } else {
            drop(guard);
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("tiamat")
                .join("workspaces");
            std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
            data_dir
        }
    };

    let manifest =
        crate::workspace::materialize_run_workspace(crate::workspace::MaterializeRequest {
            run_id,
            intake: preflight.manifest.clone(),
            managed_parent,
            create_internal_worktrees: create_internal_worktrees.unwrap_or(true),
        })
        .map_err(|e| e.to_string())?;

    emit_workspace_event(
        &app,
        &state,
        "workspace.materialized",
        EventLevel::Info,
        format!(
            "Materialized {} managed project(s); source_unchanged={}",
            manifest.projects.len(),
            manifest.source_unchanged
        ),
        serde_json::json!({
            "runId": manifest.run_id,
            "intakeId": manifest.intake_id,
            "managedRunRoot": manifest.managed_run_root,
            "projectCount": manifest.projects.len(),
            "sourceUnchanged": manifest.source_unchanged,
            "promotionStatus": format!("{:?}", manifest.promotion.status).to_lowercase(),
        }),
    )?;

    *state.last_workspace.lock().map_err(|e| e.to_string())? = Some(manifest.clone());
    Ok(manifest)
}

#[tauri::command]
pub fn get_workspace_manifest(
    state: State<'_, AppState>,
) -> Result<Option<crate::workspace::RunWorkspaceManifest>, String> {
    Ok(state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

#[tauri::command]
pub fn validate_workspace_roots(
    state: State<'_, AppState>,
    write_roots: Vec<String>,
    read_roots: Vec<String>,
) -> Result<crate::workspace::RootValidationResult, String> {
    let manifest = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no workspace manifest".to_string())?;

    let mut write_errors = Vec::new();
    for root in &write_roots {
        if let Err(err) = manifest.validate_write_root(root) {
            write_errors.push(err);
        }
    }
    let mut read_errors = Vec::new();
    for root in &read_roots {
        if let Err(err) = manifest.validate_read_root(root) {
            read_errors.push(err);
        }
    }
    Ok(crate::workspace::RootValidationResult {
        ok: write_errors.is_empty() && read_errors.is_empty(),
        write_errors,
        read_errors,
    })
}

#[tauri::command]
pub fn create_workspace_checkpoint(
    state: State<'_, AppState>,
    project_id: String,
    message: String,
) -> Result<crate::workspace::RunWorkspaceManifest, String> {
    let current = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no workspace manifest".to_string())?;
    let root = PathBuf::from(&current.managed_run_root);
    let manifest = crate::workspace::checkpoint_project(&root, &project_id, &message)
        .map_err(|e| e.to_string())?;
    *state.last_workspace.lock().map_err(|e| e.to_string())? = Some(manifest.clone());
    Ok(manifest)
}

#[tauri::command]
pub fn run_architect_pipeline(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
) -> Result<crate::planner::ArchitectRunResult, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let preflight = state
        .last_preflight
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no trusted preflight available".to_string())?;
    if !preflight.can_start {
        return Err("preflight cannot start".into());
    }
    let mut workspace = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "workspace must be materialized before architect".to_string())?;

    let capability = {
        let guard = state.last_cursor.lock().map_err(|e| e.to_string())?;
        if let Some(cached) = guard.clone() {
            cached
        } else {
            drop(guard);
            let report = crate::cursor::probe_cursor_capability();
            *state.last_cursor.lock().map_err(|e| e.to_string())? = Some(report.clone());
            report
        }
    };

    emit_planner_event(
        &app,
        &state,
        "planning.started",
        EventLevel::Info,
        "Architect planning started".into(),
        serde_json::json!({ "runId": run_id }),
    )?;

    let store = state.store.lock().map_err(|e| e.to_string())?;
    let result = crate::planner::run_architect_pipeline(crate::planner::ArchitectPipelineRequest {
        run_id,
        preflight: &preflight,
        workspace: &mut workspace,
        capability: &capability,
        executable_override: None,
        host: Some(crate::process::HostedSpawnContext {
            store: &store,
            host: &state.process_host,
        }),
    });
    drop(store);

    *state.last_workspace.lock().map_err(|e| e.to_string())? = Some(workspace);
    *state.last_architect.lock().map_err(|e| e.to_string())? = Some(result.clone());
    if let Some(plan) = result.plan.clone() {
        *state.last_plan.lock().map_err(|e| e.to_string())? = Some(plan);
    }

    if result.ok {
        emit_planner_event(
            &app,
            &state,
            "plan.compiled",
            EventLevel::Info,
            format!(
                "Architect plan compiled (phases={}, degraded={})",
                result.plan.as_ref().map(|p| p.phases.len()).unwrap_or(0),
                result.degraded_mode
            ),
            serde_json::json!({
                "runId": result.run_id,
                "phaseCount": result.plan.as_ref().map(|p| p.phases.len()).unwrap_or(0),
                "degradedMode": result.degraded_mode,
                "selectedModel": result.model_selection.selected_model,
                "planJsonPath": result.plan_json_path,
                "checkpointCommit": result.checkpoint.as_ref().map(|c| c.commit.clone()),
            }),
        )?;
    } else {
        emit_planner_event(
            &app,
            &state,
            "plan.failed",
            EventLevel::Error,
            result
                .error
                .clone()
                .unwrap_or_else(|| "architect planning failed".into()),
            serde_json::json!({
                "runId": result.run_id,
                "attemptCount": result.attempts.len(),
                "evidence": result.evidence,
            }),
        )?;
    }

    Ok(result)
}

#[tauri::command]
pub fn get_project_plan(
    state: State<'_, AppState>,
) -> Result<Option<tiamat_contracts::ProjectPlan>, String> {
    Ok(state.last_plan.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub fn get_graph_projection(
    state: State<'_, AppState>,
) -> Result<Option<crate::planner::GraphProjection>, String> {
    let plan = state.last_plan.lock().map_err(|e| e.to_string())?.clone();
    let mut projection = plan.map(|p| crate::planner::project_graph(&p));
    if let Some(snap) = state
        .last_scheduler
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
    {
        if let Some(graph) = projection.as_mut() {
            for node in &mut graph.nodes {
                if let Some(phase) = snap.phases.iter().find(|p| p.phase_id == node.phase_id) {
                    node.status = phase.status.as_str().to_string();
                }
            }
        }
    }
    Ok(projection)
}

#[tauri::command]
pub fn get_architect_result(
    state: State<'_, AppState>,
) -> Result<Option<crate::planner::ArchitectRunResult>, String> {
    Ok(state
        .last_architect
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

#[tauri::command]
pub fn start_scheduler(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    max_concurrent: Option<u32>,
) -> Result<SchedulerSnapshot, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let plan = state
        .last_plan
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no compiled plan; run architect first".to_string())?;
    if plan.run_id != run_id {
        return Err("plan runId mismatch".into());
    }
    let config = SchedulerConfig {
        max_concurrent: max_concurrent
            .unwrap_or_else(scheduler::default_max_concurrent)
            .clamp(1, 4),
        ..SchedulerConfig::default()
    };
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let snap =
        scheduler::load_plan_into_scheduler(&store, &plan, &config).map_err(|e| e.to_string())?;
    *state.last_scheduler.lock().map_err(|e| e.to_string())? = Some(snap.clone());
    emit_scheduler_event(
        &app,
        &state,
        "scheduler.loaded",
        EventLevel::Info,
        format!("Scheduler loaded {} phase(s)", snap.phases.len()),
        serde_json::json!({
            "runId": run_id,
            "phaseCount": snap.phases.len(),
            "maxConcurrent": snap.max_concurrent,
        }),
    )?;
    Ok(snap)
}

#[tauri::command]
pub fn scheduler_tick(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
) -> Result<TickResult, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        if !crate::recovery::execution_allowed(&store, run_id).map_err(|e| e.to_string())? {
            return Err(
                "startup recovery pending — choose Resume or Cancel before scheduling".into(),
            );
        }
    }
    let models = state
        .last_cursor
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .map(|c| c.models.clone())
        .unwrap_or_else(|| {
            [
                scheduler::MODEL_COMPOSER,
                scheduler::MODEL_GROK_LOW,
                scheduler::MODEL_GROK_MEDIUM,
                scheduler::MODEL_GROK_HIGH,
            ]
            .into_iter()
            .map(|id| crate::cursor::CursorModelInfo {
                id: id.into(),
                label: id.into(),
            })
            .collect()
        });
    let lease = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .get_scheduler_lease(run_id)
            .map_err(|e| e.to_string())?
    };
    let config = SchedulerConfig {
        max_concurrent: lease
            .as_ref()
            .map(|l| l.max_concurrent)
            .unwrap_or_else(scheduler::default_max_concurrent),
        ..SchedulerConfig::default()
    };
    let final_reviews: Vec<String> = state
        .last_plan
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .map(|p| {
            p.final_gates
                .iter()
                .flat_map(|g| g.dependencies.clone())
                .collect()
        })
        .unwrap_or_default();

    let store = state.store.lock().map_err(|e| e.to_string())?;
    let result = scheduler::tick(&store, run_id, &models, &config, &final_reviews)
        .map_err(|e| e.to_string())?;
    let snap = scheduler::snapshot(&store, run_id).map_err(|e| e.to_string())?;
    drop(store);
    *state.last_scheduler.lock().map_err(|e| e.to_string())? = Some(snap);
    emit_scheduler_event(
        &app,
        &state,
        "scheduler.tick",
        EventLevel::Info,
        format!(
            "Scheduler tick epoch={} started={:?}",
            result.epoch, result.started
        ),
        serde_json::json!({
            "epoch": result.epoch,
            "started": result.started,
            "blocked": result.blocked,
            "paused": result.skipped_due_to_pause,
        }),
    )?;
    Ok(result)
}

#[tauri::command]
pub fn scheduler_complete_attempt(
    app: AppHandle,
    state: State<'_, AppState>,
    attempt_id: String,
    success: bool,
    failure_kind: Option<String>,
    progress_useful: Option<bool>,
) -> Result<scheduler::PhaseRecord, String> {
    let attempt_id = Uuid::parse_str(&attempt_id).map_err(|e| e.to_string())?;
    let result = if success {
        scheduler::AttemptTerminalResult::Succeeded
    } else if failure_kind.as_deref() == Some("timeout") {
        scheduler::AttemptTerminalResult::TimedOut
    } else {
        scheduler::AttemptTerminalResult::Failed
    };
    let kind = failure_kind.map(|k| scheduler::FailureKind::parse(&k));
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let phase = scheduler::complete_attempt(
        &store,
        attempt_id,
        result,
        kind,
        progress_useful.unwrap_or(false),
    )
    .map_err(|e| e.to_string())?;
    let snap = scheduler::snapshot(&store, phase.run_id).map_err(|e| e.to_string())?;
    drop(store);
    *state.last_scheduler.lock().map_err(|e| e.to_string())? = Some(snap);
    emit_scheduler_event(
        &app,
        &state,
        if success {
            "attempt.succeeded"
        } else {
            "attempt.failed"
        },
        if success {
            EventLevel::Info
        } else {
            EventLevel::Warning
        },
        format!(
            "Attempt {} for {} -> {}",
            attempt_id,
            phase.phase_id,
            phase.status.as_str()
        ),
        serde_json::json!({
            "attemptId": attempt_id,
            "phaseId": phase.phase_id,
            "status": phase.status.as_str(),
        }),
    )?;
    Ok(phase)
}

#[tauri::command]
pub fn scheduler_pause(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
) -> Result<SchedulerSnapshot, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let snap = scheduler::pause_scheduling(&store, run_id).map_err(|e| e.to_string())?;
    drop(store);
    *state.last_scheduler.lock().map_err(|e| e.to_string())? = Some(snap.clone());
    let _ = app;
    Ok(snap)
}

#[tauri::command]
pub fn scheduler_resume(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
) -> Result<SchedulerSnapshot, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let snap = scheduler::resume_scheduling(&store, run_id).map_err(|e| e.to_string())?;
    drop(store);
    *state.last_scheduler.lock().map_err(|e| e.to_string())? = Some(snap.clone());
    let _ = app;
    Ok(snap)
}

#[tauri::command]
pub fn get_scheduler_snapshot(
    state: State<'_, AppState>,
    run_id: Option<String>,
) -> Result<Option<SchedulerSnapshot>, String> {
    if let Some(run_id) = run_id {
        let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let snap = scheduler::snapshot(&store, run_id).map_err(|e| e.to_string())?;
        return Ok(Some(snap));
    }
    Ok(state
        .last_scheduler
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

#[tauri::command]
pub fn emergency_abort(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: Option<String>,
    force: Option<bool>,
) -> Result<crate::process::AbortPressResult, String> {
    let run_uuid = run_id
        .as_ref()
        .map(|s| Uuid::parse_str(s))
        .transpose()
        .map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let active_run = match run_uuid {
        Some(id) => store
            .get_run(id)
            .map_err(|e| e.to_string())?
            .map(|r| {
                !matches!(
                    r.status.as_str(),
                    "completed" | "failed" | "cancelled" | "created"
                )
            })
            .unwrap_or(false),
        None => state.process_host.active_live_count() > 0,
    };
    let result = state
        .abort
        .handle_press(
            &store,
            &state.process_host,
            run_uuid,
            active_run,
            force.unwrap_or(false),
        )
        .map_err(|e| e.to_string())?;
    drop(store);
    let _ = app.emit("tiamat://abort", &result);
    Ok(result)
}

#[tauri::command]
pub fn get_process_registry(
    state: State<'_, AppState>,
    run_id: Option<String>,
) -> Result<crate::process::ProcessRegistrySnapshot, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let abort = store.get_abort_settings().map_err(|e| e.to_string())?;
    let processes = if let Some(id) = run_id {
        let run_id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
        store
            .list_processes_for_run(run_id)
            .map_err(|e| e.to_string())?
    } else {
        store.list_active_processes().map_err(|e| e.to_string())?
    };
    let active_count = processes.iter().filter(|p| p.state.is_active()).count() as u32;
    let can_start = crate::process::can_start_with_abort_policy(&abort);
    Ok(crate::process::ProcessRegistrySnapshot {
        active_count,
        processes,
        abort,
        can_start,
        cleanup_incomplete: active_count > 0,
    })
}

#[tauri::command]
pub fn get_abort_settings(
    state: State<'_, AppState>,
) -> Result<crate::process::AbortSettings, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.get_abort_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn acknowledge_degraded_abort(
    state: State<'_, AppState>,
) -> Result<crate::process::AbortSettings, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    crate::process::acknowledge_degraded(&store).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rebind_abort_shortcut(
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<crate::process::AbortSettings, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    crate::process::rebind_shortcut(&store, &shortcut).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_close_policy(
    state: State<'_, AppState>,
    run_id: Option<String>,
    choice: String,
) -> Result<crate::process::AbortPressResult, String> {
    let run_uuid = run_id
        .as_ref()
        .map(|s| Uuid::parse_str(s))
        .transpose()
        .map_err(|e| e.to_string())?;
    let choice = match choice.as_str() {
        "keep_running" | "keepRunning" => crate::process::ClosePolicyChoice::KeepRunning,
        "stop_all_and_exit" | "stopAllAndExit" => crate::process::ClosePolicyChoice::StopAllAndExit,
        other => return Err(format!("unknown close policy: {other}")),
    };
    let store = state.store.lock().map_err(|e| e.to_string())?;
    state
        .abort
        .apply_close_policy(&store, &state.process_host, run_uuid, choice)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reconcile_processes(
    state: State<'_, AppState>,
) -> Result<crate::process::ReconcileReport, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    crate::process::reconcile_owned_processes(&store).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn execute_phase_fixture(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    phase_id: String,
    mode: Option<String>,
) -> Result<crate::executor::PhaseExecutionOutcome, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let mode = mode.unwrap_or_else(|| "impl_success".into());

    let mut workspace = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "workspace must be materialized before phase execution".to_string())?;
    let mut plan = state
        .last_plan
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "plan must be compiled before phase execution".to_string())?;
    let capability = state
        .last_cursor
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "cursor capability missing".to_string())?;

    let fake_js = tiamat_contracts::repo_root()
        .join("fixtures")
        .join("cursor-cli")
        .join("fake-agent.mjs");
    let exe = format!("node|{}", fake_js.display());

    let store = state.store.lock().map_err(|e| e.to_string())?;
    let outcome = crate::executor::execute_phase(crate::executor::ExecutePhaseRequest {
        run_id,
        attempt_id: Some(Uuid::new_v4()),
        plan: &mut plan,
        phase_id: &phase_id,
        workspace: &mut workspace,
        capability: &capability,
        model_id: "composer-2.5",
        mode: crate::executor::ExecutionMode::Fresh,
        interruption_report: None,
        resume_chat_id: None,
        executable_override: Some(&exe),
        fake_cli_mode: Some(&mode),
        timeout_ms: Some(30_000),
        establish_baseline: true,
        flaky_retry: true,
        host: Some(crate::process::HostedSpawnContext {
            store: &store,
            host: &state.process_host,
        }),
    })
    .map_err(|e| e.to_string())?;
    drop(store);

    *state.last_plan.lock().map_err(|e| e.to_string())? = Some(plan);
    *state.last_workspace.lock().map_err(|e| e.to_string())? = Some(workspace);
    *state.last_executor.lock().map_err(|e| e.to_string())? = Some(outcome.clone());

    emit_scheduler_event(
        &app,
        &state,
        if outcome.ok {
            "phase.passed"
        } else {
            "phase.failed"
        },
        if outcome.ok {
            EventLevel::Info
        } else {
            EventLevel::Warning
        },
        outcome.message.clone(),
        serde_json::json!({
            "phaseId": outcome.phase_id,
            "ok": outcome.ok,
            "checkpointed": outcome.project_checkpoint.is_some(),
            "boundaryOk": outcome.boundary_ok,
            "layers": outcome.layers.iter().map(|l| {
                serde_json::json!({
                    "kind": format!("{:?}", l.kind).to_ascii_lowercase(),
                    "passed": l.passed,
                    "failed": l.failed,
                    "required": l.required,
                })
            }).collect::<Vec<_>>(),
        }),
    )?;
    Ok(outcome)
}

#[tauri::command]
pub fn get_executor_outcome(
    state: State<'_, AppState>,
) -> Result<Option<crate::executor::PhaseExecutionOutcome>, String> {
    Ok(state
        .last_executor
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedPerfResult {
    pub run_id: String,
    pub seeded: u64,
    pub total_events: u64,
    pub first_sequence: u64,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BurstResult {
    pub run_id: String,
    pub emitted: u64,
    pub events: Vec<EventEnvelope>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReportResult {
    pub run_id: String,
    pub report_json: String,
    pub artifact_id: Option<String>,
    pub relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOutputResult {
    pub path: String,
    pub opened: bool,
    pub message: String,
}

/// Seed N persisted fake events for P09 performance fixtures.
#[tauri::command]
pub fn seed_perf_events(
    state: State<'_, AppState>,
    run_id: String,
    count: u64,
) -> Result<SeedPerfResult, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let events = store
        .bulk_seed_events(run_id, count, "perf.seed")
        .map_err(|e| e.to_string())?;
    let total = store.event_count(run_id).map_err(|e| e.to_string())?;
    Ok(SeedPerfResult {
        run_id: run_id.to_string(),
        seeded: events.len() as u64,
        total_events: total,
        first_sequence: events.first().map(|e| e.sequence).unwrap_or(0),
        last_sequence: events.last().map(|e| e.sequence).unwrap_or(0),
    })
}

/// Append and emit a burst of events (target ~1000/s fixture).
#[tauri::command]
pub fn emit_event_burst(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    count: u64,
) -> Result<BurstResult, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let events = store
        .bulk_seed_events(run_id, count, "perf.burst")
        .map_err(|e| e.to_string())?;
    for event in &events {
        let _ = app.emit(EVENT_CHANNEL, event);
    }
    Ok(BurstResult {
        run_id: run_id.to_string(),
        emitted: events.len() as u64,
        events,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// Export a redacted run report (JSON) and store as an artifact.
#[tauri::command]
pub fn export_run_report(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<ExportReportResult, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let run = store
        .get_run(run_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("run not found: {run_id}"))?;
    let events = store.replay_events(run_id, 0).map_err(|e| e.to_string())?;
    let plan = state.last_plan.lock().map_err(|e| e.to_string())?.clone();
    let scheduler = state
        .last_scheduler
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let executor = state
        .last_executor
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let workspace = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let active = store
        .active_process_count(Some(run_id))
        .map_err(|e| e.to_string())?;

    let redacted_events: Vec<serde_json::Value> = events
        .iter()
        .map(|event| {
            let (message, _) = redact_for_persistence(&event.message, &[]);
            serde_json::json!({
                "sequence": event.sequence,
                "eventId": event.event_id,
                "type": event.r#type,
                "level": event.level,
                "timestampUtc": event.timestamp_utc,
                "phaseId": event.phase_id,
                "message": message,
            })
        })
        .collect();

    let report = serde_json::json!({
        "schemaVersion": 1,
        "runId": run.run_id,
        "status": run.status,
        "title": run.title,
        "exportedAtUtc": chrono::Utc::now().to_rfc3339(),
        "planTitle": plan.as_ref().map(|p| p.title.clone()),
        "phaseCount": plan.as_ref().map(|p| p.phases.len()).unwrap_or(0),
        "scheduler": scheduler.as_ref().map(|s| serde_json::json!({
            "mode": s.mode,
            "paused": s.paused,
            "epoch": s.epoch,
            "phases": s.phases.iter().map(|p| serde_json::json!({
                "phaseId": p.phase_id,
                "status": p.status,
                "attemptCount": p.attempt_count,
            })).collect::<Vec<_>>(),
        })),
        "executorOk": executor.as_ref().map(|e| e.ok),
        "workspaceRoot": workspace.as_ref().map(|w| w.managed_run_root.clone()),
        "processRegistryEmpty": active == 0,
        "events": redacted_events,
    });
    let report_json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    let (report_json, _) = redact_for_persistence(&report_json, &[]);
    for secret in FORBIDDEN_FIXTURE_SECRETS {
        if report_json.contains(secret) {
            return Err(format!(
                "refusing export: fixture secret would leak ({secret})"
            ));
        }
    }
    let artifact = store
        .put_artifact(
            report_json.as_bytes(),
            Some("application/json"),
            Some("reports/run-report.json"),
            serde_json::json!({ "kind": "run_report", "runId": run_id.to_string() }),
        )
        .map_err(|e| e.to_string())?;

    Ok(ExportReportResult {
        run_id: run_id.to_string(),
        report_json,
        artifact_id: Some(artifact.artifact_id),
        relative_path: artifact.relative_path,
    })
}

/// Reset a failed/blocked phase to ready so the scheduler can retry.
#[tauri::command]
pub fn scheduler_retry_phase(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    phase_id: Option<String>,
) -> Result<SchedulerSnapshot, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let snap = scheduler::snapshot(&store, run_id).map_err(|e| e.to_string())?;
    let target = phase_id.or_else(|| {
        snap.phases
            .iter()
            .find(|p| {
                matches!(
                    p.status,
                    crate::scheduler::PhaseRuntimeStatus::Failed
                        | crate::scheduler::PhaseRuntimeStatus::Blocked
                        | crate::scheduler::PhaseRuntimeStatus::NeedsReview
                )
            })
            .map(|p| p.phase_id.clone())
    });
    let Some(phase_id) = target else {
        return Err("no failed phase available to retry".into());
    };
    store
        .update_phase_status(
            run_id,
            &phase_id,
            crate::scheduler::PhaseRuntimeStatus::Ready,
            Some(&chrono::Utc::now().to_rfc3339()),
            None,
            None,
        )
        .map_err(|e| e.to_string())?;
    drop(store);
    emit_scheduler_event(
        &app,
        &state,
        "phase.retry_requested",
        EventLevel::Info,
        format!("Retry requested for {phase_id}"),
        serde_json::json!({ "phaseId": phase_id }),
    )?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let snap = scheduler::snapshot(&store, run_id).map_err(|e| e.to_string())?;
    *state.last_scheduler.lock().map_err(|e| e.to_string())? = Some(snap.clone());
    Ok(snap)
}

/// Reveal the build output folder in the platform file manager.
#[tauri::command]
pub fn open_run_output(state: State<'_, AppState>) -> Result<OpenOutputResult, String> {
    let workspace = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let Some(workspace) = workspace else {
        return Err("no managed workspace available".into());
    };
    let path = workspace.managed_run_root.clone();
    let opened = reveal_in_file_manager(&path);
    Ok(OpenOutputResult {
        path,
        opened,
        message: if opened {
            "Opened output folder".into()
        } else {
            "Output path resolved (open deferred in this environment)".into()
        },
    })
}

/// Open a directory in the platform file manager. Best effort: a headless or
/// minimal environment simply reports `false` rather than failing the command.
fn reveal_in_file_manager(path: &str) -> bool {
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(path).spawn().is_ok()
}

/// Full startup recovery scan (DB integrity, process reconcile, side effects, offer).
#[tauri::command]
pub fn run_startup_recovery(
    state: State<'_, AppState>,
) -> Result<crate::recovery::RecoveryScanReport, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let disk_path = state
        .workspace_parent
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .or_else(|| store.artifact_root().parent().map(|p| p.to_path_buf()));
    let report = crate::recovery::run_startup_recovery(&store, disk_path.as_deref())
        .map_err(|e| e.to_string())?;
    drop(store);
    *state.last_recovery.lock().map_err(|e| e.to_string())? = Some(report.clone());
    Ok(report)
}

#[tauri::command]
pub fn get_recovery_offer(
    state: State<'_, AppState>,
    run_id: Option<String>,
) -> Result<Option<crate::recovery::RecoveryOffer>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    if let Some(id) = run_id {
        let run_id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
        return store.get_recovery_offer(run_id).map_err(|e| e.to_string());
    }
    let pending = store
        .list_pending_recovery_offers()
        .map_err(|e| e.to_string())?;
    Ok(pending.into_iter().next())
}

#[tauri::command]
pub fn recovery_resume(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<crate::recovery::RecoveryOffer, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let offer = crate::recovery::resolve_resume(&store, run_id).map_err(|e| e.to_string())?;
    if let Ok(mut last) = state.last_recovery.lock() {
        if let Some(report) = last.as_mut() {
            report.offer = Some(offer.clone());
        }
    }
    Ok(offer)
}

#[tauri::command]
pub fn recovery_cancel(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<crate::recovery::RecoveryOffer, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let offer = crate::recovery::resolve_cancel(&store, run_id).map_err(|e| e.to_string())?;
    if let Ok(mut last) = state.last_recovery.lock() {
        if let Some(report) = last.as_mut() {
            report.offer = Some(offer.clone());
        }
    }
    Ok(offer)
}

#[tauri::command]
pub fn probe_disk_pressure(
    state: State<'_, AppState>,
    threshold_bytes: Option<u64>,
) -> Result<crate::recovery::DiskPressureReport, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let path = state
        .workspace_parent
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| store.artifact_root().to_path_buf());
    let threshold = threshold_bytes.unwrap_or(crate::recovery::DEFAULT_LOW_DISK_THRESHOLD_BYTES);
    let report = crate::recovery::probe_disk_pressure(&path, threshold);
    if report.low_disk {
        for run in store.list_runs().map_err(|e| e.to_string())? {
            if matches!(
                run.status.as_str(),
                "completed" | "failed" | "cancelled" | "created"
            ) {
                continue;
            }
            let _ = store.renew_scheduler_lease(run.run_id, "tiamat-disk", 3, Some(true));
            let _ = store.set_scheduler_flags(run.run_id, None, Some(true));
        }
    }
    Ok(report)
}

#[tauri::command]
pub fn set_fault_injection(
    rules: Vec<crate::recovery::FaultRule>,
) -> Result<Vec<crate::recovery::FaultRule>, String> {
    crate::recovery::set_faults(rules);
    Ok(crate::recovery::list_faults())
}

#[tauri::command]
pub fn clear_fault_injection() -> Result<(), String> {
    crate::recovery::clear_faults();
    Ok(())
}

#[tauri::command]
pub fn run_fault_injection_fixture(
    state: State<'_, AppState>,
    run_id: String,
    kind: String,
    scope: String,
) -> Result<crate::recovery::SideEffectRecord, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let kind = crate::recovery::SideEffectKind::parse(&kind)
        .ok_or_else(|| format!("unknown side effect kind: {kind}"))?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    if store.get_run(run_id).map_err(|e| e.to_string())?.is_none() {
        store
            .create_run(run_id, "fault-injection", "executing")
            .map_err(|e| e.to_string())?;
    }
    let (record, _) = crate::recovery::execute_idempotent(
        &store,
        run_id,
        kind,
        &scope,
        serde_json::json!({ "fixture": true }),
        || Ok(()),
    )
    .map_err(|e| e.to_string())?;
    Ok(record)
}

#[tauri::command]
pub fn get_retention_settings(
    state: State<'_, AppState>,
) -> Result<crate::recovery::RetentionSettings, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.get_retention_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cleanup_managed_workspace(
    state: State<'_, AppState>,
    force: bool,
) -> Result<serde_json::Value, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let settings = store.get_retention_settings().map_err(|e| e.to_string())?;
    let mut workspace = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no managed workspace".to_string())?;
    // Align manifest retention with durable settings.
    workspace.retention.retain_unpromoted = settings.retain_unpromoted_workspaces;
    workspace.retention.allow_destructive_cleanup = settings.allow_destructive_cleanup || force;
    crate::workspace::assert_can_cleanup(&workspace, force).map_err(|e| e.to_string())?;
    if force && settings.allow_destructive_cleanup {
        crate::workspace::cleanup_managed_run(&workspace, true).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "cleaned": true,
            "path": workspace.managed_run_root,
        }))
    } else {
        Ok(serde_json::json!({
            "cleaned": false,
            "blocked": true,
            "reason": "retention policy blocked destructive cleanup of unpromoted work",
            "path": workspace.managed_run_root,
        }))
    }
}

#[tauri::command]
pub fn scan_prompt_injection(text: String) -> Result<crate::security::InjectionScanResult, String> {
    Ok(crate::security::scan_prompt_injection_markers(&text))
}

#[tauri::command]
pub fn redact_text(text: String) -> Result<serde_json::Value, String> {
    let (redacted, stats) = redact_for_persistence(&text, &[]);
    for secret in FORBIDDEN_FIXTURE_SECRETS {
        if redacted.contains(secret) {
            return Err("redaction failed to remove fixture secret".into());
        }
    }
    Ok(serde_json::json!({
        "text": redacted,
        "originalBytes": stats.original_bytes,
        "redactedBytes": stats.redacted_bytes,
        "contentHash": stats.content_hash,
        "replacementCount": stats.replacement_count(),
    }))
}

#[tauri::command]
pub fn apply_output_limits_fixture(
    text: String,
    max_line_bytes: Option<usize>,
    max_total_bytes: Option<usize>,
) -> Result<crate::security::OutputLimitResult, String> {
    let mut config = crate::security::OutputLimitConfig::default();
    if let Some(v) = max_line_bytes {
        config.max_line_bytes = v;
    }
    if let Some(v) = max_total_bytes {
        config.max_total_bytes = v;
    }
    Ok(crate::security::apply_output_limits(&text, &config))
}

#[tauri::command]
pub fn plan_uninstall_retention(
    state: State<'_, AppState>,
) -> Result<crate::packaging::UninstallPlan, String> {
    let manifests = {
        let guard = state.last_workspace.lock().map_err(|e| e.to_string())?;
        guard.clone().into_iter().collect::<Vec<_>>()
    };
    Ok(crate::packaging::plan_uninstall_retention(&manifests))
}

#[tauri::command]
pub fn simulate_upgrade_preserve(
    app_data_root: String,
    previous_version: String,
    next_version: String,
) -> Result<crate::packaging::UpgradePreserveResult, String> {
    crate::packaging::simulate_upgrade_preserve(
        std::path::Path::new(&app_data_root),
        &previous_version,
        &next_version,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_long_path_fixture(root: String) -> Result<String, String> {
    let path = crate::packaging::create_long_path_fixture(std::path::Path::new(&root))
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn prove_packaged_cleanup(
    state: State<'_, AppState>,
    run_id: String,
    out_dir: String,
) -> Result<crate::packaging::PackagedCleanupReport, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let active = crate::packaging::assert_zero_owned_processes(&store, Some(run_id))
        .map_err(|e| e.to_string())?;
    let proofs = store
        .list_cleanup_proofs(run_id)
        .map_err(|e| e.to_string())?;
    let mut report = crate::packaging::PackagedCleanupReport {
        run_id,
        active_process_count: active,
        zero_owned_processes: active == 0,
        proofs,
        artifact_path: None,
    };
    let path =
        crate::packaging::write_cleanup_proof_artifact(std::path::Path::new(&out_dir), &report)
            .map_err(|e| e.to_string())?;
    report.artifact_path = Some(path.to_string_lossy().to_string());
    Ok(report)
}

#[tauri::command]
pub fn materialize_testbench(dest: String) -> Result<serde_json::Value, String> {
    let src = tiamat_contracts::repo_root()
        .join("fixtures")
        .join("testbench");
    let dest_path = std::path::PathBuf::from(&dest);
    copy_dir_recursive(&src, &dest_path).map_err(|e| e.to_string())?;
    let long =
        crate::packaging::create_long_path_fixture(&dest_path.join("long-path").join(".generated"))
            .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "destination": dest,
        "longPathMarker": long.to_string_lossy(),
        "cases": [
            "notes-only",
            "web-app",
            "multi-project",
            "dirty-git",
            "nested-repo",
            "secret-risk",
            "junction-escape",
            "unicode-项目",
            "long-path",
            "executor-app"
        ]
    }))
}

fn copy_dir_recursive(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dest.join(entry.file_name());
        if ty.is_dir() {
            if entry.file_name() == ".generated" || entry.file_name() == ".git" {
                continue;
            }
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn run_process_fixture(
    state: State<'_, AppState>,
    run_id: String,
    mode: String,
    warn_after_ms: Option<u64>,
    graceful_after_ms: Option<u64>,
    force_grace_ms: Option<u64>,
) -> Result<crate::process::HostedProcessOutcome, String> {
    let run_id = Uuid::parse_str(&run_id).map_err(|e| e.to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let fixture = tiamat_contracts::repo_root()
        .join("fixtures")
        .join("cursor-cli")
        .join("fake-agent.cmd");
    let argv = vec![fixture.to_string_lossy().to_string()];
    let mut watchdog = crate::process::WatchdogConfig::for_tests();
    if let Some(v) = warn_after_ms {
        watchdog.warn_after_ms = v;
    }
    if let Some(v) = graceful_after_ms {
        watchdog.graceful_after_ms = v;
    }
    if let Some(v) = force_grace_ms {
        watchdog.force_grace_ms = v;
    }
    let outcome = state
        .process_host
        .run_hosted(
            &store,
            crate::process::SpawnRequest {
                run_id,
                phase_id: Some("P07".into()),
                attempt_id: None,
                argv,
                stdin: Some(String::new()),
                workspace: None,
                env: vec![("TIAMAT_FAKE_CLI_MODE".into(), mode)],
                watchdog,
                resume_chat_hint: Some("chat-timeout-fixture".into()),
                next_model_on_timeout: Some("cursor-grok-4.5-low".into()),
                next_tier_on_timeout: Some("grok-low".into()),
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(outcome)
}

fn emit_scheduler_event(
    app: &AppHandle,
    state: &State<'_, AppState>,
    event_type: &str,
    level: EventLevel,
    message: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    // Prefer the active orchestrator run, then the most recent non-terminal run,
    // then the latest run — never silently pick an arbitrary first row.
    let run_id = {
        let orch = state.orchestrator.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = orch.as_ref() {
            Some(handle.run_id())
        } else {
            None
        }
    };
    let run_id = if let Some(id) = run_id {
        id
    } else {
        let runs = store.list_runs().map_err(|e| e.to_string())?;
        let active = runs
            .iter()
            .rev()
            .find(|r| !matches!(r.status.as_str(), "completed" | "failed" | "cancelled"));
        match active.or_else(|| runs.last()) {
            Some(r) => r.run_id,
            None => return Ok(()),
        }
    };
    let event = NewEvent {
        event_id: Uuid::new_v4(),
        run_id,
        project_id: Some("tiamat".into()),
        phase_id: payload
            .get("phaseId")
            .and_then(|v| v.as_str())
            .map(str::to_string),
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
    store
        .mark_outbox_delivered(&[envelope.event_id])
        .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_CHANNEL, &envelope);
    Ok(())
}

fn emit_planner_event(
    app: &AppHandle,
    state: &State<'_, AppState>,
    event_type: &str,
    level: EventLevel,
    message: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let runs = store.list_runs().map_err(|e| e.to_string())?;
    let Some(run) = runs.into_iter().next() else {
        return Ok(());
    };
    let event = NewEvent {
        event_id: Uuid::new_v4(),
        run_id: run.run_id,
        project_id: Some("tiamat".into()),
        phase_id: Some("P05".into()),
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
    store
        .mark_outbox_delivered(&[envelope.event_id])
        .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_CHANNEL, &envelope);
    Ok(())
}

pub(crate) fn emit_workspace_event(
    app: &AppHandle,
    state: &State<'_, AppState>,
    event_type: &str,
    level: EventLevel,
    message: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let runs = store.list_runs().map_err(|e| e.to_string())?;
    let Some(run) = runs.into_iter().next() else {
        return Ok(());
    };
    let event = NewEvent {
        event_id: Uuid::new_v4(),
        run_id: run.run_id,
        project_id: Some("tiamat".into()),
        phase_id: Some("P04".into()),
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
    store
        .mark_outbox_delivered(&[envelope.event_id])
        .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_CHANNEL, &envelope);
    Ok(())
}

fn emit_intake_event(
    app: &AppHandle,
    state: &State<'_, AppState>,
    event_type: &str,
    level: EventLevel,
    message: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let runs = store.list_runs().map_err(|e| e.to_string())?;
    let Some(run) = runs.into_iter().next() else {
        return Ok(());
    };
    let event = NewEvent {
        event_id: Uuid::new_v4(),
        run_id: run.run_id,
        project_id: Some("tiamat".into()),
        phase_id: Some("P02".into()),
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
    store
        .mark_outbox_delivered(&[envelope.event_id])
        .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_CHANNEL, &envelope);
    Ok(())
}

pub fn init_store(app: &AppHandle) -> Result<Store, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("tiamat");
    let db_path = data_dir.join("tiamat.db");
    let artifact_root = data_dir.join("artifacts");
    Store::open(db_path, artifact_root).map_err(|e| e.to_string())
}

pub fn open_store_at(root: PathBuf) -> Result<Store, String> {
    let db_path = root.join("tiamat.db");
    let artifact_root = root.join("artifacts");
    Store::open(db_path, artifact_root).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn get_app_info_reports_fake_orchestrator() {
        let dir = tempdir().unwrap();
        let store =
            Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
        let state = AppState {
            store: Mutex::new(store),
            last_preflight: Mutex::new(None),
            last_cursor: Mutex::new(None),
            last_workspace: Mutex::new(None),
            last_plan: Mutex::new(None),
            last_architect: Mutex::new(None),
            last_scheduler: Mutex::new(None),
            last_executor: Mutex::new(None),
            last_recovery: Mutex::new(None),
            workspace_parent: Mutex::new(None),
            process_host: crate::process::ProcessHost::new(),
            abort: crate::process::AbortController::new(),
            orchestrator: Mutex::new(None),
        };
        // Unit-test the info payload shape without Tauri State wrapper.
        let info = AppInfo {
            name: "Tiamat".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
            orchestrator_mode: crate::scheduler::ORCHESTRATOR_MODE.to_string(),
            store_schema_version: state.store.lock().unwrap().schema_version().unwrap(),
        };
        assert_eq!(info.name, "Tiamat");
        assert_eq!(info.orchestrator_mode, "dag-scheduler");
        assert_eq!(info.schema_version, 1);
        assert!(info.store_schema_version >= 6);
    }

    #[test]
    fn validate_contract_json_accepts_valid_intake_fixture() {
        let fixture = std::fs::read_to_string(
            tiamat_contracts::repo_root().join("fixtures/contracts/v1/intake-manifest.valid.json"),
        )
        .expect("fixture");
        let result = validate_contract_json("intake-manifest".to_string(), fixture);
        assert!(result.valid);
        assert!(result.error.is_none());
    }

    #[test]
    fn validate_contract_json_rejects_invalid_fixture() {
        let fixture = std::fs::read_to_string(
            tiamat_contracts::repo_root()
                .join("fixtures/contracts/v1/invalid/intake-wrong-schema-version.json"),
        )
        .expect("fixture");
        let result = validate_contract_json("intake-manifest".to_string(), fixture);
        assert!(!result.valid);
        assert!(result.error.is_some());
    }

    #[test]
    fn ensure_demo_run_is_idempotent() {
        let dir = tempdir().unwrap();
        let store =
            Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
        let (run_a, events_a) = db::ensure_demo_run(&store).unwrap();
        let (run_b, events_b) = db::ensure_demo_run(&store).unwrap();
        assert_eq!(run_a.run_id, run_b.run_id);
        assert_eq!(events_a.len(), events_b.len());
        assert!(events_a.len() >= 4);
        assert_eq!(events_a[0].sequence, 1);
    }
}
