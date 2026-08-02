use crate::cursor::{filter_allowed_cursor_models, CursorModelInfo};
use crate::planner::types::{ArchitectModelSelection, ARCHITECT_PREFERRED_MODEL};

/// Architect always uses Cursor Grok High (never SOL or other families).
pub fn select_architect_model(
    models: &[CursorModelInfo],
) -> Result<ArchitectModelSelection, String> {
    let allowed = filter_allowed_cursor_models(models);
    let available: Vec<String> = allowed.iter().map(|m| m.id.clone()).collect();
    let has = |id: &str| available.iter().any(|m| m == id);

    if has(ARCHITECT_PREFERRED_MODEL) {
        return Ok(ArchitectModelSelection {
            requested_model: ARCHITECT_PREFERRED_MODEL.into(),
            selected_model: ARCHITECT_PREFERRED_MODEL.into(),
            degraded: false,
            reason: "preferred Cursor Grok High architect model available".into(),
            available_models: available,
        });
    }

    // Accept any model id that looks like Grok High when the preferred string differs slightly.
    if let Some(grok_high) = available.iter().find(|m| {
        let lower = m.to_ascii_lowercase();
        lower.contains("grok") && lower.contains("high")
    }) {
        return Ok(ArchitectModelSelection {
            requested_model: ARCHITECT_PREFERRED_MODEL.into(),
            selected_model: grok_high.clone(),
            degraded: true,
            reason: format!(
                "{ARCHITECT_PREFERRED_MODEL} unavailable; using available Grok High '{grok_high}'"
            ),
            available_models: available,
        });
    }

    Err(format!(
        "no allowed architect model: need {ARCHITECT_PREFERRED_MODEL} (Cursor Grok High); available={available:?}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::types::ARCHITECT_FALLBACK_MODEL;

    fn model(id: &str) -> CursorModelInfo {
        CursorModelInfo {
            id: id.into(),
            label: id.into(),
        }
    }

    #[test]
    fn prefers_grok_high_and_ignores_sol() {
        let sel = select_architect_model(&[
            model("composer-2.5"),
            model("gpt-5.6-sol-high"),
            model(ARCHITECT_PREFERRED_MODEL),
        ])
        .unwrap();
        assert_eq!(sel.selected_model, ARCHITECT_PREFERRED_MODEL);
        assert!(!sel.degraded);
        assert!(!sel.available_models.iter().any(|id| id.contains("sol")));
    }

    #[test]
    fn fails_when_no_grok_high() {
        let err = select_architect_model(&[model("composer-2.5"), model("cursor-grok-4.5-medium")])
            .unwrap_err();
        assert!(err.contains("no allowed architect model"));
    }

    #[test]
    fn accepts_fuzzy_grok_high_label() {
        let catalog = crate::cursor::parse_models_output(
            "\
cursor-grok-4.5-high - Cursor Grok 4.5
gpt-5.6-sol-high - GPT 5.6 Sol High
composer-2.5 - Composer 2.5
",
        );
        let sel = select_architect_model(&catalog).unwrap();
        assert_eq!(sel.selected_model, ARCHITECT_PREFERRED_MODEL);
        assert!(!sel.degraded);
        assert!(sel.available_models.iter().all(|id| !id.contains(" - ")));
        assert_eq!(ARCHITECT_FALLBACK_MODEL, ARCHITECT_PREFERRED_MODEL);
    }

    #[test]
    fn degraded_when_only_fuzzy_grok_high_available() {
        let sel =
            select_architect_model(&[model("composer-2.5"), model("cursor-grok-high")]).unwrap();
        assert_eq!(sel.selected_model, "cursor-grok-high");
        assert!(sel.degraded);
        assert!(sel.reason.contains("unavailable"));
    }
}
