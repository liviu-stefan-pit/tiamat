use serde::{Deserialize, Serialize};
use tiamat_contracts::IntakeManifest;

use crate::intake::limits::IntakeLimits;
use crate::intake::secrets::SecretRiskFinding;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InventorySummary {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_bytes: u64,
    pub ignored_count: u64,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
    pub estimated_copy_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TrustState {
    pub confirmed: bool,
    pub acknowledged_untrusted: bool,
    pub acknowledged_execution_risk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CursorProbeStub {
    pub status: String,
    pub message: String,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub auth: Option<String>,
    pub model_count: Option<u32>,
    pub has_noninteractive_approval: Option<bool>,
}

impl Default for CursorProbeStub {
    fn default() -> Self {
        Self {
            status: "absent".into(),
            message: "Cursor CLI not probed yet.".into(),
            executable: None,
            version: None,
            auth: None,
            model_count: None,
            has_noninteractive_approval: None,
        }
    }
}

impl From<&crate::cursor::CursorCapabilityReport> for CursorProbeStub {
    fn from(report: &crate::cursor::CursorCapabilityReport) -> Self {
        Self {
            status: report.summary_status(),
            message: report.message.clone(),
            executable: report.executable.clone(),
            version: report.version.clone(),
            auth: Some(match report.auth {
                crate::cursor::CursorAuthStatus::Unknown => "unknown".into(),
                crate::cursor::CursorAuthStatus::Ready => "ready".into(),
                crate::cursor::CursorAuthStatus::Unauthenticated => "unauthenticated".into(),
                crate::cursor::CursorAuthStatus::Error => "error".into(),
            }),
            model_count: Some(report.models.len() as u32),
            has_noninteractive_approval: Some(report.features.has_noninteractive_approval()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    pub schema_version: u32,
    pub manifest: IntakeManifest,
    pub inventory: InventorySummary,
    pub warnings: Vec<String>,
    pub blockers: Vec<String>,
    pub secret_risks: Vec<SecretRiskFinding>,
    pub escape_attempts: Vec<String>,
    pub trust: TrustState,
    pub cursor: CursorProbeStub,
    pub can_start: bool,
    pub read_roots: Vec<String>,
    pub write_roots_preview: Vec<String>,
    pub limits: IntakeLimits,
    pub untrusted_content_notice: String,
}

impl PreflightReport {
    pub fn recompute_can_start(&mut self) {
        let trust_ok = self.trust.confirmed
            && self.trust.acknowledged_untrusted
            && self.trust.acknowledged_execution_risk;
        self.can_start = trust_ok && self.blockers.is_empty() && !self.manifest.sources.is_empty();
    }
}
