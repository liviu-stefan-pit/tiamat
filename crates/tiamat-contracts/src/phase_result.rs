use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::plan::TestKind;

/// Immutable agent-submitted phase result. Only the orchestrator may project this into plan files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PhaseResult {
    pub schema_version: u32,
    pub phase_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<Uuid>,
    pub status: PhaseResultStatus,
    pub summary: String,
    pub changed_files: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub acceptance_satisfied: Vec<String>,
    pub artifacts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Must be true; agents cannot submit mutable draft results.
    pub immutable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_useful: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interruption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseResultStatus {
    Passed,
    Failed,
    NeedsReview,
}

/// Captured test/diff/review evidence associated with a phase attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRecord {
    pub schema_version: u32,
    pub evidence_id: String,
    pub kind: TestKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    pub command: Vec<String>,
    pub working_directory: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub summary: String,
    #[serde(default)]
    pub artifact_hashes: Vec<String>,
    #[serde(default)]
    pub covers: Vec<String>,
    pub trustworthy: bool,
    pub partial: bool,
    pub classification: EvidenceClassification,
    pub started_at_utc: String,
    pub ended_at_utc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flaky_retry: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClassification {
    Pass,
    Fail,
    BaselineFail,
    FlakyPass,
    FlakyFail,
    Skipped,
    PolicyDenied,
}

impl PhaseResult {
    pub fn validate_immutable(&self) -> Result<(), String> {
        if self.schema_version != crate::CURRENT_SCHEMA_VERSION {
            return Err(format!(
                "incompatible schema version: expected {}, found {}",
                crate::CURRENT_SCHEMA_VERSION,
                self.schema_version
            ));
        }
        if !self.immutable {
            return Err("phase result must set immutable=true".into());
        }
        if self.phase_id.trim().is_empty() {
            return Err("phaseId required".into());
        }
        if self.summary.trim().is_empty() {
            return Err("summary required".into());
        }
        Ok(())
    }
}
