use serde::{Deserialize, Serialize};
use tiamat_contracts::ProjectPlan;

use crate::cursor::CursorUsage;
use crate::workspace::CheckpointRecord;

pub const ARCHITECT_PREFERRED_MODEL: &str = "gpt-5.6-sol-high";
pub const ARCHITECT_FALLBACK_MODEL: &str = "cursor-grok-4.5-high";
pub const ARCHITECT_ROLE: &str = "architect";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectModelSelection {
    pub requested_model: String,
    pub selected_model: String,
    pub degraded: bool,
    pub reason: String,
    pub available_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectInvocationProof {
    /// Architect always runs in plan mode.
    pub plan_mode: bool,
    /// Architect must never receive implementation approval (`--force`).
    pub force: bool,
    /// Architect must never receive auto-review approval.
    pub auto_review: bool,
    /// Workspace is the read-only control/intake mount, never a product write root.
    pub workspace: String,
    pub argv: Vec<String>,
    pub model: String,
}

impl ArchitectInvocationProof {
    pub fn cannot_implement(&self) -> bool {
        self.plan_mode
            && !self.force
            && !self.auto_review
            && self
                .argv
                .windows(2)
                .any(|w| w[0] == "--mode" && w[1] == "plan")
            && !self.argv.iter().any(|a| a == "--force")
            && !self.argv.iter().any(|a| a == "--auto-review")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanValidationIssue {
    pub code: String,
    pub message: String,
    pub phase_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanArtifactHashes {
    pub plan_json_sha256: String,
    pub master_plan_md_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectAttemptRecord {
    pub attempt: u32,
    pub model: String,
    pub chat_id: Option<String>,
    pub usage: Option<CursorUsage>,
    pub exit_code: Option<i32>,
    pub repaired: bool,
    pub validation_issues: Vec<PlanValidationIssue>,
    pub proof: ArchitectInvocationProof,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectRunResult {
    pub ok: bool,
    pub run_id: String,
    pub model_selection: ArchitectModelSelection,
    pub plan: Option<ProjectPlan>,
    pub plan_json_path: Option<String>,
    pub master_plan_md_path: Option<String>,
    pub hashes: Option<PlanArtifactHashes>,
    pub checkpoint: Option<CheckpointRecord>,
    pub attempts: Vec<ArchitectAttemptRecord>,
    pub degraded_mode: bool,
    pub error: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphProjection {
    pub run_id: String,
    pub title: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub phase_id: String,
    pub title: String,
    pub status: String,
    pub model_tier: String,
    pub objective: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
}
