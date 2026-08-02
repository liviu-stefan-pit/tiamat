use serde::{Deserialize, Serialize};

/// Configurable inventory and scan limits for intake preflight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntakeLimits {
    pub max_files: u64,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_secret_scan_bytes: u64,
    pub max_depth: u32,
}

impl Default for IntakeLimits {
    fn default() -> Self {
        Self {
            max_files: 25_000,
            max_total_bytes: 1_073_741_824, // 1 GiB
            max_file_bytes: 64 * 1024 * 1024,
            max_secret_scan_bytes: 1024 * 1024,
            max_depth: 32,
        }
    }
}

impl IntakeLimits {
    /// Tight limits for unit tests that assert over-limit failures.
    pub fn for_tests_small() -> Self {
        Self {
            max_files: 3,
            max_total_bytes: 4_096,
            max_file_bytes: 2_048,
            max_secret_scan_bytes: 2_048,
            max_depth: 8,
        }
    }
}
