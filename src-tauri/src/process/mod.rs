//! Windows Job Object process host, durable registry, watchdog, and global abort.

mod abort;
mod error;
mod host;
mod identity;
mod job;
mod reconcile;
mod spawn;
mod types;
mod windows_cmd;

pub use abort::{
    acknowledge_degraded, can_start_with_abort_policy, mark_shortcut_registered, rebind_shortcut,
    AbortController,
};
pub use error::{ProcessError, ProcessResult};
pub use host::{
    run_argv_hosted_for_tests, run_capture_hosted, watchdog_for_timeout, EventSink,
    HostedSpawnContext, ProcessHost,
};
pub use reconcile::reconcile_owned_processes;
pub use spawn::{
    attribute_list_api_available, prove_attribute_list_association, AssociationMethod,
};
pub use types::*;
pub use windows_cmd::{normalize_windows_argv, quote_windows_arg};

pub const MODULE: &str = "process";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchdog_defaults_match_plan() {
        let cfg = WatchdogConfig::default();
        assert_eq!(cfg.warn_after_ms, 8 * 60 * 1000);
        assert_eq!(cfg.graceful_after_ms, 10 * 60 * 1000);
        assert_eq!(cfg.force_grace_ms, 15_000);
    }

    #[test]
    fn process_state_machine_terminal() {
        assert!(ProcessState::Reaped.is_terminal());
        assert!(ProcessState::Active.is_active());
        assert!(!ProcessState::Registered.is_terminal());
    }

    #[cfg(windows)]
    #[test]
    fn attribute_list_job_association_works() {
        let method = prove_attribute_list_association().expect("JOB_LIST association");
        assert_eq!(method, AssociationMethod::ProcThreadAttributeJobList);
        assert!(attribute_list_api_available());
    }
}
