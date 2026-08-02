use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Side-effect kinds that participate in prepared→executing→observed→reconciled recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectKind {
    PlanWrite,
    DbCommit,
    ProcessSpawn,
    ProcessExit,
    TestLaunch,
    GitCheckpoint,
}

impl SideEffectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlanWrite => "plan_write",
            Self::DbCommit => "db_commit",
            Self::ProcessSpawn => "process_spawn",
            Self::ProcessExit => "process_exit",
            Self::TestLaunch => "test_launch",
            Self::GitCheckpoint => "git_checkpoint",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "plan_write" => Self::PlanWrite,
            "db_commit" => Self::DbCommit,
            "process_spawn" => Self::ProcessSpawn,
            "process_exit" => Self::ProcessExit,
            "test_launch" => Self::TestLaunch,
            "git_checkpoint" => Self::GitCheckpoint,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectState {
    Prepared,
    Executing,
    Observed,
    Reconciled,
}

impl SideEffectState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Executing => "executing",
            Self::Observed => "observed",
            Self::Reconciled => "reconciled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "prepared" => Self::Prepared,
            "executing" => Self::Executing,
            "observed" => Self::Observed,
            "reconciled" => Self::Reconciled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Reconciled)
    }

    pub fn needs_reconcile(self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SideEffectRecord {
    pub idempotency_key: String,
    pub run_id: Uuid,
    pub kind: SideEffectKind,
    pub state: SideEffectState,
    pub external_fact: serde_json::Value,
    pub created_at_utc: String,
    pub updated_at_utc: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOfferStatus {
    Pending,
    Resumed,
    Cancelled,
    Blocked,
}

impl RecoveryOfferStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resumed => "resumed",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "resumed" => Self::Resumed,
            "cancelled" => Self::Cancelled,
            "blocked" => Self::Blocked,
            _ => Self::Pending,
        }
    }
}

/// Startup recovery offer shown before any new scheduling/execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryOffer {
    pub offer_id: String,
    pub run_id: Uuid,
    pub status: RecoveryOfferStatus,
    pub reason: String,
    pub db_integrity_ok: bool,
    pub process_hard_failure: bool,
    pub interrupted_attempt_count: u32,
    pub unreconciled_side_effects: u32,
    pub low_disk: bool,
    pub corrupt_db_backup_path: Option<String>,
    pub details: serde_json::Value,
    pub created_at_utc: String,
    pub resolved_at_utc: Option<String>,
    pub resolution: Option<String>,
    /// True when the user must choose Resume or Cancel before new work.
    pub requires_user_choice: bool,
    /// When true, Resume is disabled (e.g. corrupt DB / hard cleanup failure).
    pub resume_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryScanReport {
    pub schema_version: u32,
    pub scanned_at_utc: String,
    pub db_integrity_ok: bool,
    pub schema_version_ok: bool,
    pub process_reconcile: Option<crate::process::ReconcileReport>,
    pub interrupted_attempts: Vec<InterruptedAttemptSummary>,
    pub unreconciled_side_effects: Vec<SideEffectRecord>,
    pub low_disk: bool,
    pub free_disk_bytes: Option<u64>,
    pub disk_path: Option<String>,
    pub offer: Option<RecoveryOffer>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InterruptedAttemptSummary {
    pub attempt_id: Uuid,
    pub run_id: Uuid,
    pub phase_id: String,
    pub prior_status: String,
    pub terminal_result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RetentionSettings {
    pub retain_run_metadata_days: u32,
    pub retain_redacted_logs_days: u32,
    pub retain_unpromoted_workspaces: bool,
    pub allow_destructive_cleanup: bool,
    pub updated_at_utc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiskPressureReport {
    pub path: String,
    pub free_bytes: Option<u64>,
    pub low_disk: bool,
    pub threshold_bytes: u64,
    pub message: String,
}

/// Default low-disk threshold: 512 MiB free.
pub const DEFAULT_LOW_DISK_THRESHOLD_BYTES: u64 = 512 * 1024 * 1024;
