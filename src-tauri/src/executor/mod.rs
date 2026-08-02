//! Phase executor: prompt assembly, managed-root execution, diff/boundary, result projection.

mod diff;
mod error;
mod prompt;
mod recover;
mod result;
mod run;
mod types;

pub use diff::{
    collect_changed_files, find_new_escapes, snapshot_paths, validate_diff_boundaries,
    DiffBoundaryReport,
};
pub use error::{ExecutorError, ExecutorResult};
pub use prompt::{assemble_phase_prompt, assemble_recovery_prompt, RECOVERY_PROMPT_PREFIX};
pub use recover::{decide_partial_recovery, PartialRecoveryDecision};
pub use result::{extract_phase_result, validate_phase_result_payload};
pub use run::{execute_phase, map_result_status_to_phase, ExecutePhaseRequest};
pub use types::*;

pub const MODULE: &str = "executor";
