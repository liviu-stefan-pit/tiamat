//! Test discovery, command policy, evidence capture, baseline/flaky classification.

mod classify;
mod discover;
mod error;
mod policy;
mod runner;
mod types;

pub use classify::{classify_baseline, classify_flaky_retry};
pub use discover::discover_advisory_tests;
pub use error::{VerificationError, VerificationResult};
pub use policy::{
    evaluate_command_policy, evaluate_command_policy_in_roots, CommandPolicyDecision,
};
pub use runner::{run_phase_gates, GateRunOptions, GateRunReport};
pub use types::*;

pub const MODULE: &str = "verification";
