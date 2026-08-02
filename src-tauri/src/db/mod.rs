//! SQLite WAL store, migrations, event outbox, and artifact metadata.

mod app_settings_store;
mod error;
mod migrations;
mod process_store;
mod scheduler_store;
mod store;
mod types;

pub use error::{DbError, DbResult};
pub use migrations::{current_version, latest_migration_version, migrate, migrate_up_to};
pub use store::{is_sqlite_busy, with_busy_retry, Store};
pub use types::{ArtifactRecord, NewEvent, RunRecord};

use serde_json::json;
use tiamat_contracts::EventLevel;
use uuid::Uuid;

/// Seed a deterministic fake run used by the P01 desktop shell and tests.
pub fn ensure_demo_run(
    store: &Store,
) -> DbResult<(RunRecord, Vec<tiamat_contracts::EventEnvelope>)> {
    if let Some(existing) = store.list_runs()?.into_iter().next() {
        let events = store.replay_events(existing.run_id, 0)?;
        return Ok((existing, events));
    }

    let run_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").expect("demo run id");
    let _run = store.create_run(run_id, "P01 demo run", "created")?;

    let specs = [
        ("run.created", "created", "Run created", None::<&str>),
        (
            "intake.placeholder",
            "preflighting",
            "Intake placeholder ready",
            None,
        ),
        ("phase.queued", "executing", "Phase P01 queued", Some("P01")),
        (
            "phase.started",
            "executing",
            "Phase P01 started",
            Some("P01"),
        ),
        (
            "test.unit.passed",
            "executing",
            "Unit evidence recorded",
            Some("P01"),
        ),
        (
            "system.info",
            "executing",
            "Structured logger connected",
            None,
        ),
    ];

    let mut events = Vec::new();
    for (idx, (event_type, status, message, phase_id)) in specs.into_iter().enumerate() {
        let event = NewEvent {
            event_id: Uuid::parse_str(&format!("22222222-2222-4222-8222-{:012}", idx + 1))
                .expect("demo event id"),
            run_id,
            project_id: Some("tiamat".into()),
            phase_id: phase_id.map(str::to_string),
            attempt_id: None,
            process_id: None,
            event_type: event_type.into(),
            level: EventLevel::Info,
            timestamp_utc: chrono::DateTime::parse_from_rfc3339("2026-08-02T09:00:00Z")
                .expect("timestamp")
                .with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(idx as i64),
            message: message.into(),
            payload: json!({ "demo": true, "index": idx + 1 }),
        };
        events.push(store.append_event_atomic(Some(status), event)?);
    }

    let _ = store.put_artifact(
        b"p01-demo-artifact",
        Some("text/plain"),
        Some("demo/p01.txt"),
        json!({ "kind": "demo" }),
    )?;

    let run = store
        .get_run(run_id)?
        .ok_or_else(|| DbError::RunNotFound(run_id.to_string()))?;
    Ok((run, events))
}
