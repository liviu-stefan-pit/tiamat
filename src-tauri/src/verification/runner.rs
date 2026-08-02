use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Utc;
use tiamat_contracts::{EvidenceClassification, EvidenceRecord, PhasePlan, TestKind, TestSpec};
use uuid::Uuid;

use crate::db::Store;
use crate::process::{run_capture_hosted, watchdog_for_timeout, ProcessHost, SpawnRequest};
use crate::security::redact_line;
use crate::verification::classify::{classify_baseline, classify_flaky_retry};
use crate::verification::error::{VerificationError, VerificationResult};
use crate::verification::policy::{evaluate_command_policy_in_roots, CommandPolicyDecision};
use crate::verification::types::{classification_blocks_pass, LayerGateSummary, TestRunOutcome};
use crate::workspace::roots::validate_relative_within;

pub struct GateRunOptions<'a> {
    pub write_root: &'a Path,
    pub establish_baseline: bool,
    pub flaky_retry: bool,
    pub extra_env: HashMap<String, String>,
    /// When set, verification commands run through ProcessHost (Job + registry).
    pub host: Option<(&'a std::sync::Mutex<Store>, &'a ProcessHost, Uuid)>,
    pub phase_id: Option<&'a str>,
    pub attempt_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct GateRunReport {
    pub evidence: Vec<EvidenceRecord>,
    pub layers: Vec<LayerGateSummary>,
    pub all_required_passed: bool,
    pub blocking_failures: Vec<String>,
}

/// Run architect-specified unit → integration → e2e gates with evidence capture.
pub fn run_phase_gates(
    phase: &PhasePlan,
    opts: GateRunOptions<'_>,
) -> VerificationResult<GateRunReport> {
    let mut evidence = Vec::new();
    let mut layers = Vec::new();
    let mut blocking = Vec::new();

    let unit = run_layer(
        TestKind::Unit,
        &phase.unit_tests,
        &phase.acceptance_criteria,
        &opts,
        &mut evidence,
    )?;
    layers.push(unit);

    let integration = run_layer(
        TestKind::Integration,
        &phase.integration_tests,
        &phase.acceptance_criteria,
        &opts,
        &mut evidence,
    )?;
    layers.push(integration);

    let e2e = run_layer(
        TestKind::E2e,
        &phase.e2e_tests,
        &phase.acceptance_criteria,
        &opts,
        &mut evidence,
    )?;
    layers.push(e2e);

    for layer in &layers {
        if !layer.all_required_passed() {
            blocking.push(format!(
                "{} gate failed (executed={}, passed={}, failed={})",
                kind_label(&layer.kind),
                layer.executed,
                layer.passed,
                layer.failed
            ));
        }
    }

    // Criterion coverage: every criterion must have at least one non-blocking evidence item.
    for ac in &phase.acceptance_criteria {
        let covered = evidence.iter().any(|e| {
            e.covers.contains(&ac.criterion_id) && !classification_blocks_pass(&e.classification)
        });
        if !covered {
            // Still accept if inapplicable layers were the only required kinds and diff evidence exists —
            // otherwise block.
            let has_any = evidence.iter().any(|e| e.covers.contains(&ac.criterion_id));
            if !has_any {
                blocking.push(format!(
                    "acceptance criterion {} has no associated evidence",
                    ac.criterion_id
                ));
            } else if evidence.iter().any(|e| {
                e.covers.contains(&ac.criterion_id) && classification_blocks_pass(&e.classification)
            }) {
                blocking.push(format!(
                    "acceptance criterion {} only has failing evidence",
                    ac.criterion_id
                ));
            }
        }
    }

    let all_required_passed = blocking.is_empty() && layers.iter().all(|l| l.all_required_passed());
    Ok(GateRunReport {
        evidence,
        layers,
        all_required_passed,
        blocking_failures: blocking,
    })
}

fn run_layer(
    kind: TestKind,
    specs: &[TestSpec],
    _criteria: &[tiamat_contracts::AcceptanceCriterion],
    opts: &GateRunOptions<'_>,
    evidence_out: &mut Vec<EvidenceRecord>,
) -> VerificationResult<LayerGateSummary> {
    if specs.is_empty() {
        return Ok(LayerGateSummary {
            kind,
            required: false,
            executed: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            inapplicable: true,
            inapplicable_reason: Some("no tests specified for this layer".into()),
        });
    }

    // Empty arrays are inapplicable; nonempty with only inapplicableReason entries still skip.
    let executable: Vec<&TestSpec> = specs
        .iter()
        .filter(|s| s.inapplicable_reason.is_none())
        .collect();
    if executable.is_empty() {
        let reason = specs
            .iter()
            .find_map(|s| s.inapplicable_reason.clone())
            .unwrap_or_else(|| "marked inapplicable".into());
        return Ok(LayerGateSummary {
            kind,
            required: false,
            executed: 0,
            passed: 0,
            failed: 0,
            skipped: specs.len() as u32,
            inapplicable: true,
            inapplicable_reason: Some(reason),
        });
    }

    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = 0u32;
    let mut executed = 0u32;

    for spec in executable {
        match run_one_spec(kind.clone(), spec, opts)? {
            Some(outcome) => {
                executed += 1;
                // FlakyPass counts as gate-pass but keeps the flaky label.
                // BaselineFail is never silently ignored (§15.2).
                match outcome.evidence.classification {
                    EvidenceClassification::Pass | EvidenceClassification::FlakyPass => {
                        passed += 1;
                    }
                    _ => {
                        failed += 1;
                    }
                }
                evidence_out.push(outcome.evidence);
            }
            None => skipped += 1,
        }
    }

    Ok(LayerGateSummary {
        kind,
        required: true,
        executed,
        passed,
        failed,
        skipped,
        inapplicable: false,
        inapplicable_reason: None,
    })
}

fn run_one_spec(
    kind: TestKind,
    spec: &TestSpec,
    opts: &GateRunOptions<'_>,
) -> VerificationResult<Option<TestRunOutcome>> {
    let cwd = resolve_workdir(opts.write_root, &spec.working_directory)?;
    match evaluate_command_policy_in_roots(
        &spec.command,
        &cwd,
        Some(&[opts.write_root.display().to_string()]),
    ) {
        CommandPolicyDecision::Deny { reason } => {
            let now = Utc::now().to_rfc3339();
            return Ok(Some(TestRunOutcome {
                evidence: EvidenceRecord {
                    schema_version: 1,
                    evidence_id: format!("ev-{}", Uuid::new_v4()),
                    kind,
                    test_id: Some(spec.test_id.clone()),
                    command: spec.command.clone(),
                    working_directory: cwd.display().to_string(),
                    exit_code: -1,
                    duration_ms: 0,
                    summary: redact_line(&format!("policy denied: {reason}")),
                    artifact_hashes: vec![],
                    covers: spec.covers.clone(),
                    trustworthy: true,
                    partial: false,
                    classification: EvidenceClassification::PolicyDenied,
                    started_at_utc: now.clone(),
                    ended_at_utc: now,
                    baseline_exit_code: None,
                    flaky_retry: None,
                },
                passed_expected: false,
            }));
        }
        CommandPolicyDecision::Allow => {}
    }

    let baseline_exit = if opts.establish_baseline {
        run_command_exit(&spec.command, &cwd, spec.timeout_seconds, opts).ok()
    } else {
        None
    };

    let first = run_command_capture(&spec.command, &cwd, spec.timeout_seconds, opts)?;
    let mut classification =
        classify_baseline(baseline_exit, first.exit_code, spec.expected.exit_code);
    let mut flaky_retry = None;
    let mut final_exit = first.exit_code;
    let mut final_summary = first.summary.clone();
    let mut duration_ms = first.duration_ms;
    let started = first.started_at_utc.clone();
    let mut ended = first.ended_at_utc.clone();

    if opts.flaky_retry
        && matches!(classification, EvidenceClassification::Fail)
        && final_exit != spec.expected.exit_code
    {
        let retry = run_command_capture(&spec.command, &cwd, spec.timeout_seconds, opts)?;
        classification =
            classify_flaky_retry(classification, retry.exit_code, spec.expected.exit_code);
        flaky_retry = Some(true);
        final_exit = retry.exit_code;
        final_summary = format!("initial_fail; retry: {}", redact_line(&retry.summary));
        duration_ms += retry.duration_ms;
        ended = retry.ended_at_utc;
    }

    let passed_expected = matches!(
        classification,
        EvidenceClassification::Pass | EvidenceClassification::FlakyPass
    );

    Ok(Some(TestRunOutcome {
        evidence: EvidenceRecord {
            schema_version: 1,
            evidence_id: format!("ev-{}", Uuid::new_v4()),
            kind,
            test_id: Some(spec.test_id.clone()),
            command: spec.command.clone(),
            working_directory: cwd.display().to_string(),
            exit_code: final_exit,
            duration_ms,
            summary: redact_line(&final_summary),
            artifact_hashes: vec![],
            covers: spec.covers.clone(),
            trustworthy: true,
            partial: false,
            classification,
            started_at_utc: started,
            ended_at_utc: ended,
            baseline_exit_code: baseline_exit,
            flaky_retry,
        },
        passed_expected,
    }))
}

fn resolve_workdir(root: &Path, relative: &str) -> VerificationResult<PathBuf> {
    if relative == "." || relative.is_empty() {
        return Ok(root.to_path_buf());
    }
    validate_relative_within(root, relative)
        .map_err(|e| VerificationError::PathEscape(e.to_string()))
}

struct CmdCapture {
    exit_code: i32,
    summary: String,
    duration_ms: u64,
    started_at_utc: String,
    ended_at_utc: String,
}

fn run_command_exit(
    command: &[String],
    cwd: &Path,
    timeout_seconds: u32,
    opts: &GateRunOptions<'_>,
) -> VerificationResult<i32> {
    Ok(run_command_capture(command, cwd, timeout_seconds, opts)?.exit_code)
}

fn run_command_capture(
    command: &[String],
    cwd: &Path,
    timeout_seconds: u32,
    opts: &GateRunOptions<'_>,
) -> VerificationResult<CmdCapture> {
    let started_at = Utc::now();
    let started_instant = Instant::now();
    let timeout_ms = (timeout_seconds as u64).saturating_mul(1000).max(1_000);

    let mut env = opts.extra_env.clone();
    env.insert("TIAMAT_TEST_CWD".into(), cwd.display().to_string());
    let env_vec: Vec<(String, String)> = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    let run_id = opts.host.map(|(_, _, id)| id).unwrap_or_else(Uuid::new_v4);
    let request = SpawnRequest {
        run_id,
        phase_id: opts.phase_id.map(str::to_string),
        attempt_id: None,
        argv: command.to_vec(),
        stdin: Some(String::new()),
        workspace: Some(cwd.display().to_string()),
        env: env_vec,
        watchdog: watchdog_for_timeout(timeout_ms),
        resume_chat_hint: None,
        next_model_on_timeout: None,
        next_tier_on_timeout: None,
    };
    let _ = opts.attempt_id;

    let cap = match opts.host {
        Some((store, host, _)) => {
            let _ = store.lock().ok().and_then(|s| {
                s.create_run(run_id, "verification", "executing").ok()
            });
            run_capture_hosted(store, host, request)
                .map_err(|e| VerificationError::Spawn(e.to_string()))?
        }
        None => {
            // Ephemeral hosted path — still Job-associated (not bare Command::spawn).
            let dir = std::env::temp_dir().join(format!("tiamat-verify-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&dir).map_err(|e| VerificationError::Spawn(e.to_string()))?;
            let store = std::sync::Mutex::new(
                Store::open_in_memory(&dir).map_err(|e| VerificationError::Spawn(e.to_string()))?,
            );
            let _ = store.lock().ok().and_then(|s| {
                s.create_run(run_id, "verification-ephemeral", "executing").ok()
            });
            let host = ProcessHost::new();
            run_capture_hosted(&store, &host, request)
                .map_err(|e| VerificationError::Spawn(e.to_string()))?
        }
    };

    // Hosted path only — bare Command::spawn / run_argv_capture_env retired for verification.

    let exit_code = cap.exit_code.unwrap_or(-1);
    let summary = redact_line(&format!(
        "exit={exit_code}; stdout={}; stderr={}",
        truncate(&cap.stdout, 400),
        truncate(&cap.stderr, 400)
    ));

    Ok(CmdCapture {
        exit_code,
        summary,
        duration_ms: if cap.duration_ms > 0 {
            cap.duration_ms
        } else {
            started_instant.elapsed().as_millis() as u64
        },
        started_at_utc: started_at.to_rfc3339(),
        ended_at_utc: Utc::now().to_rfc3339(),
    })
}

fn truncate(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.len() <= max {
        t.to_string()
    } else {
        format!("{}…", &t[..max])
    }
}

fn kind_label(kind: &TestKind) -> &'static str {
    match kind {
        TestKind::Unit => "unit",
        TestKind::Integration => "integration",
        TestKind::E2e => "e2e",
        TestKind::Manual => "manual",
        TestKind::Diff => "diff",
        TestKind::Review => "review",
        TestKind::Artifact => "artifact",
        TestKind::Cleanup => "cleanup",
    }
}
