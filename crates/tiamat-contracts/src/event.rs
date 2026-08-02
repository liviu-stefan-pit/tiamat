use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel {
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub event_id: Uuid,
    pub sequence: u64,
    pub run_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<Uuid>,
    pub r#type: String,
    pub level: EventLevel,
    pub timestamp_utc: String,
    pub message: String,
    pub payload: Value,
}

impl EventEnvelope {
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
