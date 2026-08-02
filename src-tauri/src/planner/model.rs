use crate::cursor::CursorModelInfo;
use crate::planner::types::{
    ArchitectModelSelection, ARCHITECT_FALLBACK_MODEL, ARCHITECT_PREFERRED_MODEL,
};

/// Select SOL for architecture when available; otherwise Grok High degraded mode.
pub fn select_architect_model(
    models: &[CursorModelInfo],
) -> Result<ArchitectModelSelection, String> {
    let available: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
    let has = |id: &str| available.iter().any(|m| m == id);

    if has(ARCHITECT_PREFERRED_MODEL) {
        return Ok(ArchitectModelSelection {
            requested_model: ARCHITECT_PREFERRED_MODEL.into(),
            selected_model: ARCHITECT_PREFERRED_MODEL.into(),
            degraded: false,
            reason: "preferred SOL architect model available".into(),
            available_models: available,
        });
    }

    if has(ARCHITECT_FALLBACK_MODEL) {
        return Ok(ArchitectModelSelection {
            requested_model: ARCHITECT_PREFERRED_MODEL.into(),
            selected_model: ARCHITECT_FALLBACK_MODEL.into(),
            degraded: true,
            reason: format!(
                "{ARCHITECT_PREFERRED_MODEL} unavailable; using {ARCHITECT_FALLBACK_MODEL} degraded mode"
            ),
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
        "no allowed architect model: need {ARCHITECT_PREFERRED_MODEL} or {ARCHITECT_FALLBACK_MODEL}; available={available:?}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> CursorModelInfo {
        CursorModelInfo {
            id: id.into(),
            label: id.into(),
        }
    }

    #[test]
    fn prefers_sol_when_available() {
        let sel = select_architect_model(&[
            model("composer-2.5"),
            model(ARCHITECT_PREFERRED_MODEL),
            model(ARCHITECT_FALLBACK_MODEL),
        ])
        .unwrap();
        assert_eq!(sel.selected_model, ARCHITECT_PREFERRED_MODEL);
        assert!(!sel.degraded);
    }

    #[test]
    fn falls_back_to_grok_high_and_records_degraded() {
        let sel = select_architect_model(&[model("composer-2.5"), model(ARCHITECT_FALLBACK_MODEL)])
            .unwrap();
        assert_eq!(sel.selected_model, ARCHITECT_FALLBACK_MODEL);
        assert!(sel.degraded);
        assert_eq!(sel.requested_model, ARCHITECT_PREFERRED_MODEL);
    }

    #[test]
    fn fails_when_no_allowed_high_tier() {
        let err = select_architect_model(&[model("composer-2.5"), model("cursor-grok-4.5-medium")])
            .unwrap_err();
        assert!(err.contains("no allowed architect model"));
    }

    #[test]
    fn prefers_sol_from_parsed_display_line_catalog() {
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
        assert!(sel
            .available_models
            .iter()
            .all(|id| !id.contains(" - ")));
    }
}
