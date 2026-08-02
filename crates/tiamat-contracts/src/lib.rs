//! Versioned domain contracts and JSON Schema validation for Tiamat.

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub mod event;
pub mod intake;
pub mod phase_result;
pub mod plan;
pub mod validation;

pub use event::{EventEnvelope, EventLevel};
pub use intake::{IntakeManifest, IntakeSource, ProjectKind, ProjectSummary, SourceKind};
pub use phase_result::{EvidenceClassification, EvidenceRecord, PhaseResult, PhaseResultStatus};
pub use plan::{
    AcceptanceCriterion, FinalGate, ManualCheck, ModelTier, PhasePlan, PhaseStatus, ProjectPlan,
    RollbackSpec, RollbackStrategy, TestExpected, TestKind, TestSpec,
};
pub use validation::{
    compile_schema, compile_schema_named, embedded_schema_text, repo_root, schema_path,
    validate_json, validate_json_str, ValidationError, EMBEDDED_SCHEMA_NAMES,
};
