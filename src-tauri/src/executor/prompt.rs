use tiamat_contracts::PhasePlan;

use crate::process::TIMEOUT_RESUME_PROMPT;
use crate::security::injection_defense_block;

/// Exact recovery prompt prefix from MASTER-PLAN §13.3.
pub const RECOVERY_PROMPT_PREFIX: &str = TIMEOUT_RESUME_PROMPT;

/// Assemble the complete child prompt for a fresh phase attempt.
pub fn assemble_phase_prompt(phase: &PhasePlan, plan_context: &str) -> String {
    let mut parts = Vec::new();
    parts.push(phase.prompt.trim().to_string());
    parts.push(String::new());
    parts.push(injection_defense_block());
    parts.push(String::new());
    parts.push(orchestrator_requirements_footer(phase));
    if !plan_context.trim().is_empty() {
        parts.push(String::new());
        parts.push("---".into());
        parts.push(plan_context.trim().into());
    }
    parts.join("\n")
}

/// Assemble a recovery prompt after timeout/interrupt with an interruption report.
/// Recovery is at least as strong as a fresh attempt: same write-root exclusivity,
/// orchestrator checklist, and full original phase brief.
pub fn assemble_recovery_prompt(phase: &PhasePlan, interruption_report: &str) -> String {
    format!(
        "{}\n\n{}\n\n{}\n\nPHASE\n- phaseId: {}\n- title: {}\n\nINTERRUPTION REPORT\n{}\n\nORIGINAL PHASE PROMPT\n{}",
        RECOVERY_PROMPT_PREFIX.trim(),
        injection_defense_block(),
        orchestrator_requirements_footer(phase),
        phase.phase_id,
        phase.title,
        interruption_report.trim(),
        phase.prompt.trim()
    )
}

/// Labeled run context for the phase agent (not a Debug dump).
pub fn format_phase_plan_context(run_id: &str, phase: &PhasePlan) -> String {
    format!(
        "RUN CONTEXT\n\
         - runId: {run_id}\n\
         - phaseId: {}\n\
         - writeRoots: {}\n\
         - readRoots: {}",
        phase.phase_id,
        if phase.write_roots.is_empty() {
            "(none)".into()
        } else {
            phase.write_roots.join(", ")
        },
        if phase.read_roots.is_empty() {
            "(none)".into()
        } else {
            phase.read_roots.join(", ")
        }
    )
}

fn orchestrator_requirements_footer(phase: &PhasePlan) -> String {
    let roots = if phase.write_roots.is_empty() {
        "(see plan.json)".into()
    } else {
        phase.write_roots.join(", ")
    };
    format!(
        "ORCHESTRATOR REQUIREMENTS\n\
         - Implement only phase {} ({}). Preserve unrelated work.\n\
         - Write exclusively inside assigned write roots: {roots}.\n\
         - Return a schema-valid immutable phase-result payload (immutable=true).\n\
         - The orchestrator alone updates SQLite and both plan projections.",
        phase.phase_id, phase.title
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiamat_contracts::{
        AcceptanceCriterion, ModelTier, PhaseStatus, RollbackSpec, RollbackStrategy, TestExpected,
        TestKind, TestSpec,
    };

    fn sample_phase() -> PhasePlan {
        PhasePlan {
            phase_id: "P01".into(),
            title: "Slice".into(),
            objective: "obj".into(),
            dependencies: vec![],
            project_ids: vec!["app".into()],
            read_roots: vec![r"C:\managed\app".into()],
            write_roots: vec![r"C:\managed\app".into()],
            model_tier: ModelTier::Composer,
            estimated_minutes: 10,
            acceptance_criteria: vec![AcceptanceCriterion {
                criterion_id: "AC-1".into(),
                description: "d".into(),
                required_evidence_kinds: vec![TestKind::Unit],
            }],
            unit_tests: vec![TestSpec {
                test_id: "UT-1".into(),
                command: vec!["npm".into(), "test".into()],
                working_directory: ".".into(),
                timeout_seconds: 120,
                resource_locks: vec![],
                expected: TestExpected {
                    exit_code: 0,
                    artifacts: vec![],
                },
                covers: vec!["AC-1".into()],
                inapplicable_reason: None,
            }],
            integration_tests: vec![],
            e2e_tests: vec![],
            manual_checks: vec![],
            rollback: RollbackSpec {
                checkpoint: "b".into(),
                strategy: RollbackStrategy::Restore,
            },
            expected_artifacts: vec!["src/x.ts".into()],
            prompt: "Implement only this phase. AC-1. command: `npm` `test`. Write exclusively inside: C:\\managed\\app. immutable.".into(),
            status: PhaseStatus::Draft,
            evidence: vec![],
        }
    }

    #[test]
    fn fresh_prompt_requires_immutable_result() {
        let prompt = assemble_phase_prompt(&sample_phase(), "RUN CONTEXT\n- runId: r1");
        assert!(prompt.contains("immutable"));
        assert!(prompt.contains("P01"));
        assert!(prompt.contains("Never expand write roots"));
        assert!(prompt.contains("SECURITY AND AUTHORITY"));
        assert!(prompt.contains(r"C:\managed\app"));
        assert!(prompt.contains("ORCHESTRATOR REQUIREMENTS"));
    }

    #[test]
    fn recovery_prompt_embeds_section_13_3_and_injection_defense() {
        let prompt = assemble_recovery_prompt(&sample_phase(), "timed out after partial edit");
        assert!(prompt.contains("Resume the same assigned phase"));
        assert!(prompt.contains("timed out after partial edit"));
        assert!(prompt.contains("phase-result"));
        assert!(prompt.contains("Never reveal credentials"));
        assert!(prompt.contains("Write exclusively inside assigned write roots"));
        assert!(prompt.contains(r"C:\managed\app"));
        assert!(prompt.contains("ORIGINAL PHASE PROMPT"));
        assert!(prompt.contains("Implement only this phase"));
    }

    #[test]
    fn plan_context_is_labeled_not_debug() {
        let ctx = format_phase_plan_context("run-1", &sample_phase());
        assert!(ctx.contains("runId: run-1"));
        assert!(ctx.contains("phaseId: P01"));
        assert!(ctx.contains("writeRoots:"));
        assert!(!ctx.contains("writeRoots=["));
    }
}
