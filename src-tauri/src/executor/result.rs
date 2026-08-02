use tiamat_contracts::{PhaseResult, PhaseResultStatus};

use crate::executor::error::{ExecutorError, ExecutorResult};
use crate::planner::extract_final_json_object;

/// Extract and validate an immutable phase-result payload from agent stdout.
pub fn extract_phase_result(stdout: &str) -> ExecutorResult<PhaseResult> {
    // Prefer fenced / assistant text objects that look like phase results.
    if let Ok(json_text) = extract_final_json_object(stdout) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_text) {
            if value.get("immutable").is_some() && value.get("phaseId").is_some() {
                return validate_phase_result_payload(&value);
            }
        }
    }
    // Scan JSONL lines for an immutable phase-result object.
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !(trimmed.starts_with('{') && trimmed.contains("immutable")) {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if value.get("immutable").is_some() && value.get("phaseId").is_some() {
                return validate_phase_result_payload(&value);
            }
        }
    }
    for chunk in stdout.split("```") {
        let trimmed = chunk.trim();
        let candidate = trimmed.strip_prefix("json").unwrap_or(trimmed).trim();
        if candidate.contains("immutable") && candidate.contains("phaseId") {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(candidate) {
                return validate_phase_result_payload(&value);
            }
        }
    }
    Err(ExecutorError::InvalidPhaseResult(
        "no immutable phase-result JSON found in agent output".into(),
    ))
}

pub fn validate_phase_result_payload(value: &serde_json::Value) -> ExecutorResult<PhaseResult> {
    let result: PhaseResult = serde_json::from_value(value.clone())
        .map_err(|e| ExecutorError::InvalidPhaseResult(format!("deserialize phase result: {e}")))?;
    result
        .validate_immutable()
        .map_err(ExecutorError::InvalidPhaseResult)?;
    if !matches!(
        result.status,
        PhaseResultStatus::Passed | PhaseResultStatus::Failed | PhaseResultStatus::NeedsReview
    ) {
        return Err(ExecutorError::InvalidPhaseResult(
            "unsupported phase result status".into(),
        ));
    }
    // CODE-001: fail closed — embedded schema from tiamat-contracts (no disk / soft-skip).
    let compiled =
        tiamat_contracts::compile_schema_named("phase-result.schema.json").map_err(|e| {
            ExecutorError::InvalidPhaseResult(format!("phase-result schema unavailable: {e}"))
        })?;
    tiamat_contracts::validate_json(&compiled, value)
        .map_err(|e| ExecutorError::InvalidPhaseResult(format!("schema validation: {e}")))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_mutable_result() {
        let value = json!({
            "schemaVersion": 1,
            "phaseId": "P01",
            "status": "passed",
            "summary": "done",
            "changedFiles": [],
            "evidenceIds": [],
            "acceptanceSatisfied": [],
            "artifacts": [],
            "immutable": false
        });
        assert!(validate_phase_result_payload(&value).is_err());
    }

    #[test]
    fn accepts_valid_immutable_result() {
        let value = json!({
            "schemaVersion": 1,
            "phaseId": "P01",
            "status": "passed",
            "summary": "done",
            "changedFiles": ["src/a.ts"],
            "evidenceIds": ["ev-1"],
            "acceptanceSatisfied": ["AC-1"],
            "artifacts": [],
            "immutable": true
        });
        let result = validate_phase_result_payload(&value).unwrap();
        assert!(result.immutable);
        assert_eq!(result.phase_id, "P01");
    }

    #[test]
    fn extracts_from_stream_text() {
        let stdout = r#"
{"type":"assistant","message":{"content":[{"type":"text","text":"working"}]}}
```json
{"schemaVersion":1,"phaseId":"P01","status":"passed","summary":"ok","changedFiles":["src/x.ts"],"evidenceIds":[],"acceptanceSatisfied":["AC-1"],"artifacts":[],"immutable":true}
```
{"type":"result","subtype":"success"}
"#;
        let result = extract_phase_result(stdout).unwrap();
        assert_eq!(result.phase_id, "P01");
    }

    #[test]
    fn schema_validation_fails_closed_rejects_invalid_status_and_extras() {
        let extra = json!({
            "schemaVersion": 1,
            "phaseId": "P01",
            "status": "passed",
            "summary": "done",
            "changedFiles": [],
            "evidenceIds": [],
            "acceptanceSatisfied": [],
            "artifacts": [],
            "immutable": true,
            "unexpectedForbidden": true
        });
        assert!(
            validate_phase_result_payload(&extra).is_err(),
            "additionalProperties must fail closed via embedded schema"
        );
        let bad_status = json!({
            "schemaVersion": 1,
            "phaseId": "P01",
            "status": "not-a-real-status",
            "summary": "done",
            "changedFiles": [],
            "evidenceIds": [],
            "acceptanceSatisfied": [],
            "artifacts": [],
            "immutable": true
        });
        assert!(validate_phase_result_payload(&bad_status).is_err());
    }
}
