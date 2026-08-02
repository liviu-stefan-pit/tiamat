use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPlan {
    pub schema_version: u32,
    pub run_id: Uuid,
    pub title: String,
    pub summary: String,
    pub assumptions: Vec<String>,
    pub risks: Vec<String>,
    pub phases: Vec<PhasePlan>,
    pub final_gates: Vec<FinalGate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PhasePlan {
    pub phase_id: String,
    pub title: String,
    pub objective: String,
    pub dependencies: Vec<String>,
    pub project_ids: Vec<String>,
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub model_tier: ModelTier,
    pub estimated_minutes: u32,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub unit_tests: Vec<TestSpec>,
    pub integration_tests: Vec<TestSpec>,
    pub e2e_tests: Vec<TestSpec>,
    pub manual_checks: Vec<ManualCheck>,
    pub rollback: RollbackSpec,
    pub expected_artifacts: Vec<String>,
    pub prompt: String,
    pub status: PhaseStatus,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCriterion {
    pub criterion_id: String,
    pub description: String,
    pub required_evidence_kinds: Vec<TestKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestSpec {
    pub test_id: String,
    pub command: Vec<String>,
    pub working_directory: String,
    pub timeout_seconds: u32,
    pub resource_locks: Vec<String>,
    pub expected: TestExpected,
    pub covers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inapplicable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestExpected {
    pub exit_code: i32,
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManualCheck {
    pub description: String,
    pub blocking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RollbackSpec {
    pub checkpoint: String,
    pub strategy: RollbackStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FinalGate {
    pub gate_id: String,
    pub description: String,
    pub dependencies: Vec<String>,
    pub required_evidence_kinds: Vec<TestKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ModelTier {
    Composer,
    #[serde(rename = "grok-low")]
    GrokLow,
    #[serde(rename = "grok-medium")]
    GrokMedium,
    #[serde(rename = "grok-high")]
    GrokHigh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Draft,
    Ready,
    Queued,
    Running,
    Verifying,
    Passed,
    Failed,
    Blocked,
    Cancelled,
    Skipped,
    NeedsReview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TestKind {
    Unit,
    Integration,
    E2e,
    Manual,
    Diff,
    Review,
    Artifact,
    Cleanup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RollbackStrategy {
    Restore,
    Quarantine,
}

impl ProjectPlan {
    pub fn validate_schema_version(&self) -> Result<(), crate::validation::ValidationError> {
        if self.schema_version != crate::CURRENT_SCHEMA_VERSION {
            return Err(
                crate::validation::ValidationError::IncompatibleSchemaVersion {
                    expected: crate::CURRENT_SCHEMA_VERSION,
                    found: self.schema_version,
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::{compile_schema_named, validate_json};
    use serde_json::{json, Value};

    const SCHEMA_STATUSES: &[&str] = &[
        "draft",
        "ready",
        "queued",
        "running",
        "verifying",
        "passed",
        "failed",
        "blocked",
        "cancelled",
        "skipped",
        "needs_review",
    ];

    fn all_rust_statuses() -> Vec<PhaseStatus> {
        vec![
            PhaseStatus::Draft,
            PhaseStatus::Ready,
            PhaseStatus::Queued,
            PhaseStatus::Running,
            PhaseStatus::Verifying,
            PhaseStatus::Passed,
            PhaseStatus::Failed,
            PhaseStatus::Blocked,
            PhaseStatus::Cancelled,
            PhaseStatus::Skipped,
            PhaseStatus::NeedsReview,
        ]
    }

    #[test]
    fn phase_status_serde_matches_schema_enum() {
        let serialized: Vec<String> = all_rust_statuses()
            .iter()
            .map(|s| serde_json::to_value(s).unwrap())
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            serialized,
            SCHEMA_STATUSES
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
        );

        for (wire, expected) in SCHEMA_STATUSES.iter().zip(all_rust_statuses()) {
            let got: PhaseStatus = serde_json::from_value(Value::String((*wire).into())).unwrap();
            assert_eq!(got, expected, "wire={wire}");
        }
    }

    #[test]
    fn needs_review_is_snake_case_not_needsreview() {
        let value = serde_json::to_value(PhaseStatus::NeedsReview).unwrap();
        assert_eq!(value, json!("needs_review"));
        let back: PhaseStatus = serde_json::from_value(json!("needs_review")).unwrap();
        assert_eq!(back, PhaseStatus::NeedsReview);
        assert!(serde_json::from_value::<PhaseStatus>(json!("needsreview")).is_err());
    }

    #[test]
    fn phase_status_round_trip_against_project_plan_schema() {
        let schema = compile_schema_named("project-plan.schema.json").expect("embedded schema");
        // Minimal plan skeleton; swap only status.
        let base = json!({
            "schemaVersion": 1,
            "runId": "a1b2c3d4-e5f6-4789-a012-3456789abcde",
            "title": "t",
            "summary": "s",
            "assumptions": [],
            "risks": [],
            "phases": [{
                "phaseId": "P01",
                "title": "t",
                "objective": "o",
                "dependencies": [],
                "projectIds": ["p"],
                "readRoots": ["."],
                "writeRoots": ["."],
                "modelTier": "composer",
                "estimatedMinutes": 1,
                "acceptanceCriteria": [],
                "unitTests": [],
                "integrationTests": [],
                "e2eTests": [],
                "manualChecks": [],
                "rollback": { "checkpoint": "c", "strategy": "restore" },
                "expectedArtifacts": [],
                "prompt": "p",
                "status": "draft",
                "evidence": []
            }],
            "finalGates": []
        });

        for status in SCHEMA_STATUSES {
            let mut plan = base.clone();
            plan["phases"][0]["status"] = json!(status);
            validate_json(&schema, &plan).unwrap_or_else(|e| panic!("{status}: {e}"));
            let typed: ProjectPlan = serde_json::from_value(plan.clone()).expect(status);
            assert_eq!(
                serde_json::to_value(&typed.phases[0].status).unwrap(),
                json!(status)
            );
            let round = serde_json::to_value(&typed).unwrap();
            validate_json(&schema, &round).unwrap_or_else(|e| panic!("round-trip {status}: {e}"));
        }
    }
}
