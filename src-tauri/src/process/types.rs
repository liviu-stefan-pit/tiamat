use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

/// Process lifecycle from MASTER-PLAN §8.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    Registered,
    Spawned,
    Active,
    GracefulStop,
    ForcedStop,
    Reaped,
}

impl ProcessState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Spawned => "spawned",
            Self::Active => "active",
            Self::GracefulStop => "graceful_stop",
            Self::ForcedStop => "forced_stop",
            Self::Reaped => "reaped",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "spawned" => Self::Spawned,
            "active" => Self::Active,
            "graceful_stop" => Self::GracefulStop,
            "forced_stop" => Self::ForcedStop,
            "reaped" => Self::Reaped,
            _ => Self::Registered,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Reaped)
    }

    pub fn is_active(self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRecord {
    pub process_id: Uuid,
    pub run_id: Uuid,
    pub phase_id: Option<String>,
    pub attempt_id: Option<Uuid>,
    pub executable: String,
    pub args_redacted: Vec<String>,
    pub pid: Option<u32>,
    pub creation_time_100ns: Option<u64>,
    pub executable_identity: Option<String>,
    pub job_name: Option<String>,
    pub job_associated: bool,
    pub parent_pid: Option<u32>,
    pub workspace: Option<String>,
    pub state: ProcessState,
    pub heartbeat_at_utc: Option<String>,
    pub registered_at_utc: String,
    pub spawned_at_utc: Option<String>,
    pub stopped_at_utc: Option<String>,
    pub reaped_at_utc: Option<String>,
    pub exit_code: Option<i32>,
    pub terminal_reason: Option<String>,
    pub chat_id: Option<String>,
    pub resume_metadata: Value,
    pub cleanup_evidence: Value,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CleanupProof {
    pub proof_id: Uuid,
    pub run_id: Uuid,
    pub process_id: Option<Uuid>,
    pub observed_at_utc: String,
    pub active_process_count: u32,
    pub job_handle_open: bool,
    pub handles_closed: bool,
    pub zero_active_observed: bool,
    pub success: bool,
    pub detail: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogConfig {
    /// Emit `attempt.warning` after this many milliseconds.
    pub warn_after_ms: u64,
    /// Request graceful stop after this many milliseconds.
    pub graceful_after_ms: u64,
    /// Force Job Object terminate after graceful request.
    pub force_grace_ms: u64,
    /// Bound for draining stdout/stderr after stop.
    pub drain_timeout_ms: u64,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            warn_after_ms: 8 * 60 * 1000,
            graceful_after_ms: 10 * 60 * 1000,
            force_grace_ms: 15_000,
            drain_timeout_ms: 2_000,
        }
    }
}

impl WatchdogConfig {
    /// Fast timings for unit/integration tests.
    pub fn for_tests() -> Self {
        Self {
            warn_after_ms: 80,
            graceful_after_ms: 160,
            force_grace_ms: 50,
            drain_timeout_ms: 500,
        }
    }
}

/// Same-chat resume prompt after attempt watchdog timeout (MASTER-PLAN §13.3).
pub const TIMEOUT_RESUME_PROMPT: &str = r#"Resume the same assigned phase after an interrupted attempt. Read
.tiamat/MASTER-PLAN.md and .tiamat/plan.json first. Inspect git status, the current
diff, persisted test evidence, and the interruption report supplied below. Preserve
valid progress, repair partial or inconsistent work, and implement only this phase.
Do not repeat completed work blindly. Add and run the phase's unit, integration,
and end-to-end tests as applicable. Do not mark the phase complete until all
acceptance gates pass. Leave the workspace coherent and checkpoint-ready, then
return the required immutable
phase-result payload; Tiamat will transactionally update both plan projections."#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResumeMetadata {
    pub chat_id: Option<String>,
    pub parent_attempt_id: Option<Uuid>,
    pub next_model: Option<String>,
    pub next_tier: Option<String>,
    pub reason: String,
    pub progress_useful: bool,
    pub recovery_prompt: String,
}

impl ResumeMetadata {
    pub fn timeout_resume(
        chat_id: Option<String>,
        parent_attempt_id: Option<Uuid>,
        next_model: Option<String>,
        next_tier: Option<String>,
        progress_useful: bool,
    ) -> Self {
        Self {
            chat_id,
            parent_attempt_id,
            next_model,
            next_tier,
            reason: "attempt_watchdog_timeout".into(),
            progress_useful,
            recovery_prompt: TIMEOUT_RESUME_PROMPT.trim().to_string(),
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({}))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AbortSettings {
    pub shortcut: String,
    pub registered: bool,
    pub degraded: bool,
    pub collision_reason: Option<String>,
    pub degraded_acknowledged: bool,
    pub tray_fallback_enabled: bool,
    pub second_press_force_ms: u64,
    pub updated_at_utc: String,
}

impl Default for AbortSettings {
    fn default() -> Self {
        Self {
            shortcut: "Ctrl+Shift+F12".into(),
            registered: false,
            degraded: false,
            collision_reason: None,
            degraded_acknowledged: false,
            tray_fallback_enabled: true,
            second_press_force_ms: 3_000,
            updated_at_utc: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortAction {
    BeginEmergencyCancel,
    ForceTerminate,
    PromptConfirm,
    Acknowledged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AbortPressResult {
    pub action: AbortAction,
    pub forced: bool,
    pub active_run: bool,
    pub message: String,
    pub processes_stopped: u32,
    pub cleanup_ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosePolicyChoice {
    KeepRunning,
    StopAllAndExit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostedProcessOutcome {
    pub process_id: Uuid,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub killed: bool,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub chat_id: Option<String>,
    pub resume: Option<ResumeMetadata>,
    pub cleanup_ok: bool,
    pub zero_survivors: bool,
    pub active_after_cleanup: u32,
    /// Output hit the caps in `security::limits`, so stdout/stderr here are clipped.
    pub truncated: bool,
    /// Output was clipped because it arrived faster than the caps allow.
    pub flood_detected: bool,
    /// Teardown succeeded but needed force, or could not be fully verified.
    pub cleanup_warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SpawnRequest {
    pub run_id: Uuid,
    pub phase_id: Option<String>,
    pub attempt_id: Option<Uuid>,
    pub argv: Vec<String>,
    pub stdin: Option<String>,
    pub workspace: Option<String>,
    pub env: Vec<(String, String)>,
    pub watchdog: WatchdogConfig,
    /// Expected chat id carrier for resume metadata after timeout.
    pub resume_chat_hint: Option<String>,
    pub next_model_on_timeout: Option<String>,
    pub next_tier_on_timeout: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRegistrySnapshot {
    pub active_count: u32,
    pub processes: Vec<ProcessRecord>,
    pub abort: AbortSettings,
    pub can_start: bool,
    pub cleanup_incomplete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileReport {
    pub inspected: u32,
    pub terminated: u32,
    pub already_gone: u32,
    pub unverifiable: u32,
    pub interrupted_attempts: u32,
    pub hard_failure: bool,
    pub messages: Vec<String>,
}
