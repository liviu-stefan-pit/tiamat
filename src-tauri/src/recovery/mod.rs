//! Crash recovery, idempotent side effects, startup reconciliation, and fault injection.

mod disk;
mod error;
mod fault;
mod integrity;
mod ledger;
mod startup;
mod types;

pub use disk::{probe_disk_default, probe_disk_pressure};
pub use error::{RecoveryError, RecoveryResult};
pub use fault::{
    check_fault, clear_faults, hit_count, list_faults, set_fault, set_faults, FaultAction,
    FaultPoint, FaultRule,
};
pub use integrity::{
    open_store_with_integrity_guard, preserve_corrupt_copy, verify_store_integrity,
    write_header_corrupted_db_fixture, write_malformed_db_fixture, IntegrityOpenResult,
};
pub use ledger::{execute_idempotent, make_idempotency_key, reconcile_side_effect};
pub use startup::{execution_allowed, resolve_cancel, resolve_resume, run_startup_recovery};
pub use types::{
    DiskPressureReport, InterruptedAttemptSummary, RecoveryOffer, RecoveryOfferStatus,
    RecoveryScanReport, RetentionSettings, SideEffectKind, SideEffectRecord, SideEffectState,
    DEFAULT_LOW_DISK_THRESHOLD_BYTES,
};

pub const MODULE: &str = "recovery";
