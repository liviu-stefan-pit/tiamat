use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackagingError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type PackagingResult<T> = Result<T, PackagingError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub cursor_cli_path: Option<String>,
    pub canary_capability_hash: Option<String>,
    pub canary_consented_at_utc: Option<String>,
    pub canary_last_success_at_utc: Option<String>,
    pub canary_last_version: Option<String>,
    pub updated_at_utc: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            cursor_cli_path: None,
            canary_capability_hash: None,
            canary_consented_at_utc: None,
            canary_last_success_at_utc: None,
            canary_last_version: None,
            updated_at_utc: "1970-01-01T00:00:00Z".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallScenario {
    FreshInstall,
    UpgradePreserve,
    UninstallRetainUnpromoted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PackageArtifact {
    pub path: String,
    pub kind: String,
    pub sha256: String,
    pub byte_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PackageManifest {
    pub version: String,
    pub product_name: String,
    pub created_at_utc: String,
    pub artifacts: Vec<PackageArtifact>,
    pub signing: String,
}
