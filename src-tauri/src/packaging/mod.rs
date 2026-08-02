//! Packaging, install/upgrade/uninstall policy, and packaged acceptance helpers (P11).

mod install;
mod paths;
mod proof;
mod types;

pub use install::{
    ensure_upgrade_scaffold, plan_uninstall_retention, sample_unpromoted_manifest,
    simulate_upgrade_preserve, UninstallPlan, UpgradePreserveResult,
};
pub use paths::{
    create_long_path_fixture, is_extended_path, long_path_prefix, normalize_windows_path,
};
pub use proof::{assert_zero_owned_processes, write_cleanup_proof_artifact, PackagedCleanupReport};
pub use types::{
    AppSettings, InstallScenario, PackageArtifact, PackageManifest, PackagingError, PackagingResult,
};
