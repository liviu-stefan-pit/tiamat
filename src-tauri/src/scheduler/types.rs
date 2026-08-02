use serde::{Deserialize, Serialize};
use tiamat_contracts::ModelTier;
use uuid::Uuid;

pub const MODEL_COMPOSER: &str = "composer-2.5";
pub const MODEL_GROK_LOW: &str = "cursor-grok-4.5-low";
pub const MODEL_GROK_MEDIUM: &str = "cursor-grok-4.5-medium";
pub const MODEL_GROK_HIGH: &str = "cursor-grok-4.5-high";
pub const MODEL_SOL: &str = "gpt-5.6-sol-high";

pub const DEFAULT_MAX_ATTEMPTS: u32 = 4;
pub const LEASE_TTL_SECS: i64 = 30;
pub const ORCHESTRATOR_MODE: &str = "dag-scheduler";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseRuntimeStatus {
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

impl PhaseRuntimeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Verifying => "verifying",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
            Self::NeedsReview => "needs_review",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "ready" => Self::Ready,
            "queued" => Self::Queued,
            "running" => Self::Running,
            "verifying" => Self::Verifying,
            "passed" => Self::Passed,
            "failed" => Self::Failed,
            "blocked" => Self::Blocked,
            "cancelled" => Self::Cancelled,
            "skipped" => Self::Skipped,
            "needs_review" => Self::NeedsReview,
            _ => Self::Draft,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Passed | Self::Failed | Self::Cancelled | Self::Skipped
        )
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Verifying)
    }

    pub fn counts_as_success_dep(self) -> bool {
        matches!(self, Self::Passed | Self::Skipped)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Starting,
    Running,
    Stopping,
    Completed,
}

impl AttemptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Completed => "completed",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "stopping" => Self::Stopping,
            "completed" => Self::Completed,
            _ => Self::Starting,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Stopping)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptTerminalResult {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Killed,
    PolicyDenied,
    Lost,
}

impl AttemptTerminalResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::Killed => "killed",
            Self::PolicyDenied => "policy_denied",
            Self::Lost => "lost",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "timed_out" => Self::TimedOut,
            "cancelled" => Self::Cancelled,
            "killed" => Self::Killed,
            "policy_denied" => Self::PolicyDenied,
            "lost" => Self::Lost,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Timeout,
    TestFailure,
    MalformedOutput,
    LowConfidenceReview,
    Policy,
    Auth,
    Build,
    Interrupted,
    Other,
}

impl FailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::TestFailure => "test_failure",
            Self::MalformedOutput => "malformed_output",
            Self::LowConfidenceReview => "low_confidence_review",
            Self::Policy => "policy",
            Self::Auth => "auth",
            Self::Build => "build",
            Self::Interrupted => "interrupted",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "timeout" => Self::Timeout,
            "test_failure" => Self::TestFailure,
            "malformed_output" => Self::MalformedOutput,
            "low_confidence_review" => Self::LowConfidenceReview,
            "policy" => Self::Policy,
            "auth" => Self::Auth,
            "build" => Self::Build,
            "interrupted" => Self::Interrupted,
            _ => Self::Other,
        }
    }

    /// Deterministic policy/auth/build failures do not consume blind model escalations.
    pub fn is_deterministic(self) -> bool {
        matches!(self, Self::Policy | Self::Auth | Self::Build)
    }

    pub fn should_escalate(self) -> bool {
        matches!(
            self,
            Self::Timeout
                | Self::TestFailure
                | Self::MalformedOutput
                | Self::LowConfidenceReview
                | Self::Other
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PhaseRecord {
    pub run_id: Uuid,
    pub phase_id: String,
    pub title: String,
    pub status: PhaseRuntimeStatus,
    pub project_ids: Vec<String>,
    pub write_roots: Vec<String>,
    pub resource_locks: Vec<String>,
    pub dependencies: Vec<String>,
    pub model_tier: ModelTier,
    pub estimated_minutes: u32,
    pub critical_path_length: u32,
    pub ready_at_utc: Option<String>,
    pub queued_at_utc: Option<String>,
    pub attempt_count: u32,
    pub last_failure_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttemptRecord {
    pub attempt_id: Uuid,
    pub run_id: Uuid,
    pub phase_id: String,
    pub attempt_number: u32,
    pub status: AttemptStatus,
    pub terminal_result: Option<AttemptTerminalResult>,
    pub requested_tier: ModelTier,
    pub requested_model: String,
    pub selected_model: String,
    pub selection_reason: String,
    pub availability: Vec<String>,
    pub resume_parent_attempt_id: Option<Uuid>,
    pub progress_useful: bool,
    pub failure_kind: Option<FailureKind>,
    pub started_at_utc: Option<String>,
    pub finished_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerLease {
    pub run_id: Uuid,
    pub lease_holder: String,
    pub epoch: u64,
    pub renewed_at_utc: String,
    pub expires_at_utc: String,
    pub paused: bool,
    pub max_concurrent: u32,
    pub cleanup_incomplete: bool,
    pub low_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub requested_tier: ModelTier,
    pub requested_model: String,
    pub selected_model: String,
    pub selection_reason: String,
    pub substituted: bool,
    pub escalated: bool,
    pub available_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerConfig {
    pub max_concurrent: u32,
    pub max_attempts: u32,
    pub lease_holder: String,
    pub allow_downgrade_before_first_attempt: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            lease_holder: "tiamat-local".into(),
            allow_downgrade_before_first_attempt: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerSnapshot {
    pub run_id: Uuid,
    pub mode: String,
    pub paused: bool,
    pub epoch: u64,
    pub max_concurrent: u32,
    pub active_attempts: u32,
    pub phases: Vec<PhaseRecord>,
    pub attempts: Vec<AttemptRecord>,
    pub held_locks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TickResult {
    pub epoch: u64,
    pub started: Vec<String>,
    pub blocked: Vec<String>,
    pub skipped_due_to_pause: bool,
    pub skipped_due_to_capacity: bool,
    pub message: String,
}

pub fn default_max_concurrent() -> u32 {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    ((cpus / 4).max(1) as u32).clamp(1, 3)
}

pub fn preferred_model_for_tier(tier: &ModelTier) -> &'static str {
    match tier {
        ModelTier::Composer => MODEL_COMPOSER,
        ModelTier::GrokLow => MODEL_GROK_LOW,
        ModelTier::GrokMedium => MODEL_GROK_MEDIUM,
        ModelTier::GrokHigh => MODEL_GROK_HIGH,
    }
}

pub fn tier_rank(tier: &ModelTier) -> u8 {
    match tier {
        ModelTier::Composer => 0,
        ModelTier::GrokLow => 1,
        ModelTier::GrokMedium => 2,
        ModelTier::GrokHigh => 3,
    }
}

pub fn escalate_tier(tier: &ModelTier) -> Option<ModelTier> {
    match tier {
        ModelTier::Composer => Some(ModelTier::GrokLow),
        ModelTier::GrokLow => Some(ModelTier::GrokMedium),
        ModelTier::GrokMedium => Some(ModelTier::GrokHigh),
        ModelTier::GrokHigh => None,
    }
}

pub fn parse_model_tier(value: &str) -> ModelTier {
    match value {
        "grok-low" => ModelTier::GrokLow,
        "grok-medium" => ModelTier::GrokMedium,
        "grok-high" => ModelTier::GrokHigh,
        _ => ModelTier::Composer,
    }
}

pub fn model_tier_str(tier: &ModelTier) -> &'static str {
    match tier {
        ModelTier::Composer => "composer",
        ModelTier::GrokLow => "grok-low",
        ModelTier::GrokMedium => "grok-medium",
        ModelTier::GrokHigh => "grok-high",
    }
}

/// Normalize a write root into a stable lock key (case-folded, trimmed separators).
pub fn write_lock_name(root: &str) -> String {
    let normalized = root
        .trim()
        .trim_end_matches(['/', '\\'])
        .replace('/', "\\")
        .to_ascii_lowercase();
    format!("write:{normalized}")
}

pub fn resource_lock_name(name: &str) -> String {
    format!("resource:{}", name.trim().to_ascii_lowercase())
}
