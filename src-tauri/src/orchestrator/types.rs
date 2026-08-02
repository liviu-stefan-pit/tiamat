use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartRunRequest {
    pub input_paths: Vec<String>,
    pub output_dir: String,
    /// Optional concurrency override (clamped 1..=4).
    pub max_concurrent: Option<u32>,
    /// When set, forces the fake-CLI mode for deterministic tests.
    pub fake_cli_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartRunResult {
    pub run_id: Uuid,
    pub status: String,
    pub message: String,
    pub managed_run_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunStatusSnapshot {
    pub run_id: Option<Uuid>,
    pub status: String,
    pub phase: Option<String>,
    pub message: String,
    pub active_attempts: u32,
    pub completed_phases: u32,
    pub total_phases: u32,
    pub managed_run_root: Option<String>,
}
