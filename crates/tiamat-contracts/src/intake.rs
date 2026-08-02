use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntakeSource {
    pub path: String,
    pub kind: SourceKind,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    File,
    Folder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Git,
    Folder,
    Notes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub project_id: String,
    pub root: String,
    pub kind: ProjectKind,
    pub languages: Vec<String>,
    pub build_systems: Vec<String>,
    pub test_commands: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntakeManifest {
    pub schema_version: u32,
    pub intake_id: Uuid,
    pub sources: Vec<IntakeSource>,
    pub projects: Vec<ProjectSummary>,
    pub inventory_artifact: String,
}

impl IntakeManifest {
    pub fn validate_schema_version(&self) -> Result<(), crate::validation::ValidationError> {
        if self.schema_version != crate::CURRENT_SCHEMA_VERSION {
            return Err(
                crate::validation::ValidationError::IncompatibleSchemaVersion {
                    expected: crate::CURRENT_SCHEMA_VERSION,
                    found: self.schema_version,
                },
            );
        }
        Ok(())
    }
}
