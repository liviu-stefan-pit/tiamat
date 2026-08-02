use serde::{Deserialize, Serialize};
use tiamat_contracts::{EvidenceRecord, PhaseResult, PhaseStatus};
use uuid::Uuid;

use crate::verification::LayerGateSummary;
use crate::workspace::{CheckpointRecord, QuarantineRecord};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhaseExecutionOutcome {
    pub ok: bool,
    pub run_id: Uuid,
    pub phase_id: String,
    pub attempt_id: Option<Uuid>,
    pub terminal_status: PhaseStatus,
    pub phase_result: Option<PhaseResult>,
    pub evidence: Vec<EvidenceRecord>,
    pub layers: Vec<LayerGateSummary>,
    pub changed_files: Vec<String>,
    pub boundary_ok: bool,
    pub quarantined: Option<QuarantineRecord>,
    pub project_checkpoint: Option<CheckpointRecord>,
    pub control_checkpoint: Option<CheckpointRecord>,
    pub plan_projected: bool,
    pub recovery: Option<RecoveryReport>,
    pub chat_id: Option<String>,
    pub message: String,
    pub evidence_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub decision: String,
    pub progress_useful: bool,
    pub reason: String,
    pub resumed: bool,
    pub rolled_back: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Fresh,
    Resume,
}
