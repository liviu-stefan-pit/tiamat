use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tiamat_contracts::{
    EvidenceClassification, EvidenceRecord, PhasePlan, PhaseResult, PhaseResultStatus, PhaseStatus,
    ProjectPlan, TestKind,
};
use uuid::Uuid;

use crate::cursor::{
    build_cursor_command, parse_stream_json, CursorCapabilityReport, CursorInvokeRequest,
    ProcessCapture, DEFAULT_CURSOR_TIMEOUT_MS,
};
use crate::db::Store;
use crate::executor::diff::{
    collect_changed_files, find_new_escapes, snapshot_paths, validate_diff_boundaries,
};
use crate::executor::error::{ExecutorError, ExecutorResult};
use crate::executor::prompt::{assemble_phase_prompt, assemble_recovery_prompt};
use crate::executor::recover::{decide_partial_recovery, PartialRecoveryDecision};
use crate::executor::result::extract_phase_result;
use crate::executor::types::{ExecutionMode, PhaseExecutionOutcome, RecoveryReport};
use crate::planner::{render_master_plan_markdown, write_plan_artifacts};
use crate::process::{
    run_capture_hosted, watchdog_for_timeout, HostedSpawnContext, ProcessHost, SpawnRequest,
};
use crate::security::redact_line;
use crate::verification::{run_phase_gates, GateRunOptions};
use crate::workspace::{
    create_checkpoint, create_control_checkpoint, quarantine_path, rollback_to_checkpoint,
    write_manifest, RunWorkspaceManifest,
};

pub struct ExecutePhaseRequest<'a> {
    pub run_id: Uuid,
    pub attempt_id: Option<Uuid>,
    pub plan: &'a mut ProjectPlan,
    pub phase_id: &'a str,
    pub workspace: &'a mut RunWorkspaceManifest,
    pub capability: &'a CursorCapabilityReport,
    pub model_id: &'a str,
    pub mode: ExecutionMode,
    pub interruption_report: Option<&'a str>,
    pub resume_chat_id: Option<&'a str>,
    /// Optional override executable (node + fake-agent.mjs) for tests.
    pub executable_override: Option<&'a str>,
    pub fake_cli_mode: Option<&'a str>,
    pub timeout_ms: Option<u64>,
    pub establish_baseline: bool,
    pub flaky_retry: bool,
    /// Production: AppState ProcessHost + Store. Tests may omit (ephemeral hosted spawn).
    pub host: Option<HostedSpawnContext<'a>>,
}

/// Execute one phase in its assigned managed root with verification gates and orchestrator projection.
pub fn execute_phase(req: ExecutePhaseRequest<'_>) -> ExecutorResult<PhaseExecutionOutcome> {
    let mut notes = Vec::new();
    let phase_idx = req
        .plan
        .phases
        .iter()
        .position(|p| p.phase_id == req.phase_id)
        .ok_or_else(|| ExecutorError::Message(format!("phase {} not in plan", req.phase_id)))?;

    req.plan.phases[phase_idx].status = PhaseStatus::Running;
    project_plan_files(req.workspace, req.plan)?;
    notes.push("plan projected: status=running".into());

    let phase = req.plan.phases[phase_idx].clone();
    let project_id = phase
        .project_ids
        .first()
        .cloned()
        .ok_or_else(|| ExecutorError::Message("phase missing projectIds".into()))?;
    let managed = req
        .workspace
        .projects
        .iter()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| {
            ExecutorError::Message(format!("managed project {project_id} not in workspace"))
        })?
        .clone();
    let write_root = PathBuf::from(&managed.write_root);
    let managed_root = PathBuf::from(&managed.managed_root);
    let managed_run_root = PathBuf::from(&req.workspace.managed_run_root);

    let prompt = match req.mode {
        ExecutionMode::Fresh => {
            let ctx = format!(
                "runId={}\nphaseId={}\nwriteRoots={:?}\nreadRoots={:?}",
                req.run_id, phase.phase_id, phase.write_roots, phase.read_roots
            );
            assemble_phase_prompt(&phase, &ctx)
        }
        ExecutionMode::Resume => assemble_recovery_prompt(
            &phase,
            req.interruption_report.unwrap_or("interrupted attempt"),
        ),
    };

    let executable = req
        .executable_override
        .map(|s| s.to_string())
        .or_else(|| req.capability.executable.clone())
        .ok_or_else(|| ExecutorError::Message("Cursor CLI executable missing".into()))?;

    let invoke = CursorInvokeRequest {
        workspace: write_root.display().to_string(),
        model: Some(req.model_id.to_string()),
        prompt,
        trust: true,
        force: true,
        auto_review: false,
        plan_mode: false,
        resume_chat_id: req.resume_chat_id.map(|s| s.to_string()),
        output_format: Some("stream-json".into()),
        timeout_ms: req.timeout_ms.or(Some(DEFAULT_CURSOR_TIMEOUT_MS)),
        api_key: None,
    };

    let built = build_cursor_command(
        &executable,
        &req.capability.features,
        &invoke,
        Some(Path::new(&req.workspace.managed_run_root)),
    )
    .map_err(|e| ExecutorError::Message(e.to_string()))?;

    let (argv, stdin) = expand_executable_override(&built.argv, Some(built.stdin.as_str()));
    let mut env = HashMap::new();
    if let Some(mode) = req.fake_cli_mode {
        env.insert("TIAMAT_FAKE_CLI_MODE".into(), mode.to_string());
    }
    env.insert(
        "TIAMAT_FAKE_WRITE_ROOT".into(),
        write_root.display().to_string(),
    );
    env.insert(
        "TIAMAT_FAKE_MANAGED_RUN_ROOT".into(),
        managed_run_root.display().to_string(),
    );
    env.insert("TIAMAT_FAKE_PHASE_ID".into(), phase.phase_id.clone());

    let before_snapshot = snapshot_paths(&managed_run_root, 6);
    let timeout = req.timeout_ms.unwrap_or(60_000);
    let write_root_str = write_root.display().to_string();
    let capture = run_phase_agent_hosted(
        req.host.as_ref(),
        req.run_id,
        Some(phase.phase_id.as_str()),
        req.attempt_id,
        &argv,
        timeout,
        stdin,
        &env,
        Some(write_root_str.as_str()),
    )
    .map_err(ExecutorError::Message)?;
    let parsed = parse_stream_json(&capture.stdout, &capture.stderr, &[]);
    let chat_id = parsed.chat_id.clone();

    if capture.timed_out {
        return handle_timeout(
            req,
            &phase,
            &project_id,
            &managed_root,
            &write_root,
            chat_id,
            notes,
        );
    }

    let mut changed = collect_changed_files(&managed_root)?;
    let after_snapshot = snapshot_paths(&managed_run_root, 6);
    let mut escaped = find_new_escapes(
        &managed_run_root,
        &phase.write_roots,
        &before_snapshot,
        &after_snapshot,
    );
    let mut boundary = validate_diff_boundaries(&managed_root, &phase.write_roots, &changed)?;
    if !escaped.is_empty() {
        boundary.ok = false;
        boundary.escaped_paths.append(&mut escaped);
    }

    if !boundary.ok {
        let escape_target = boundary
            .escaped_paths
            .first()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .unwrap_or_else(|| managed_root.clone());
        let q = quarantine_path(
            req.workspace,
            &project_id,
            &escape_target,
            &format!("boundary escape: {}", boundary.escaped_paths.join(", ")),
            None,
        )?;
        write_manifest(req.workspace)?;
        req.plan.phases[phase_idx].status = PhaseStatus::Failed;
        project_plan_files(req.workspace, req.plan)?;
        notes.push("quarantined after boundary escape; checkpoint refused".into());
        return Ok(PhaseExecutionOutcome {
            ok: false,
            run_id: req.run_id,
            phase_id: phase.phase_id,
            attempt_id: req.attempt_id,
            terminal_status: PhaseStatus::Failed,
            phase_result: None,
            evidence: vec![],
            layers: vec![],
            changed_files: changed,
            boundary_ok: false,
            quarantined: Some(q),
            project_checkpoint: None,
            control_checkpoint: None,
            plan_projected: true,
            recovery: Some(RecoveryReport {
                decision: "quarantine".into(),
                progress_useful: false,
                reason: "out-of-bound edits".into(),
                resumed: false,
                rolled_back: false,
            }),
            chat_id,
            message: "Out-of-bound edits quarantined; phase failed without checkpoint".into(),
            evidence_notes: notes,
        });
    }

    req.plan.phases[phase_idx].status = PhaseStatus::Verifying;
    project_plan_files(req.workspace, req.plan)?;
    notes.push("plan projected: status=verifying".into());

    let gate_report = run_phase_gates(
        &phase,
        GateRunOptions {
            write_root: &write_root,
            establish_baseline: req.establish_baseline,
            flaky_retry: req.flaky_retry,
            extra_env: HashMap::new(),
            host: req.host.as_ref().map(|h| (h.store, h.host, req.run_id)),
            phase_id: Some(phase.phase_id.as_str()),
            attempt_id: req.attempt_id,
        },
    )
    .map_err(|e| ExecutorError::Verification(e.to_string()))?;

    let mut evidence = gate_report.evidence;
    evidence.push(diff_evidence(&changed, boundary.ok));

    if !gate_report.all_required_passed {
        req.plan.phases[phase_idx].status = PhaseStatus::Failed;
        req.plan.phases[phase_idx].evidence =
            evidence.iter().map(|e| e.evidence_id.clone()).collect();
        project_plan_files(req.workspace, req.plan)?;
        let _ = create_control_checkpoint(
            req.workspace,
            &format!("phase {} failed gates", phase.phase_id),
        );
        write_manifest(req.workspace)?;
        notes.push("failed tests prevented pass/checkpoint".into());
        return Ok(PhaseExecutionOutcome {
            ok: false,
            run_id: req.run_id,
            phase_id: phase.phase_id,
            attempt_id: req.attempt_id,
            terminal_status: PhaseStatus::Failed,
            phase_result: None,
            evidence,
            layers: gate_report.layers,
            changed_files: changed,
            boundary_ok: true,
            quarantined: None,
            project_checkpoint: None,
            control_checkpoint: None,
            plan_projected: true,
            recovery: None,
            chat_id,
            message: format!(
                "Verification gates failed: {}",
                gate_report.blocking_failures.join("; ")
            ),
            evidence_notes: notes,
        });
    }

    let phase_result = match extract_phase_result(&parsed.assistant_text)
        .or_else(|_| extract_phase_result(&capture.stdout))
    {
        Ok(r) => r,
        Err(err) => {
            req.plan.phases[phase_idx].status = PhaseStatus::Failed;
            project_plan_files(req.workspace, req.plan)?;
            return Ok(PhaseExecutionOutcome {
                ok: false,
                run_id: req.run_id,
                phase_id: phase.phase_id,
                attempt_id: req.attempt_id,
                terminal_status: PhaseStatus::Failed,
                phase_result: None,
                evidence,
                layers: gate_report.layers,
                changed_files: changed,
                boundary_ok: true,
                quarantined: None,
                project_checkpoint: None,
                control_checkpoint: None,
                plan_projected: true,
                recovery: None,
                chat_id,
                message: err.to_string(),
                evidence_notes: notes,
            });
        }
    };

    // Merge claimed changed files for evidence completeness.
    for f in &phase_result.changed_files {
        if !changed.iter().any(|c| c.eq_ignore_ascii_case(f)) {
            changed.push(f.clone());
        }
    }

    if matches!(phase_result.status, PhaseResultStatus::NeedsReview) {
        req.plan.phases[phase_idx].status = map_result_status_to_phase(&phase_result.status);
        req.plan.phases[phase_idx].evidence =
            evidence.iter().map(|e| e.evidence_id.clone()).collect();
        project_plan_files(req.workspace, req.plan)?;
        write_manifest(req.workspace)?;
        notes.push("phase-result needs_review — scheduling paused for human gate".into());
        return Ok(PhaseExecutionOutcome {
            ok: false,
            run_id: req.run_id,
            phase_id: phase.phase_id,
            attempt_id: req.attempt_id,
            terminal_status: PhaseStatus::NeedsReview,
            phase_result: Some(phase_result),
            evidence,
            layers: gate_report.layers,
            changed_files: changed,
            boundary_ok: true,
            quarantined: None,
            project_checkpoint: None,
            control_checkpoint: None,
            plan_projected: true,
            recovery: None,
            chat_id,
            message: "phase-result status is needs_review".into(),
            evidence_notes: notes,
        });
    }

    if !matches!(phase_result.status, PhaseResultStatus::Passed) {
        req.plan.phases[phase_idx].status = map_result_status_to_phase(&phase_result.status);
        req.plan.phases[phase_idx].evidence =
            evidence.iter().map(|e| e.evidence_id.clone()).collect();
        project_plan_files(req.workspace, req.plan)?;
        write_manifest(req.workspace)?;
        return Ok(PhaseExecutionOutcome {
            ok: false,
            run_id: req.run_id,
            phase_id: phase.phase_id,
            attempt_id: req.attempt_id,
            terminal_status: PhaseStatus::Failed,
            phase_result: Some(phase_result),
            evidence,
            layers: gate_report.layers,
            changed_files: changed,
            boundary_ok: true,
            quarantined: None,
            project_checkpoint: None,
            control_checkpoint: None,
            plan_projected: true,
            recovery: None,
            chat_id,
            message: "phase-result status is not passed".into(),
            evidence_notes: notes,
        });
    }

    // Checkpoint only after orchestrator accepts immutable result and updates projections.
    accept_phase_result(req.plan, phase_idx, &phase_result, &evidence)?;
    project_plan_files(req.workspace, req.plan)?;
    notes.push("orchestrator accepted immutable phase-result; plan projected".into());

    let control_cp = create_control_checkpoint(
        req.workspace,
        &format!("phase {} plan projection", phase.phase_id),
    )?;
    let project_cp = create_checkpoint(
        req.workspace,
        &project_id,
        &format!("phase {} passed gates", phase.phase_id),
    )?;
    write_manifest(req.workspace)?;
    notes.push(format!(
        "checkpoints created control={} project={}",
        control_cp.checkpoint_id, project_cp.checkpoint_id
    ));

    Ok(PhaseExecutionOutcome {
        ok: true,
        run_id: req.run_id,
        phase_id: phase.phase_id,
        attempt_id: req.attempt_id,
        terminal_status: PhaseStatus::Passed,
        phase_result: Some(phase_result),
        evidence,
        layers: gate_report.layers,
        changed_files: changed,
        boundary_ok: true,
        quarantined: None,
        project_checkpoint: Some(project_cp),
        control_checkpoint: Some(control_cp),
        plan_projected: true,
        recovery: None,
        chat_id,
        message: "Phase passed: gates green, plan projected, checkpoints created".into(),
        evidence_notes: notes,
    })
}

fn handle_timeout(
    req: ExecutePhaseRequest<'_>,
    phase: &PhasePlan,
    project_id: &str,
    managed_root: &Path,
    write_root: &Path,
    chat_id: Option<String>,
    mut notes: Vec<String>,
) -> ExecutorResult<PhaseExecutionOutcome> {
    let changed = collect_changed_files(managed_root).unwrap_or_default();
    let boundary = validate_diff_boundaries(managed_root, &phase.write_roots, &changed)?;
    let progress_useful = !changed.is_empty() && boundary.ok;
    let decision = decide_partial_recovery(
        progress_useful,
        boundary.ok,
        phase.rollback.strategy.clone(),
        None,
        None,
    );

    let phase_idx = req
        .plan
        .phases
        .iter()
        .position(|p| p.phase_id == phase.phase_id)
        .unwrap();

    match &decision {
        PartialRecoveryDecision::Quarantine { reason } => {
            let q = quarantine_path(req.workspace, project_id, managed_root, reason, None)?;
            req.plan.phases[phase_idx].status = PhaseStatus::Failed;
            project_plan_files(req.workspace, req.plan)?;
            write_manifest(req.workspace)?;
            notes.push(format!("timeout → quarantine: {reason}"));
            Ok(outcome_timeout(
                req,
                phase,
                changed,
                boundary.ok,
                Some(q),
                RecoveryReport {
                    decision: "quarantine".into(),
                    progress_useful,
                    reason: reason.clone(),
                    resumed: false,
                    rolled_back: false,
                },
                chat_id,
                notes,
                "Timed-out partial work quarantined",
            ))
        }
        PartialRecoveryDecision::Rollback => {
            if let Some(cp) = req
                .workspace
                .checkpoints
                .iter()
                .rev()
                .find(|c| c.project_id == project_id)
                .cloned()
            {
                let _ = rollback_to_checkpoint(req.workspace, project_id, &cp.checkpoint_id);
            }
            req.plan.phases[phase_idx].status = PhaseStatus::Failed;
            project_plan_files(req.workspace, req.plan)?;
            write_manifest(req.workspace)?;
            notes.push("timeout → rollback to prior checkpoint".into());
            Ok(outcome_timeout(
                req,
                phase,
                changed,
                boundary.ok,
                None,
                RecoveryReport {
                    decision: "rollback".into(),
                    progress_useful: false,
                    reason: "timeout without useful progress".into(),
                    resumed: false,
                    rolled_back: true,
                },
                chat_id,
                notes,
                "Timed-out work rolled back; no pass checkpoint",
            ))
        }
        PartialRecoveryDecision::Resume { progress_useful } => {
            req.plan.phases[phase_idx].status = PhaseStatus::Failed;
            project_plan_files(req.workspace, req.plan)?;
            write_manifest(req.workspace)?;
            notes.push(format!(
                "timeout → resume eligible (progress_useful={progress_useful}); write_root={}",
                write_root.display()
            ));
            Ok(outcome_timeout(
                req,
                phase,
                changed,
                boundary.ok,
                None,
                RecoveryReport {
                    decision: "resume".into(),
                    progress_useful: *progress_useful,
                    reason: "timeout with recoverable progress".into(),
                    resumed: false,
                    rolled_back: false,
                },
                chat_id,
                notes,
                "Timed-out with useful progress; resume required (no pass checkpoint)",
            ))
        }
        PartialRecoveryDecision::Fail { reason } => {
            req.plan.phases[phase_idx].status = PhaseStatus::Failed;
            project_plan_files(req.workspace, req.plan)?;
            Ok(outcome_timeout(
                req,
                phase,
                changed,
                boundary.ok,
                None,
                RecoveryReport {
                    decision: "fail".into(),
                    progress_useful: false,
                    reason: reason.clone(),
                    resumed: false,
                    rolled_back: false,
                },
                chat_id,
                notes,
                reason,
            ))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn outcome_timeout(
    req: ExecutePhaseRequest<'_>,
    phase: &PhasePlan,
    changed: Vec<String>,
    boundary_ok: bool,
    quarantined: Option<crate::workspace::QuarantineRecord>,
    recovery: RecoveryReport,
    chat_id: Option<String>,
    notes: Vec<String>,
    message: &str,
) -> PhaseExecutionOutcome {
    PhaseExecutionOutcome {
        ok: false,
        run_id: req.run_id,
        phase_id: phase.phase_id.clone(),
        attempt_id: req.attempt_id,
        terminal_status: PhaseStatus::Failed,
        phase_result: None,
        evidence: vec![],
        layers: vec![],
        changed_files: changed,
        boundary_ok,
        quarantined,
        project_checkpoint: None,
        control_checkpoint: None,
        plan_projected: true,
        recovery: Some(recovery),
        chat_id,
        message: message.into(),
        evidence_notes: notes,
    }
}

fn accept_phase_result(
    plan: &mut ProjectPlan,
    phase_idx: usize,
    result: &PhaseResult,
    evidence: &[EvidenceRecord],
) -> ExecutorResult<()> {
    if !result.immutable {
        return Err(ExecutorError::InvalidPhaseResult(
            "refusing mutable phase result".into(),
        ));
    }
    plan.phases[phase_idx].status = PhaseStatus::Passed;
    plan.phases[phase_idx].evidence = evidence.iter().map(|e| e.evidence_id.clone()).collect();
    if plan.phases[phase_idx].evidence.is_empty() {
        plan.phases[phase_idx].evidence = result.evidence_ids.clone();
    }
    Ok(())
}

fn project_plan_files(workspace: &RunWorkspaceManifest, plan: &ProjectPlan) -> ExecutorResult<()> {
    let control = PathBuf::from(&workspace.control_root);
    write_plan_artifacts(&control, plan)?;
    let md = render_master_plan_markdown(plan);
    if !md.contains(&plan.title) {
        return Err(ExecutorError::Message(
            "plan markdown projection mismatch".into(),
        ));
    }
    Ok(())
}

fn diff_evidence(changed: &[String], boundary_ok: bool) -> EvidenceRecord {
    let now = chrono::Utc::now().to_rfc3339();
    EvidenceRecord {
        schema_version: 1,
        evidence_id: format!("ev-diff-{}", Uuid::new_v4()),
        kind: TestKind::Diff,
        test_id: None,
        command: vec!["git".into(), "status".into(), "--porcelain".into()],
        working_directory: ".".into(),
        exit_code: if boundary_ok { 0 } else { 1 },
        duration_ms: 0,
        summary: redact_line(&format!(
            "changed={} boundary_ok={boundary_ok}",
            changed.join(",")
        )),
        artifact_hashes: vec![],
        covers: vec![],
        trustworthy: true,
        partial: false,
        classification: if boundary_ok {
            EvidenceClassification::Pass
        } else {
            EvidenceClassification::Fail
        },
        started_at_utc: now.clone(),
        ended_at_utc: now,
        baseline_exit_code: None,
        flaky_retry: None,
    }
}

fn expand_executable_override<'a>(
    argv: &'a [String],
    stdin: Option<&'a str>,
) -> (Vec<String>, Option<&'a str>) {
    if argv.is_empty() {
        return (argv.to_vec(), stdin);
    }
    if argv[0].contains('|') {
        let mut parts: Vec<String> = argv[0].split('|').map(|s| s.to_string()).collect();
        parts.extend(argv.iter().skip(1).cloned());
        return (parts, stdin);
    }
    (argv.to_vec(), stdin)
}

/// Map immutable phase-result status onto orchestrator PhaseStatus (ARCH-003).
pub fn map_result_status_to_phase(status: &PhaseResultStatus) -> PhaseStatus {
    match status {
        PhaseResultStatus::Passed => PhaseStatus::Passed,
        PhaseResultStatus::NeedsReview => PhaseStatus::NeedsReview,
        PhaseResultStatus::Failed => PhaseStatus::Failed,
    }
}

/// Production Cursor/agent spawn via ProcessHost (Job Object + registry + cleanup proof).
/// When `host` is None (unit tests), uses an ephemeral in-memory store + host so work
/// remains Job-associated rather than bare `Command::spawn`.
#[allow(clippy::too_many_arguments)]
fn run_phase_agent_hosted(
    host: Option<&HostedSpawnContext<'_>>,
    run_id: Uuid,
    phase_id: Option<&str>,
    attempt_id: Option<Uuid>,
    argv: &[String],
    timeout_ms: u64,
    stdin: Option<&str>,
    env: &HashMap<String, String>,
    workspace: Option<&str>,
) -> Result<ProcessCapture, String> {
    let env_vec: Vec<(String, String)> = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let request = SpawnRequest {
        run_id,
        phase_id: phase_id.map(str::to_string),
        // Process registry FK references attempts(attempt_id); callers may not have
        // inserted an attempts row yet. Carry attempt via phase_id / events instead.
        attempt_id: None,
        argv: argv.to_vec(),
        stdin: stdin.map(str::to_string),
        workspace: workspace.map(str::to_string),
        env: env_vec,
        watchdog: watchdog_for_timeout(timeout_ms),
        resume_chat_hint: None,
        next_model_on_timeout: None,
        next_tier_on_timeout: None,
    };
    let _ = attempt_id;
    match host {
        Some(ctx) => {
            // Ensure run row exists for FK/event paths.
            let _ = ctx.store.create_run(run_id, "phase-exec", "executing");
            run_capture_hosted(ctx.store, ctx.host, request).map_err(|e| e.to_string())
        }
        None => {
            let dir = std::env::temp_dir().join(format!("tiamat-ephemeral-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let store = Store::open_in_memory(&dir).map_err(|e| e.to_string())?;
            let _ = store.create_run(run_id, "phase-exec-ephemeral", "executing");
            let ph = ProcessHost::new();
            run_capture_hosted(&store, &ph, request).map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_review_maps_to_phase_needs_review_not_failed() {
        assert_eq!(
            map_result_status_to_phase(&PhaseResultStatus::NeedsReview),
            PhaseStatus::NeedsReview
        );
        assert_eq!(
            map_result_status_to_phase(&PhaseResultStatus::Failed),
            PhaseStatus::Failed
        );
        assert_eq!(
            map_result_status_to_phase(&PhaseResultStatus::Passed),
            PhaseStatus::Passed
        );
    }
}
