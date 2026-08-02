//! Security and policy audit event helpers.

use serde_json::{json, Value};
use uuid::Uuid;

use crate::db::{NewEvent, Store};
use crate::security::redaction::redact_line;
use tiamat_contracts::EventLevel;

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub run_id: Uuid,
    pub phase_id: Option<String>,
    pub attempt_id: Option<Uuid>,
    pub event_type: String,
    pub level: EventLevel,
    pub message: String,
    pub payload: Value,
}

pub fn emit_security_event(store: &Store, event: AuditEvent) -> crate::db::DbResult<()> {
    let _ = store.append_event_atomic(
        None,
        NewEvent {
            event_id: Uuid::new_v4(),
            run_id: event.run_id,
            project_id: None,
            phase_id: event.phase_id,
            attempt_id: event.attempt_id,
            process_id: None,
            event_type: event.event_type,
            level: event.level,
            timestamp_utc: chrono::Utc::now(),
            message: redact_line(&event.message),
            payload: event.payload,
        },
    )?;
    Ok(())
}

pub fn emit_policy_event(
    store: &Store,
    run_id: Uuid,
    phase_id: Option<&str>,
    denied: bool,
    reason: &str,
    command: &[String],
) -> crate::db::DbResult<()> {
    let event_type = if denied {
        "policy.denied"
    } else {
        "policy.allowed"
    };
    emit_security_event(
        store,
        AuditEvent {
            run_id,
            phase_id: phase_id.map(str::to_string),
            attempt_id: None,
            event_type: event_type.into(),
            level: if denied {
                EventLevel::Warning
            } else {
                EventLevel::Info
            },
            message: redact_line(reason),
            payload: json!({
                "denied": denied,
                "reason": redact_line(reason),
                "command": command.iter().map(|c| redact_line(c)).collect::<Vec<_>>(),
            }),
        },
    )
}
