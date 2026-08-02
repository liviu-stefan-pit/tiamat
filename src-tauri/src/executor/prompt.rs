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
    parts.push("ORCHESTRATOR REQUIREMENTS".into());
    parts.push("- Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json before changing files.".into());
    parts.push("- Inspect current git status and prior evidence.".into());
    parts.push(format!(
        "- Implement only phase {} ({}). Preserve unrelated work.",
        phase.phase_id, phase.title
    ));
    parts.push(format!(
        "- Write exclusively inside assigned write roots: {}.",
        phase.write_roots.join(", ")
    ));
    parts.push("- Add and run the phase unit, integration, and E2E tests as applicable.".into());
    parts.push("- Do not declare success without command output and artifacts.".into());
    parts.push("- Return a schema-valid immutable phase-result payload (immutable=true).".into());
    parts.push("- The orchestrator alone updates SQLite and both plan projections.".into());
    if !plan_context.trim().is_empty() {
        parts.push(String::new());
        parts.push("---".into());
        parts.push(plan_context.trim().into());
    }
    parts.join("\n")
}

/// Assemble a recovery prompt after timeout/interrupt with an interruption report.
pub fn assemble_recovery_prompt(phase: &PhasePlan, interruption_report: &str) -> String {
    format!(
        "{}\n\n{}\n\nPHASE\n- phaseId: {}\n- title: {}\n\nINTERRUPTION REPORT\n{}\n\nORIGINAL PHASE PROMPT\n{}",
        RECOVERY_PROMPT_PREFIX.trim(),
        injection_defense_block(),
        phase.phase_id,
        phase.title,
        interruption_report.trim(),
        phase.prompt.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiamat_contracts::{
        AcceptanceCriterion, ModelTier, PhaseStatus, RollbackSpec, RollbackStrategy, TestKind,
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
            unit_tests: vec![],
            integration_tests: vec![],
            e2e_tests: vec![],
            manual_checks: vec![],
            rollback: RollbackSpec {
                checkpoint: "b".into(),
                strategy: RollbackStrategy::Restore,
            },
            expected_artifacts: vec![],
            prompt: "Implement P01 only.".into(),
            status: PhaseStatus::Draft,
            evidence: vec![],
        }
    }

    #[test]
    fn fresh_prompt_requires_immutable_result() {
        let prompt = assemble_phase_prompt(&sample_phase(), "context");
        assert!(prompt.contains("immutable"));
        assert!(prompt.contains("P01"));
        assert!(prompt.contains(".tiamat/plan.json"));
        assert!(prompt.contains("Never expand write roots"));
        assert!(prompt.contains("SECURITY AND AUTHORITY"));
    }

    #[test]
    fn recovery_prompt_embeds_section_13_3_and_injection_defense() {
        let prompt = assemble_recovery_prompt(&sample_phase(), "timed out after partial edit");
        assert!(prompt.contains("Resume the same assigned phase"));
        assert!(prompt.contains("timed out after partial edit"));
        assert!(prompt.contains("phase-result"));
        assert!(prompt.contains("Never reveal credentials"));
    }
}
