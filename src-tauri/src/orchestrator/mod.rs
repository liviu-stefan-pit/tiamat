//! Run orchestrator: architect → DAG scheduler → real phase execution → completion.
//!
//! The scheduler's `tick()` only marks phases Running and inserts attempt rows.
//! This module owns the missing link: spawn `execute_phase` for each new attempt,
//! wait for workers, call `complete_attempt`, and loop until the run is terminal.

mod runner;
mod types;

pub use runner::{
    cancel_active_run, get_run_status, start_run, OrchestratorHandle, OrchestratorSlot,
};
pub use types::{RunStatusSnapshot, StartRunRequest, StartRunResult};

pub const MODULE: &str = "orchestrator";
