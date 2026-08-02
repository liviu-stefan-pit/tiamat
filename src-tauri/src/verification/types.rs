use serde::{Deserialize, Serialize};
use tiamat_contracts::{EvidenceClassification, EvidenceRecord, TestKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryTestHint {
    pub kind: TestKind,
    pub command: Vec<String>,
    pub source: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestRunOutcome {
    pub evidence: EvidenceRecord,
    pub passed_expected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LayerGateSummary {
    pub kind: TestKind,
    pub required: bool,
    pub executed: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub inapplicable: bool,
    pub inapplicable_reason: Option<String>,
}

impl LayerGateSummary {
    pub fn all_required_passed(&self) -> bool {
        if self.inapplicable || !self.required {
            return true;
        }
        self.failed == 0 && self.executed > 0
    }
}

pub fn classification_blocks_pass(c: &EvidenceClassification) -> bool {
    matches!(
        c,
        EvidenceClassification::Fail
            | EvidenceClassification::FlakyFail
            | EvidenceClassification::PolicyDenied
    )
}
