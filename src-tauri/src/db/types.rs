use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tiamat_contracts::{EventEnvelope, EventLevel};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub run_id: Uuid,
    pub status: String,
    pub title: String,
    pub created_at_utc: String,
    pub updated_at_utc: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRecord {
    pub artifact_id: String,
    pub content_hash: String,
    pub byte_size: u64,
    pub media_type: Option<String>,
    pub relative_path: Option<String>,
    pub created_at_utc: String,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct NewEvent {
    pub event_id: Uuid,
    pub run_id: Uuid,
    pub project_id: Option<String>,
    pub phase_id: Option<String>,
    pub attempt_id: Option<Uuid>,
    pub process_id: Option<Uuid>,
    pub event_type: String,
    pub level: EventLevel,
    pub timestamp_utc: DateTime<Utc>,
    pub message: String,
    pub payload: Value,
}

impl NewEvent {
    pub fn into_envelope(self, sequence: u64) -> EventEnvelope {
        EventEnvelope {
            schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
            event_id: self.event_id,
            sequence,
            run_id: self.run_id,
            project_id: self.project_id,
            phase_id: self.phase_id,
            attempt_id: self.attempt_id,
            process_id: self.process_id,
            r#type: self.event_type,
            level: self.level,
            timestamp_utc: self.timestamp_utc.to_rfc3339(),
            message: self.message,
            payload: self.payload,
        }
    }
}

pub fn level_to_str(level: &EventLevel) -> &'static str {
    match level {
        EventLevel::Debug => "debug",
        EventLevel::Info => "info",
        EventLevel::Warning => "warning",
        EventLevel::Error => "error",
    }
}

pub fn level_from_str(value: &str) -> EventLevel {
    match value {
        "debug" => EventLevel::Debug,
        "warning" => EventLevel::Warning,
        "error" => EventLevel::Error,
        _ => EventLevel::Info,
    }
}
