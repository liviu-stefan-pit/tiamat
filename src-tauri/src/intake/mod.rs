//! Intake, project discovery, and trust preflight.

mod detect;
mod error;
mod ignore;
mod inventory;
mod limits;
mod paths;
mod preflight;
mod repo;
mod secrets;
mod types;

pub use error::{IntakeError, IntakeResult};
pub use ignore::{is_ignored_dir_name, is_ignored_file_name};
pub use limits::IntakeLimits;
pub use paths::{is_path_within_root, strip_verbatim_prefix};
pub use preflight::{apply_trust, run_preflight, run_preflight_with_configured};
pub use secrets::assert_no_secret_leak;
pub use types::{CursorProbeStub, InventorySummary, PreflightReport, TrustState};

pub const MODULE: &str = "intake";

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use crate::intake::paths::validate_raw_path;

    #[test]
    fn module_name() {
        assert_eq!(MODULE, "intake");
    }

    #[test]
    fn parser_rejects_ads_and_unc() {
        // Alternate data streams are an NTFS concept; a colon is a legal Unix filename char.
        #[cfg(windows)]
        assert!(validate_raw_path(r"C:\x:stream").is_err());
        assert!(validate_raw_path(r"\\host\share").is_err());
    }
}
