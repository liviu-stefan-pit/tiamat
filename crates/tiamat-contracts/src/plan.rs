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
#[serde(rename_all = "lowercase")]
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
