use crate::cursor::{filter_allowed_cursor_models, is_allowed_cursor_model, CursorModelInfo};
use crate::scheduler::error::{SchedulerError, SchedulerResult};
use crate::scheduler::types::{
    escalate_tier, model_tier_str, preferred_model_for_tier, tier_rank, FailureKind, ModelSelection,
};
use tiamat_contracts::ModelTier;

/// Resolve an implementation/review model from the requested tier and runtime availability.
/// Only Composer and Grok (all efforts) are allowed — never SOL or other families.
/// Tiny/mechanical phases request Composer; everything else uses Grok tiers with escalation.
pub fn route_model(
    requested_tier: &ModelTier,
    available: &[CursorModelInfo],
    prior_attempt_count: u32,
    prior_failure: Option<FailureKind>,
    is_final_review: bool,
    allow_downgrade: bool,
    same_tier_resume: bool,
) -> SchedulerResult<ModelSelection> {
    let allowed = filter_allowed_cursor_models(available);
    let available_ids: Vec<String> = allowed.iter().map(|m| m.id.clone()).collect();
    if available_ids.is_empty() {
        return Err(SchedulerError::Routing(
            "no Composer/Grok models available; SOL and other families are not used".into(),
        ));
    }

    let mut effective_tier = if is_final_review {
        ModelTier::GrokHigh
    } else {
        requested_tier.clone()
    };

    let mut escalated = false;
    if let Some(kind) = prior_failure {
        if kind.should_escalate() && !same_tier_resume {
            if let Some(next) = escalate_tier(&effective_tier) {
                effective_tier = next;
                escalated = true;
            }
        }
    }

    // Downgrade only before first attempt when explicitly allowed and no prior failure.
    if allow_downgrade && prior_attempt_count == 0 && prior_failure.is_none() && !is_final_review {
        // Keep requested tier; downgrade is an explicit caller choice via a lower requested_tier.
    }

    let preferred = preferred_model_for_tier(&effective_tier);
    if let Some(selected) = pick_available(preferred, &available_ids, &effective_tier) {
        let substituted = selected != preferred;
        let reason = if same_tier_resume {
            format!(
                "same-tier resume at {} using {selected}",
                model_tier_str(&effective_tier)
            )
        } else if escalated {
            format!(
                "escalated to {} after {:?}; selected {selected}",
                model_tier_str(&effective_tier),
                prior_failure
            )
        } else if substituted {
            format!(
                "preferred {preferred} unavailable; deterministic fallback {selected} for {}",
                model_tier_str(&effective_tier)
            )
        } else {
            format!(
                "selected preferred {selected} for {}",
                model_tier_str(&effective_tier)
            )
        };

        if !is_allowed_cursor_model(&selected) {
            return Err(SchedulerError::Routing(format!(
                "refusing to route implementation/review to non-Cursor model {selected}"
            )));
        }

        return Ok(ModelSelection {
            requested_tier: requested_tier.clone(),
            requested_model: preferred_model_for_tier(requested_tier).to_string(),
            selected_model: selected,
            selection_reason: reason,
            substituted,
            escalated,
            available_models: available_ids,
        });
    }

    // Walk up the ladder within Composer/Grok family only.
    let mut probe = effective_tier.clone();
    while let Some(next) = escalate_tier(&probe) {
        probe = next;
        escalated = true;
        let preferred_next = preferred_model_for_tier(&probe);
        if let Some(selected) = pick_available(preferred_next, &available_ids, &probe) {
            if !is_allowed_cursor_model(&selected) {
                continue;
            }
            return Ok(ModelSelection {
                requested_tier: requested_tier.clone(),
                requested_model: preferred_model_for_tier(requested_tier).to_string(),
                selected_model: selected.clone(),
                selection_reason: format!(
                    "no model for {}; escalated availability fallback to {selected}",
                    model_tier_str(&effective_tier)
                ),
                substituted: true,
                escalated,
                available_models: available_ids,
            });
        }
    }

    Err(SchedulerError::Routing(format!(
        "no allowed Composer/Grok model for tier {}; available={available_ids:?}",
        model_tier_str(requested_tier)
    )))
}

fn pick_available(preferred: &str, available: &[String], tier: &ModelTier) -> Option<String> {
    if available.iter().any(|m| m == preferred) {
        return Some(preferred.to_string());
    }

    // Deterministic family/tier fallbacks — Composer/Grok only.
    let candidates: &[&str] = match tier {
        ModelTier::Composer => &["composer-2.5", "composer-2", "composer"],
        ModelTier::GrokLow => &["cursor-grok-4.5-low", "grok-low"],
        ModelTier::GrokMedium => &["cursor-grok-4.5-medium", "grok-medium"],
        ModelTier::GrokHigh => &["cursor-grok-4.5-high", "grok-high"],
    };

    for cand in candidates {
        if available.iter().any(|m| m == *cand) {
            return Some((*cand).to_string());
        }
    }

    // Fuzzy match within the same family/tier keywords.
    available
        .iter()
        .find(|m| {
            let lower = m.to_ascii_lowercase();
            if !is_allowed_cursor_model(m) {
                return false;
            }
            match tier {
                ModelTier::Composer => lower.contains("composer"),
                ModelTier::GrokLow => lower.contains("grok") && lower.contains("low"),
                ModelTier::GrokMedium => lower.contains("grok") && lower.contains("medium"),
                ModelTier::GrokHigh => lower.contains("grok") && lower.contains("high"),
            }
        })
        .cloned()
}

/// Decide whether another attempt is allowed and which tier/resume mode to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryDecision {
    pub allow: bool,
    pub next_tier: ModelTier,
    pub same_tier_resume: bool,
    pub reason: String,
}

pub fn decide_retry(
    original_tier: &ModelTier,
    last_selected_tier: &ModelTier,
    attempt_count: u32,
    max_attempts: u32,
    failure: FailureKind,
    progress_useful: bool,
    already_same_tier_resumed: bool,
) -> RetryDecision {
    if attempt_count >= max_attempts {
        return RetryDecision {
            allow: false,
            next_tier: last_selected_tier.clone(),
            same_tier_resume: false,
            reason: format!("attempt budget exhausted ({attempt_count}/{max_attempts})"),
        };
    }

    if failure.is_deterministic() {
        return RetryDecision {
            allow: false,
            next_tier: last_selected_tier.clone(),
            same_tier_resume: false,
            reason: format!(
                "deterministic {} failure; not consuming blind model escalation",
                failure.as_str()
            ),
        };
    }

    if tier_rank(last_selected_tier) >= tier_rank(&ModelTier::GrokHigh) {
        if progress_useful && !already_same_tier_resumed {
            return RetryDecision {
                allow: true,
                next_tier: ModelTier::GrokHigh,
                same_tier_resume: true,
                reason: "Grok High same-tier resume permitted due to useful progress".into(),
            };
        }
        return RetryDecision {
            allow: false,
            next_tier: ModelTier::GrokHigh,
            same_tier_resume: false,
            reason: "already at Grok High without further same-tier resume".into(),
        };
    }

    let next = escalate_tier(last_selected_tier).unwrap_or(last_selected_tier.clone());
    RetryDecision {
        allow: true,
        next_tier: next.clone(),
        same_tier_resume: false,
        reason: format!(
            "escalate {} → {} after {}",
            model_tier_str(&original_tier.max_with(last_selected_tier)),
            model_tier_str(&next),
            failure.as_str()
        ),
    }
}

trait TierOrd {
    fn max_with(&self, other: &ModelTier) -> ModelTier;
}

impl TierOrd for ModelTier {
    fn max_with(&self, other: &ModelTier) -> ModelTier {
        if tier_rank(self) >= tier_rank(other) {
            self.clone()
        } else {
            other.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::types::MODEL_SOL;

    fn models(ids: &[&str]) -> Vec<CursorModelInfo> {
        ids.iter()
            .map(|id| CursorModelInfo {
                id: (*id).into(),
                label: (*id).into(),
            })
            .collect()
    }

    #[test]
    fn never_selects_sol_for_implementation() {
        let err = route_model(
            &ModelTier::Composer,
            &models(&[MODEL_SOL]),
            0,
            None,
            false,
            false,
            false,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Composer/Grok") || err.to_string().contains("SOL"),
            "{err}"
        );
    }

    #[test]
    fn selects_composer_when_available() {
        let sel = route_model(
            &ModelTier::Composer,
            &models(&[MODEL_SOL, "composer-2.5", "cursor-grok-4.5-high"]),
            0,
            None,
            false,
            false,
            false,
        )
        .unwrap();
        assert_eq!(sel.selected_model, "composer-2.5");
        assert!(!sel.escalated);
    }

    #[test]
    fn escalates_after_timeout() {
        let sel = route_model(
            &ModelTier::Composer,
            &models(&[
                "composer-2.5",
                "cursor-grok-4.5-low",
                "cursor-grok-4.5-medium",
                "cursor-grok-4.5-high",
            ]),
            1,
            Some(FailureKind::Timeout),
            false,
            false,
            false,
        )
        .unwrap();
        assert_eq!(sel.selected_model, "cursor-grok-4.5-low");
        assert!(sel.escalated);
    }

    #[test]
    fn final_review_stays_at_grok_high() {
        let sel = route_model(
            &ModelTier::Composer,
            &models(&["composer-2.5", "cursor-grok-4.5-high"]),
            0,
            None,
            true,
            false,
            false,
        )
        .unwrap();
        assert_eq!(sel.selected_model, "cursor-grok-4.5-high");
    }

    #[test]
    fn deterministic_failure_blocks_escalation() {
        let decision = decide_retry(
            &ModelTier::Composer,
            &ModelTier::Composer,
            1,
            4,
            FailureKind::Policy,
            false,
            false,
        );
        assert!(!decision.allow);
        assert!(decision.reason.contains("deterministic"));
    }

    #[test]
    fn grok_high_allows_one_same_tier_resume() {
        let decision = decide_retry(
            &ModelTier::GrokHigh,
            &ModelTier::GrokHigh,
            2,
            4,
            FailureKind::Timeout,
            true,
            false,
        );
        assert!(decision.allow);
        assert!(decision.same_tier_resume);

        let decision2 = decide_retry(
            &ModelTier::GrokHigh,
            &ModelTier::GrokHigh,
            3,
            4,
            FailureKind::Timeout,
            true,
            true,
        );
        assert!(!decision2.allow);
    }

    #[test]
    fn records_substitution_when_preferred_missing() {
        let sel = route_model(
            &ModelTier::GrokLow,
            &models(&["cursor-grok-4.5-medium", "cursor-grok-4.5-high"]),
            0,
            None,
            false,
            false,
            false,
        )
        .unwrap();
        assert!(sel.substituted || sel.escalated);
        assert!(!sel.selected_model.to_ascii_lowercase().contains("sol"));
    }
}
