//! Versioned domain contracts and JSON Schema validation for Tiamat.

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub mod event;
pub mod intake;
pub mod plan;
pub mod validation;

pub use event::EventEnvelope;
pub use intake::{IntakeManifest, IntakeSource, ProjectKind, ProjectSummary};
pub use plan::{
    AcceptanceCriterion, FinalGate, ModelTier, PhasePlan, PhaseStatus, ProjectPlan,
    RollbackStrategy, TestExpected, TestKind, TestSpec,
};
pub use validation::{
    compile_schema, repo_root, schema_path, validate_json, validate_json_str, ValidationError,
};
