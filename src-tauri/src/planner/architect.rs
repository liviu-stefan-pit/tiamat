use std::path::{Path, PathBuf};

use tiamat_contracts::ProjectPlan;
use uuid::Uuid;

use std::collections::HashMap;

use crate::cursor::{
    parse_stream_json, prepare_hosted_cursor_argv, CursorAuthStatus, CursorCapabilityReport,
    ProcessCapture,
};
use crate::db::Store;
use crate::intake::PreflightReport;
use crate::planner::context::package_architect_context;
use crate::planner::invoke::build_architect_command;
use crate::planner::model::select_architect_model;
use crate::planner::parse::extract_final_json_object;
use crate::planner::persist::{checkpoint_control_plan, write_plan_artifacts};
use crate::planner::prompt::{repair_prompt, ARCHITECT_SYSTEM_PROMPT};
use crate::planner::types::{
    ArchitectAttemptRecord, ArchitectRunResult, GraphEdge, GraphNode, GraphProjection,
};
use crate::planner::validate::validate_plan_json;
use crate::process::{
    run_capture_hosted, watchdog_for_timeout, HostedSpawnContext, ProcessHost, SpawnRequest,
};
use crate::security::{check_prompt_size, redact_line, OutputLimitConfig};
use crate::workspace::{write_manifest, RunWorkspaceManifest};

fn architect_timeout_ms() -> u64 {
    crate::cursor::TimeoutSettings::from_env().architect_timeout_ms
}
const STDERR_EXCERPT_LIMIT: usize = 800;

pub struct ArchitectPipelineRequest<'a> {
    pub run_id: Uuid,
    pub preflight: &'a PreflightReport,
    pub workspace: &'a mut RunWorkspaceManifest,
    pub capability: &'a CursorCapabilityReport,
    /// Optional override executable argv prefix for tests (e.g. node + fake-agent.mjs).
    pub executable_override: Option<&'a str>,
    /// Test-only fake CLI mode (`TIAMAT_FAKE_CLI_MODE`); never set in production.
    pub fake_cli_mode: Option<&'a str>,
    /// Production: AppState ProcessHost + Store. Tests may omit (ephemeral hosted).
    pub host: Option<HostedSpawnContext<'a>>,
}

/// Run the architect → validate → (one repair) → persist → checkpoint pipeline.
pub fn run_architect_pipeline(req: ArchitectPipelineRequest<'_>) -> ArchitectRunResult {
    let mut evidence = Vec::new();
    let mut attempts = Vec::new();

    if let Err(err) = gate_architect_capability(req.capability, req.executable_override.is_some()) {
        return ArchitectRunResult {
            ok: false,
            run_id: req.run_id.to_string(),
            model_selection: crate::planner::types::ArchitectModelSelection {
                requested_model: crate::planner::types::ARCHITECT_PREFERRED_MODEL.into(),
                selected_model: String::new(),
                degraded: false,
                reason: err.clone(),
                available_models: req.capability.models.iter().map(|m| m.id.clone()).collect(),
            },
            plan: None,
            plan_json_path: None,
            master_plan_md_path: None,
            hashes: None,
            checkpoint: None,
            attempts,
            degraded_mode: false,
            error: Some(err),
            evidence,
        };
    }

    let model_selection = match select_architect_model(&req.capability.models) {
        Ok(sel) => sel,
        Err(err) => {
            return ArchitectRunResult {
                ok: false,
                run_id: req.run_id.to_string(),
                model_selection: crate::planner::types::ArchitectModelSelection {
                    requested_model: crate::planner::types::ARCHITECT_PREFERRED_MODEL.into(),
                    selected_model: String::new(),
                    degraded: false,
                    reason: err.clone(),
                    available_models: req.capability.models.iter().map(|m| m.id.clone()).collect(),
                },
                plan: None,
                plan_json_path: None,
                master_plan_md_path: None,
                hashes: None,
                checkpoint: None,
                attempts,
                degraded_mode: false,
                error: Some(err),
                evidence,
            };
        }
    };
    evidence.push(format!(
        "model_selection: requested={} selected={} degraded={}",
        model_selection.requested_model, model_selection.selected_model, model_selection.degraded
    ));

    let executable = match req.executable_override {
        Some(path) => path.to_string(),
        None => match &req.capability.executable {
            Some(path) => path.clone(),
            None => {
                return fail(
                    req.run_id,
                    model_selection,
                    attempts,
                    evidence,
                    "Cursor CLI executable missing".into(),
                );
            }
        },
    };

    let context = package_architect_context(req.preflight, req.workspace);
    evidence.push(format!(
        "context_coverage={} omissions={}",
        context.coverage.len(),
        context.omitted.len()
    ));

    let user_prompt = format!(
        "{ARCHITECT_SYSTEM_PROMPT}\n\n---\n\n# Bounded intake context\n\n{}\n",
        context.text
    );
    if let Err(err) = check_prompt_size(&user_prompt, &OutputLimitConfig::default()) {
        return fail(req.run_id, model_selection, attempts, evidence, err);
    }

    let control_root = PathBuf::from(&req.workspace.control_root);
    let workspace_mount = architect_workspace_mount(req.workspace);

    // Attempt 1
    let (plan, chat_id) = match invoke_and_validate(
        &executable,
        req.capability,
        &workspace_mount,
        &model_selection.selected_model,
        &user_prompt,
        None,
        req.run_id,
        req.workspace,
        false,
        req.executable_override.is_some(),
        req.fake_cli_mode,
        &mut attempts,
        &mut evidence,
        req.host.as_ref(),
    ) {
        Ok(pair) => pair,
        Err(issues) => {
            let parent_chat = attempts.last().and_then(|a| a.chat_id.clone());
            if let Some(parent_chat) = parent_chat {
                // One repair resume only.
                evidence.push("repair_resume_started".into());
                let repair = repair_prompt(&issues);
                match invoke_and_validate(
                    &executable,
                    req.capability,
                    &workspace_mount,
                    &model_selection.selected_model,
                    &repair,
                    Some(parent_chat.as_str()),
                    req.run_id,
                    req.workspace,
                    true,
                    req.executable_override.is_some(),
                    req.fake_cli_mode,
                    &mut attempts,
                    &mut evidence,
                    req.host.as_ref(),
                ) {
                    Ok(pair) => pair,
                    Err(repair_issues) => {
                        return fail(
                            req.run_id,
                            model_selection,
                            attempts,
                            evidence,
                            format!("architect repair failed: {}", repair_issues.join("; ")),
                        );
                    }
                }
            } else {
                // No chat id yet — one fresh full-prompt retry (not --resume).
                evidence.push("fresh_retry_without_chat_id".into());
                match invoke_and_validate(
                    &executable,
                    req.capability,
                    &workspace_mount,
                    &model_selection.selected_model,
                    &user_prompt,
                    None,
                    req.run_id,
                    req.workspace,
                    true,
                    req.executable_override.is_some(),
                    req.fake_cli_mode,
                    &mut attempts,
                    &mut evidence,
                    req.host.as_ref(),
                ) {
                    Ok(pair) => pair,
                    Err(retry_issues) => {
                        return fail(
                            req.run_id,
                            model_selection,
                            attempts,
                            evidence,
                            format!(
                                "architect output invalid and no chat id for repair: {}",
                                retry_issues.join("; ")
                            ),
                        );
                    }
                }
            }
        }
    };
    let _ = chat_id;

    // Persist atomically into control/.tiamat and checkpoint.
    let (json_path, md_path, hashes) = match write_plan_artifacts(&control_root, &plan) {
        Ok(v) => v,
        Err(err) => {
            return fail(
                req.run_id,
                model_selection,
                attempts,
                evidence,
                format!("plan persist failed: {err}"),
            );
        }
    };
    evidence.push(format!("wrote {}", json_path.display()));
    evidence.push(format!("wrote {}", md_path.display()));

    let checkpoint = match checkpoint_control_plan(req.workspace, "initial-architect-plan") {
        Ok(cp) => cp,
        Err(err) => {
            return fail(
                req.run_id,
                model_selection,
                attempts,
                evidence,
                format!("control checkpoint failed: {err}"),
            );
        }
    };
    evidence.push(format!("checkpoint {}", checkpoint.commit));

    if let Err(err) = write_manifest(req.workspace) {
        evidence.push(format!("manifest rewrite warning: {err}"));
    }

    ArchitectRunResult {
        ok: true,
        run_id: req.run_id.to_string(),
        degraded_mode: model_selection.degraded,
        model_selection,
        plan: Some(plan),
        plan_json_path: Some(json_path.display().to_string()),
        master_plan_md_path: Some(md_path.display().to_string()),
        hashes: Some(hashes),
        checkpoint: Some(checkpoint),
        attempts,
        error: None,
        evidence,
    }
}

fn gate_architect_capability(
    capability: &CursorCapabilityReport,
    test_override: bool,
) -> Result<(), String> {
    if test_override {
        return Ok(());
    }
    match capability.status {
        crate::cursor::CursorCapabilityStatus::Absent => {
            return Err(if capability.message.trim().is_empty() {
                "Cursor agent CLI not found. Install `agent`/`cursor-agent`, set TIAMAT_CURSOR_CLI, or configure the path in Settings.".into()
            } else {
                capability.message.clone()
            });
        }
        crate::cursor::CursorCapabilityStatus::Error
        | crate::cursor::CursorCapabilityStatus::UnsupportedVersion => {
            return Err(format!(
                "architect blocked: {}",
                if capability.message.trim().is_empty() {
                    "Cursor CLI probe failed".into()
                } else {
                    capability.message.clone()
                }
            ));
        }
        crate::cursor::CursorCapabilityStatus::Available => {}
    }
    if !capability.features.mode_plan {
        let exe = capability
            .executable
            .as_deref()
            .unwrap_or("unknown executable");
        return Err(format!(
            "Cursor CLI at {exe} does not advertise plan mode (--mode plan); architect cannot start. Point Settings at the `agent` / `cursor-agent` CLI, not the Cursor IDE binary."
        ));
    }
    match capability.auth {
        CursorAuthStatus::Unauthenticated => {
            let detail = capability
                .auth_message
                .clone()
                .unwrap_or_else(|| "Cursor CLI is unauthenticated".into());
            Err(format!("architect blocked: {detail}"))
        }
        CursorAuthStatus::Error => {
            let detail = capability
                .auth_message
                .clone()
                .unwrap_or_else(|| "Cursor auth probe failed".into());
            Err(format!("architect blocked: {detail}"))
        }
        CursorAuthStatus::Ready | CursorAuthStatus::Unknown => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
fn invoke_and_validate(
    executable: &str,
    capability: &CursorCapabilityReport,
    workspace_mount: &Path,
    model: &str,
    prompt: &str,
    resume_chat_id: Option<&str>,
    run_id: Uuid,
    workspace: &RunWorkspaceManifest,
    repaired: bool,
    allow_fake_env: bool,
    fake_cli_mode: Option<&str>,
    attempts: &mut Vec<ArchitectAttemptRecord>,
    evidence: &mut Vec<String>,
    host: Option<&HostedSpawnContext<'_>>,
) -> Result<(ProjectPlan, Option<String>), Vec<String>> {
    let (built, proof) = build_architect_command(
        executable,
        &capability.features,
        workspace_mount,
        model,
        prompt,
        resume_chat_id,
        Some(architect_timeout_ms()),
    )
    .map_err(|e| vec![e])?;

    evidence.push(format!(
        "architect_invoke plan_mode={} force={} argv_len={}",
        proof.plan_mode,
        proof.force,
        proof.argv.len()
    ));
    if !proof.cannot_implement() {
        return Err(vec![
            "architect path violated cannot-implement invariant".into()
        ]);
    }

    // Support `node|path/to/fake-agent.mjs` executable encoding used by resolve/tests.
    let argv = expand_executable_argv(&built.argv);
    let (argv, unwind_env) = prepare_hosted_cursor_argv(&argv);

    evidence.push(format!(
        "architect_argv={}",
        crate::cursor::redaction::redact_argv(&argv).join(" ")
    ));
    if unwind_env.iter().any(|(k, _)| k == "CURSOR_INVOKED_AS") {
        evidence.push("cursor_launcher_unwound_to_node".into());
    }

    let mut env = HashMap::new();
    for (k, v) in unwind_env {
        env.insert(k, v);
    }
    if allow_fake_env {
        if let Some(mode) = fake_cli_mode {
            env.insert("TIAMAT_FAKE_CLI_MODE".into(), mode.to_string());
        }
        env.insert("TIAMAT_FAKE_PLAN_RUN_ID".into(), run_id.to_string());
        if let Some(project) = workspace.projects.first() {
            env.insert(
                "TIAMAT_FAKE_PLAN_PROJECT_ID".into(),
                project.project_id.clone(),
            );
            env.insert(
                "TIAMAT_FAKE_PLAN_WRITE_ROOT".into(),
                project.write_root.clone(),
            );
            env.insert(
                "TIAMAT_FAKE_PLAN_READ_ROOT".into(),
                project
                    .read_roots
                    .first()
                    .cloned()
                    .unwrap_or_else(|| workspace.managed_run_root.clone()),
            );
        } else if let Some(notes) = workspace.notes_roots.first() {
            env.insert("TIAMAT_FAKE_PLAN_PROJECT_ID".into(), "notes".into());
            env.insert("TIAMAT_FAKE_PLAN_WRITE_ROOT".into(), notes.clone());
            env.insert("TIAMAT_FAKE_PLAN_READ_ROOT".into(), notes.clone());
        }
    }

    let capture = run_architect_hosted(
        host,
        run_id,
        &argv,
        built.timeout_ms,
        Some(&built.stdin),
        &env,
        workspace_mount,
        resume_chat_id,
    )
    .map_err(|e| vec![e])?;
    let parsed = parse_stream_json(&capture.stdout, &capture.stderr, &[]);
    let chat_id = parsed
        .chat_id
        .clone()
        .or_else(|| extract_chat_id_from_text(&capture.stderr));

    let mut issues = Vec::new();
    if capture.timed_out {
        issues.push("architect process timed out".into());
    }
    if capture.truncated || capture.flood_detected {
        issues.push("architect stdout/stderr truncated before plan extraction".into());
    }
    if let Some(warning) = &capture.cleanup_warning {
        evidence.push(format!("cleanup_warning: {warning}"));
    }
    if capture.exit_code.unwrap_or(1) != 0 {
        issues.push(format!(
            "architect exited with code {:?}",
            capture.exit_code
        ));
    }
    if parsed.terminal_ok == Some(false) {
        issues.push("architect stream reported terminal error (result subtype=error)".into());
    }
    if let Some(excerpt) = stderr_excerpt(&capture.stderr) {
        issues.push(format!("stderr: {excerpt}"));
        evidence.push(format!("stderr_excerpt: {excerpt}"));
    }
    evidence.push(format!(
        "architect_capture exit={:?} timed_out={} truncated={} duration_ms={} stdout_bytes={} stderr_bytes={}",
        capture.exit_code,
        capture.timed_out,
        capture.truncated,
        capture.duration_ms,
        capture.stdout.len(),
        capture.stderr.len()
    ));

    // Prefer assistant-assembled text; fall back only to plan-shaped JSONL objects.
    let json_text = match extract_final_json_object(&parsed.assistant_text)
        .or_else(|_| extract_plan_json_object_from_stream(&capture.stdout))
    {
        Ok(text) => text,
        Err(err) => {
            issues.push(err);
            attempts.push(ArchitectAttemptRecord {
                attempt: (attempts.len() as u32) + 1,
                model: model.to_string(),
                chat_id: chat_id.clone(),
                usage: parsed.usage.clone(),
                exit_code: capture.exit_code,
                repaired,
                validation_issues: issues
                    .iter()
                    .map(|m| crate::planner::types::PlanValidationIssue {
                        code: "parse".into(),
                        message: m.clone(),
                        phase_id: None,
                    })
                    .collect(),
                proof,
            });
            return Err(issues);
        }
    };

    if !issues.is_empty() && parsed.terminal_ok == Some(false) {
        attempts.push(ArchitectAttemptRecord {
            attempt: (attempts.len() as u32) + 1,
            model: model.to_string(),
            chat_id: chat_id.clone(),
            usage: parsed.usage.clone(),
            exit_code: capture.exit_code,
            repaired,
            validation_issues: issues
                .iter()
                .map(|m| crate::planner::types::PlanValidationIssue {
                    code: "terminal".into(),
                    message: m.clone(),
                    phase_id: None,
                })
                .collect(),
            proof,
        });
        return Err(issues);
    }

    match validate_plan_json(&json_text, run_id, workspace) {
        Ok(plan) => {
            if !issues.is_empty()
                && (capture.timed_out
                    || capture.exit_code.unwrap_or(1) != 0
                    || capture.truncated
                    || parsed.terminal_ok == Some(false))
            {
                attempts.push(ArchitectAttemptRecord {
                    attempt: (attempts.len() as u32) + 1,
                    model: model.to_string(),
                    chat_id: chat_id.clone(),
                    usage: parsed.usage.clone(),
                    exit_code: capture.exit_code,
                    repaired,
                    validation_issues: issues
                        .iter()
                        .map(|m| crate::planner::types::PlanValidationIssue {
                            code: "process".into(),
                            message: m.clone(),
                            phase_id: None,
                        })
                        .collect(),
                    proof,
                });
                return Err(issues);
            }
            attempts.push(ArchitectAttemptRecord {
                attempt: (attempts.len() as u32) + 1,
                model: model.to_string(),
                chat_id: chat_id.clone(),
                usage: parsed.usage.clone(),
                exit_code: capture.exit_code,
                repaired,
                validation_issues: vec![],
                proof,
            });
            Ok((plan, chat_id))
        }
        Err(validation_issues) => {
            let mut messages: Vec<String> = validation_issues
                .iter()
                .map(|i| format!("{}: {}", i.code, i.message))
                .collect();
            messages.extend(issues);
            attempts.push(ArchitectAttemptRecord {
                attempt: (attempts.len() as u32) + 1,
                model: model.to_string(),
                chat_id: chat_id.clone(),
                usage: parsed.usage.clone(),
                exit_code: capture.exit_code,
                repaired,
                validation_issues,
                proof,
            });
            Err(messages)
        }
    }
}

fn expand_executable_argv(argv: &[String]) -> Vec<String> {
    if argv.is_empty() {
        return argv.to_vec();
    }
    let first = &argv[0];
    if let Some((prog, script)) = first.split_once('|') {
        let mut out = vec![prog.to_string(), script.to_string()];
        out.extend(argv.iter().skip(1).cloned());
        out
    } else {
        argv.to_vec()
    }
}

fn architect_workspace_mount(workspace: &RunWorkspaceManifest) -> PathBuf {
    // Prefer notes (rough-spec) then control root — never a product write root alone.
    if let Some(notes) = workspace.notes_roots.first() {
        return PathBuf::from(notes);
    }
    PathBuf::from(&workspace.control_root)
}

fn stderr_excerpt(stderr: &str) -> Option<String> {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return None;
    }
    let redacted = redact_line(trimmed);
    let excerpt = if redacted.len() > STDERR_EXCERPT_LIMIT {
        format!("{}…", &redacted[..STDERR_EXCERPT_LIMIT])
    } else {
        redacted
    };
    Some(excerpt.replace('\n', " | "))
}

fn extract_chat_id_from_text(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            for key in [
                "session_id",
                "sessionId",
                "chatId",
                "chat_id",
                "conversationId",
                "conversation_id",
            ] {
                if let Some(id) = v.get(key).and_then(|x| x.as_str()) {
                    if !id.is_empty() {
                        return Some(id.to_string());
                    }
                }
            }
        }
    }
    None
}

fn extract_plan_json_object_from_stream(stdout: &str) -> Result<String, String> {
    // Prefer last JSONL object that looks like a ProjectPlan.
    let mut last_plan = None;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if looks_like_project_plan(&v) {
                last_plan = Some(trimmed.to_string());
            }
        }
    }
    if let Some(plan) = last_plan {
        return Ok(plan);
    }
    extract_final_json_object(stdout)
}

fn looks_like_project_plan(value: &serde_json::Value) -> bool {
    value.get("schemaVersion").is_some()
        && value.get("phases").and_then(|p| p.as_array()).is_some()
        && (value.get("runId").is_some() || value.get("run_id").is_some())
}

fn fail(
    run_id: Uuid,
    model_selection: crate::planner::types::ArchitectModelSelection,
    attempts: Vec<ArchitectAttemptRecord>,
    evidence: Vec<String>,
    error: String,
) -> ArchitectRunResult {
    let degraded_mode = model_selection.degraded;
    ArchitectRunResult {
        ok: false,
        run_id: run_id.to_string(),
        model_selection,
        plan: None,
        plan_json_path: None,
        master_plan_md_path: None,
        hashes: None,
        checkpoint: None,
        attempts,
        degraded_mode,
        error: Some(error),
        evidence,
    }
}

pub fn project_graph(plan: &ProjectPlan) -> GraphProjection {
    let nodes = plan
        .phases
        .iter()
        .map(|p| GraphNode {
            phase_id: p.phase_id.clone(),
            title: p.title.clone(),
            status: normalize_phase_status(&p.status),
            model_tier: normalize_model_tier(&p.model_tier),
            objective: p.objective.clone(),
        })
        .collect();
    let mut edges = Vec::new();
    for phase in &plan.phases {
        for dep in &phase.dependencies {
            edges.push(GraphEdge {
                from: dep.clone(),
                to: phase.phase_id.clone(),
            });
        }
    }
    GraphProjection {
        run_id: plan.run_id.to_string(),
        title: plan.title.clone(),
        nodes,
        edges,
    }
}

fn normalize_phase_status(status: &tiamat_contracts::PhaseStatus) -> String {
    match status {
        tiamat_contracts::PhaseStatus::Draft => "draft",
        tiamat_contracts::PhaseStatus::Ready => "ready",
        tiamat_contracts::PhaseStatus::Queued => "queued",
        tiamat_contracts::PhaseStatus::Running => "running",
        tiamat_contracts::PhaseStatus::Verifying => "verifying",
        tiamat_contracts::PhaseStatus::Passed => "passed",
        tiamat_contracts::PhaseStatus::Failed => "failed",
        tiamat_contracts::PhaseStatus::Blocked => "blocked",
        tiamat_contracts::PhaseStatus::Cancelled => "cancelled",
        tiamat_contracts::PhaseStatus::Skipped => "skipped",
        tiamat_contracts::PhaseStatus::NeedsReview => "needs_review",
    }
    .into()
}

#[allow(clippy::too_many_arguments)]
fn run_architect_hosted(
    host: Option<&HostedSpawnContext<'_>>,
    run_id: Uuid,
    argv: &[String],
    timeout_ms: u64,
    stdin: Option<&str>,
    env: &HashMap<String, String>,
    workspace_mount: &Path,
    resume_chat_hint: Option<&str>,
) -> Result<ProcessCapture, String> {
    let env_vec: Vec<(String, String)> = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let request = SpawnRequest {
        run_id,
        phase_id: Some("architect".into()),
        attempt_id: None,
        argv: argv.to_vec(),
        stdin: stdin.map(str::to_string),
        workspace: Some(workspace_mount.display().to_string()),
        env: env_vec,
        watchdog: watchdog_for_timeout(timeout_ms),
        resume_chat_hint: resume_chat_hint.map(str::to_string),
        next_model_on_timeout: None,
        next_tier_on_timeout: None,
    };
    match host {
        Some(ctx) => {
            let _ = ctx.store.ensure_run(run_id, "architect", "planning");
            run_capture_hosted(ctx.store, ctx.host, request).map_err(|e| e.to_string())
        }
        None => {
            let dir = std::env::temp_dir().join(format!("tiamat-architect-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let store = Store::open_in_memory(&dir).map_err(|e| e.to_string())?;
            let _ = store.ensure_run(run_id, "architect-ephemeral", "planning");
            let ph = ProcessHost::new();
            run_capture_hosted(&store, &ph, request).map_err(|e| e.to_string())
        }
    }
}

fn normalize_model_tier(tier: &tiamat_contracts::ModelTier) -> String {
    match tier {
        tiamat_contracts::ModelTier::Composer => "composer",
        tiamat_contracts::ModelTier::GrokLow => "grok-low",
        tiamat_contracts::ModelTier::GrokMedium => "grok-medium",
        tiamat_contracts::ModelTier::GrokHigh => "grok-high",
    }
    .into()
}
