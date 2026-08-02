//! Command policy — re-exported from security for verification callers.

pub use crate::security::{
    evaluate_command_policy, evaluate_command_policy_in_roots, CommandPolicyDecision,
};
