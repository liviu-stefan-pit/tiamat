use tiamat_contracts::{PhaseResult, PhaseResultStatus, RollbackStrategy};

use crate::process::ResumeMetadata;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartialRecoveryDecision {
    /// Resume same chat with recovery prompt; keep workspace edits.
    Resume { progress_useful: bool },
    /// Roll back to prior checkpoint.
    Rollback,
    /// Quarantine the attempt tree and retry from clean checkpoint.
    Quarantine { reason: String },
    /// Fail without resume (corrupt / policy).
    Fail { reason: String },
}

/// Decide how to handle timed-out or interrupted partial work.
pub fn decide_partial_recovery(
    progress_useful: bool,
    boundary_ok: bool,
    rollback_strategy: RollbackStrategy,
    resume_meta: Option<&ResumeMetadata>,
    phase_result: Option<&PhaseResult>,
) -> PartialRecoveryDecision {
    if !boundary_ok {
        return PartialRecoveryDecision::Quarantine {
            reason: "out-of-bound edits during partial attempt".into(),
        };
    }

    if let Some(result) = phase_result {
        if !result.immutable {
            return PartialRecoveryDecision::Fail {
                reason: "corrupt phase result (immutable=false)".into(),
            };
        }
        if matches!(result.status, PhaseResultStatus::Failed) && !progress_useful {
            return match rollback_strategy {
                RollbackStrategy::Quarantine => PartialRecoveryDecision::Quarantine {
                    reason: "failed result without useful progress".into(),
                },
                RollbackStrategy::Restore => PartialRecoveryDecision::Rollback,
            };
        }
    }

    if progress_useful {
        return PartialRecoveryDecision::Resume {
            progress_useful: true,
        };
    }

    if resume_meta.map(|m| m.progress_useful).unwrap_or(false) {
        return PartialRecoveryDecision::Resume {
            progress_useful: true,
        };
    }

    match rollback_strategy {
        RollbackStrategy::Quarantine => PartialRecoveryDecision::Quarantine {
            reason: "timeout without useful progress".into(),
        },
        RollbackStrategy::Restore => PartialRecoveryDecision::Rollback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_forces_quarantine() {
        let d = decide_partial_recovery(true, false, RollbackStrategy::Restore, None, None);
        assert!(matches!(d, PartialRecoveryDecision::Quarantine { .. }));
    }

    #[test]
    fn useful_progress_resumes() {
        let d = decide_partial_recovery(true, true, RollbackStrategy::Restore, None, None);
        assert_eq!(
            d,
            PartialRecoveryDecision::Resume {
                progress_useful: true
            }
        );
    }

    #[test]
    fn no_progress_rolls_back() {
        let d = decide_partial_recovery(false, true, RollbackStrategy::Restore, None, None);
        assert_eq!(d, PartialRecoveryDecision::Rollback);
    }
}
