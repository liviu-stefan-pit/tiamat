use sha2::{Digest, Sha256};
use tiamat_contracts::{ModelTier, PhasePlan, PhaseStatus, ProjectPlan, TestSpec};

/// Deterministically render `.tiamat/PLAN-SCHEDULE.md` from derived `plan.json`.
/// Architect-authored `.tiamat/MASTER-PLAN.md` is the canonical human plan.
pub fn render_plan_schedule_markdown(plan: &ProjectPlan) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", plan.title));
    out.push_str(&format!("Run ID: `{}`  \n", plan.run_id));
    out.push_str(&format!("Schema version: {}\n\n", plan.schema_version));
    out.push_str("## Summary\n\n");
    out.push_str(plan.summary.trim());
    out.push_str("\n\n");

    if !plan.assumptions.is_empty() {
        out.push_str("## Assumptions\n\n");
        for item in &plan.assumptions {
            out.push_str(&format!("- {}\n", item));
        }
        out.push('\n');
    }
    if !plan.risks.is_empty() {
        out.push_str("## Risks\n\n");
        for item in &plan.risks {
            out.push_str(&format!("- {}\n", item));
        }
        out.push('\n');
    }

    out.push_str("## Phases\n\n");
    for phase in &plan.phases {
        out.push_str(&render_phase(phase));
    }

    if !plan.final_gates.is_empty() {
        out.push_str("## Final gates\n\n");
        for gate in &plan.final_gates {
            out.push_str(&format!("### {}\n\n", gate.gate_id));
            out.push_str(&format!("{}\n\n", gate.description));
            out.push_str(&format!(
                "- Dependencies: {}\n",
                if gate.dependencies.is_empty() {
                    "(none)".into()
                } else {
                    gate.dependencies.join(", ")
                }
            ));
            out.push_str(&format!(
                "- Required evidence: {}\n\n",
                gate.required_evidence_kinds
                    .iter()
                    .map(|k| format!("{k:?}").to_ascii_lowercase())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    out.push_str("---\n\n");
    out.push_str(
        "_Schedule projection derived from `.tiamat/plan.json`. Canonical design lives in \
         `.tiamat/MASTER-PLAN.md`._\n",
    );
    out
}

/// Backward-compatible alias used by older call sites / tests.
pub fn render_master_plan_markdown(plan: &ProjectPlan) -> String {
    render_plan_schedule_markdown(plan)
}

fn render_phase(phase: &PhasePlan) -> String {
    let mut out = String::new();
    out.push_str(&format!("### {} — {}\n\n", phase.phase_id, phase.title));
    out.push_str(&format!("- Status: `{}`\n", status_str(&phase.status)));
    out.push_str(&format!("- Objective: {}\n", phase.objective));
    out.push_str(&format!(
        "- Dependencies: {}\n",
        if phase.dependencies.is_empty() {
            "(none)".into()
        } else {
            phase.dependencies.join(", ")
        }
    ));
    out.push_str(&format!(
        "- Project IDs: {}\n",
        phase.project_ids.join(", ")
    ));
    out.push_str(&format!("- Read roots: {}\n", phase.read_roots.join(", ")));
    out.push_str(&format!(
        "- Write roots: {}\n",
        phase.write_roots.join(", ")
    ));
    out.push_str(&format!(
        "- Model tier: `{}`\n",
        tier_str(&phase.model_tier)
    ));
    out.push_str(&format!(
        "- Estimated minutes: {}\n",
        phase.estimated_minutes
    ));
    out.push_str(&format!(
        "- Rollback: {} ({:?})\n\n",
        phase.rollback.checkpoint, phase.rollback.strategy
    ));

    out.push_str("#### Acceptance criteria\n\n");
    for ac in &phase.acceptance_criteria {
        out.push_str(&format!(
            "- `{}`: {} (evidence: {:?})\n",
            ac.criterion_id, ac.description, ac.required_evidence_kinds
        ));
    }
    out.push('\n');

    out.push_str("#### Tests\n\n");
    out.push_str(&render_tests("Unit", &phase.unit_tests));
    out.push_str(&render_tests("Integration", &phase.integration_tests));
    out.push_str(&render_tests("E2E", &phase.e2e_tests));

    if !phase.manual_checks.is_empty() {
        out.push_str("#### Manual checks\n\n");
        for check in &phase.manual_checks {
            out.push_str(&format!(
                "- {} (blocking={})\n",
                check.description, check.blocking
            ));
        }
        out.push('\n');
    }

    if !phase.expected_artifacts.is_empty() {
        out.push_str("#### Expected artifacts\n\n");
        for art in &phase.expected_artifacts {
            out.push_str(&format!("- `{art}`\n"));
        }
        out.push('\n');
    }

    out.push_str("#### Implementation prompt\n\n");
    out.push_str("```text\n");
    out.push_str(phase.prompt.trim());
    out.push_str("\n```\n\n");
    out
}

fn render_tests(label: &str, tests: &[TestSpec]) -> String {
    let mut out = String::new();
    out.push_str(&format!("##### {label}\n\n"));
    if tests.is_empty() {
        out.push_str("_None specified._\n\n");
        return out;
    }
    for test in tests {
        out.push_str(&format!(
            "- `{}`: `{}` in `{}` (timeout {}s; covers {})\n",
            test.test_id,
            test.command.join(" "),
            test.working_directory,
            test.timeout_seconds,
            test.covers.join(", ")
        ));
        if let Some(reason) = &test.inapplicable_reason {
            out.push_str(&format!("  - inapplicable: {reason}\n"));
        }
    }
    out.push('\n');
    out
}

fn tier_str(tier: &ModelTier) -> &'static str {
    match tier {
        ModelTier::Composer => "composer",
        ModelTier::GrokLow => "grok-low",
        ModelTier::GrokMedium => "grok-medium",
        ModelTier::GrokHigh => "grok-high",
    }
}

fn status_str(status: &PhaseStatus) -> &'static str {
    match status {
        PhaseStatus::Draft => "draft",
        PhaseStatus::Ready => "ready",
        PhaseStatus::Queued => "queued",
        PhaseStatus::Running => "running",
        PhaseStatus::Verifying => "verifying",
        PhaseStatus::Passed => "passed",
        PhaseStatus::Failed => "failed",
        PhaseStatus::Blocked => "blocked",
        PhaseStatus::Cancelled => "cancelled",
        PhaseStatus::Skipped => "skipped",
        PhaseStatus::NeedsReview => "needs_review",
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Re-render and hash-check PLAN-SCHEDULE.md against plan.json.
pub fn verify_schedule_projection(plan: &ProjectPlan, markdown: &str) -> Result<String, String> {
    let expected = render_plan_schedule_markdown(plan);
    let expected_hash = sha256_hex(expected.as_bytes());
    let actual_hash = sha256_hex(markdown.as_bytes());
    if expected != markdown || expected_hash != actual_hash {
        return Err(format!(
            "PLAN-SCHEDULE.md disagrees with plan.json (expected={expected_hash}, actual={actual_hash})"
        ));
    }
    Ok(expected_hash)
}

pub fn verify_markdown_projection(plan: &ProjectPlan, markdown: &str) -> Result<String, String> {
    verify_schedule_projection(plan, markdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiamat_contracts::{
        AcceptanceCriterion, FinalGate, RollbackSpec, RollbackStrategy, TestExpected, TestKind,
    };
    use uuid::Uuid;

    fn sample_plan() -> ProjectPlan {
        ProjectPlan {
            schema_version: 1,
            run_id: Uuid::parse_str("d4e5f6a7-b8c9-4012-d345-6789abcdef01").unwrap(),
            title: "Sample".into(),
            summary: "Summary line".into(),
            assumptions: vec!["A1".into()],
            risks: vec!["R1".into()],
            phases: vec![PhasePlan {
                phase_id: "P01".into(),
                title: "One".into(),
                objective: "Do one thing".into(),
                dependencies: vec![],
                project_ids: vec!["app".into()],
                read_roots: vec![".".into()],
                write_roots: vec![".".into()],
                model_tier: ModelTier::Composer,
                estimated_minutes: 5,
                acceptance_criteria: vec![AcceptanceCriterion {
                    criterion_id: "AC-P01-01".into(),
                    description: "works".into(),
                    required_evidence_kinds: vec![TestKind::Unit],
                }],
                unit_tests: vec![TestSpec {
                    test_id: "UT-1".into(),
                    command: vec!["npm".into(), "test".into()],
                    working_directory: ".".into(),
                    timeout_seconds: 60,
                    resource_locks: vec![],
                    expected: TestExpected {
                        exit_code: 0,
                        artifacts: vec![],
                    },
                    covers: vec!["AC-P01-01".into()],
                    inapplicable_reason: None,
                }],
                integration_tests: vec![],
                e2e_tests: vec![],
                manual_checks: vec![],
                rollback: RollbackSpec {
                    checkpoint: "base".into(),
                    strategy: RollbackStrategy::Restore,
                },
                expected_artifacts: vec![],
                prompt: "Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json".into(),
                status: PhaseStatus::Draft,
                evidence: vec![],
            }],
            final_gates: vec![FinalGate {
                gate_id: "FG-01".into(),
                description: "review".into(),
                dependencies: vec!["P01".into()],
                required_evidence_kinds: vec![TestKind::Review],
            }],
        }
    }

    #[test]
    fn render_is_deterministic_and_hash_stable() {
        let plan = sample_plan();
        let a = render_master_plan_markdown(&plan);
        let b = render_master_plan_markdown(&plan);
        assert_eq!(a, b);
        assert!(a.contains("### P01 — One"));
        assert!(a.contains(".tiamat/plan.json"));
        let hash = verify_markdown_projection(&plan, &a).unwrap();
        assert_eq!(hash, sha256_hex(a.as_bytes()));
    }

    #[test]
    fn hash_check_detects_tamper() {
        let plan = sample_plan();
        let mut md = render_master_plan_markdown(&plan);
        md.push_str("\ntampered\n");
        assert!(verify_markdown_projection(&plan, &md).is_err());
    }
}
