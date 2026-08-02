//! Security: redaction, command policy, prompt-injection boundaries, limits, audit.

mod audit;
mod injection;
mod limits;
mod policy;
mod redaction;

pub use audit::{emit_policy_event, emit_security_event, AuditEvent};
pub use injection::{
    assert_write_roots_unchanged, injection_defense_block, scan_prompt_injection_markers,
    InjectionScanResult, PROMPT_INJECTION_DEFENSE,
};
pub use limits::{
    apply_output_limits, check_prompt_size, OutputLimitConfig, OutputLimitResult,
    DEFAULT_MAX_LINE_BYTES, DEFAULT_MAX_PROMPT_BYTES, DEFAULT_MAX_TOTAL_OUTPUT_BYTES,
};
pub use policy::{
    evaluate_command_policy, evaluate_command_policy_in_roots, CommandPolicyDecision,
};
pub use redaction::{
    content_hash, redact_for_persistence, redact_line, RedactionStats, FORBIDDEN_FIXTURE_SECRETS,
};

pub const MODULE: &str = "security";
