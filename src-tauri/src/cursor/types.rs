use serde::{Deserialize, Serialize};

pub const MINIMUM_CURSOR_CLI_VERSION: &str = "0.1.0";
/// Default implementation-agent wall clock. Prefer `TimeoutSettings::from_env()`
/// at call sites so `TIAMAT_PHASE_TIMEOUT_MS` is honoured.
pub const DEFAULT_CURSOR_TIMEOUT_MS: u64 = crate::cursor::timeouts::DEFAULT_PHASE_TIMEOUT_MS;
pub const PROBE_TIMEOUT_MS: u64 = 5_000;
pub const MODELS_PROBE_TIMEOUT_MS: u64 = 8_000;
pub const HELP_EXCERPT_LIMIT: usize = 4_000;

pub const ENV_EXECUTABLE_KEYS: &[&str] = &["TIAMAT_CURSOR_CLI", "CURSOR_CLI_PATH"];
pub const CANDIDATE_NAMES: &[&str] = &["agent", "cursor-agent"];

/// Tiamat only routes to Cursor Composer and Grok families (never SOL or other vendors).
pub fn is_allowed_cursor_model(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.contains("sol") || lower.contains("fast") {
        return false;
    }
    lower.contains("composer") || lower.contains("grok")
}

/// Drop any model IDs outside the Composer/Grok allowlist.
pub fn filter_allowed_cursor_models(models: &[CursorModelInfo]) -> Vec<CursorModelInfo> {
    models
        .iter()
        .filter(|m| is_allowed_cursor_model(&m.id))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorCapabilityStatus {
    Absent,
    Available,
    UnsupportedVersion,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorModelsStatus {
    Available,
    Absent,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorAuthStatus {
    Unknown,
    Ready,
    Unauthenticated,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CursorFeatureFlags {
    pub print_mode: bool,
    pub output_format: bool,
    pub stream_json: bool,
    pub workspace: bool,
    pub force: bool,
    pub model: bool,
    pub list_models: bool,
    pub trust: bool,
    pub api_key: bool,
    pub stream_partial_output: bool,
    pub mode_plan: bool,
    pub resume: bool,
    pub auto_review: bool,
}

impl CursorFeatureFlags {
    /// True when a discovered noninteractive approval flag is present.
    pub fn has_noninteractive_approval(&self) -> bool {
        self.force || self.auto_review
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorCapabilityReport {
    pub status: CursorCapabilityStatus,
    pub message: String,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub version_raw: Option<String>,
    pub minimum_version: String,
    pub help_excerpt: Option<String>,
    pub features: CursorFeatureFlags,
    pub auth: CursorAuthStatus,
    pub auth_message: Option<String>,
    pub models: Vec<CursorModelInfo>,
    pub probed_at_utc: String,
}

impl CursorCapabilityReport {
    pub fn absent(message: impl Into<String>) -> Self {
        Self {
            status: CursorCapabilityStatus::Absent,
            message: message.into(),
            executable: None,
            version: None,
            version_raw: None,
            minimum_version: MINIMUM_CURSOR_CLI_VERSION.into(),
            help_excerpt: None,
            features: CursorFeatureFlags::default(),
            auth: CursorAuthStatus::Unknown,
            auth_message: None,
            models: Vec::new(),
            probed_at_utc: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn summary_status(&self) -> String {
        match self.status {
            CursorCapabilityStatus::Absent => "absent".into(),
            CursorCapabilityStatus::Available => "available".into(),
            CursorCapabilityStatus::UnsupportedVersion => "unsupported_version".into(),
            CursorCapabilityStatus::Error => "error".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorModelInfo {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorModelsReport {
    pub status: CursorModelsStatus,
    pub models: Vec<CursorModelInfo>,
    pub message: Option<String>,
    pub executable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorInvokeRequest {
    pub workspace: String,
    pub model: Option<String>,
    pub prompt: String,
    pub output_format: Option<String>,
    pub resume_chat_id: Option<String>,
    pub force: bool,
    pub trust: bool,
    pub auto_review: bool,
    pub plan_mode: bool,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
}

impl Default for CursorInvokeRequest {
    fn default() -> Self {
        Self {
            workspace: String::new(),
            model: None,
            prompt: String::new(),
            output_format: Some("stream-json".into()),
            resume_chat_id: None,
            force: true,
            trust: true,
            auto_review: false,
            plan_mode: false,
            api_key: None,
            timeout_ms: Some(DEFAULT_CURSOR_TIMEOUT_MS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltCursorCommand {
    /// Argument array for CreateProcess / Command::args — never a shell string.
    pub argv: Vec<String>,
    pub stdin: String,
    pub timeout_ms: u64,
    pub workspace: String,
    pub executable: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorCommandPreview {
    pub argv: Vec<String>,
    pub command_display: String,
    pub stdin_preview: String,
    pub timeout_ms: u64,
    pub workspace: String,
    pub executable: String,
    pub spawned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CursorUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamEventKind {
    System,
    Assistant,
    Result,
    Usage,
    Diagnostic,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedStreamEvent {
    pub kind: StreamEventKind,
    pub raw_line: String,
    pub redacted_line: String,
    pub chat_id: Option<String>,
    pub text: Option<String>,
    pub usage: Option<CursorUsage>,
    pub ok: bool,
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StreamParseResult {
    pub events: Vec<ParsedStreamEvent>,
    pub chat_id: Option<String>,
    pub usage: Option<CursorUsage>,
    pub assistant_text: String,
    /// Markdown harvested from CreatePlan / plan-tool arguments (plan mode).
    /// Cursor plan mode often puts the real document here instead of assistant text.
    #[serde(default)]
    pub plan_markdown: String,
    pub diagnostics: Vec<String>,
    pub terminal_ok: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessCapture {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub duration_ms: u64,
    /// Output exceeded the per-line or total caps in `security::limits` and was clipped.
    pub truncated: bool,
    /// Output arrived faster than the caps allow, so the tail was dropped entirely.
    pub flood_detected: bool,
    /// Whether the process tree was verifiably torn down with zero survivors.
    pub cleanup_ok: bool,
    /// Process-tree teardown needed force or could not be verified; evidence, not an error.
    pub cleanup_warning: Option<String>,
}
