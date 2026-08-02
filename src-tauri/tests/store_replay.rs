//! SQLite restart/replay integration tests for P01.

use std::fs;

use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;
use tiamat_contracts::EventLevel;
use tiamat_lib::db::{
    ensure_demo_run, latest_migration_version, migrate, migrate_up_to, NewEvent, Store,
};
use uuid::Uuid;

#[test]
fn empty_and_prior_version_migrations() {
    let empty = Connection::open_in_memory().unwrap();
    let version = migrate(&empty).unwrap();
    assert_eq!(version, latest_migration_version());

    let latest = latest_migration_version();
    // Parameterized: every prior version 1..latest must upgrade cleanly.
    for prior in 1..=latest {
        let dir = tempdir().unwrap();
        let prior_path = dir.path().join(format!("prior-v{prior}.db"));
        {
            let prior_conn = Connection::open(&prior_path).unwrap();
            let stopped = migrate_up_to(&prior_conn, prior).unwrap();
            assert_eq!(stopped, prior);
            prior_conn
                .execute(
                    "INSERT INTO runs (run_id, status, title, created_at_utc, updated_at_utc, metadata_json)
                     VALUES (?1, 'created', 'Prior', '2026-08-02T00:00:00Z', '2026-08-02T00:00:00Z', '{}')",
                    [format!("aaaaaaaa-aaaa-4aaa-8aaa-{:012}", prior)],
                )
                .unwrap();
        }

        let artifacts = dir.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        let db_path = dir.path().join("tiamat.db");
        fs::copy(&prior_path, &db_path).unwrap();
        let store = Store::open(&db_path, &artifacts).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            latest,
            "upgrade from v{prior}"
        );
        assert_eq!(store.list_runs().unwrap().len(), 1);
    }
}

#[test]
fn restart_preserves_monotonic_replay_without_duplicates() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("tiamat.db");
    let artifacts = dir.path().join("artifacts");
    let run_id = Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap();

    {
        let store = Store::open(&db_path, &artifacts).unwrap();
        store.create_run(run_id, "Integration", "created").unwrap();
        for i in 0..5 {
            let event = NewEvent {
                event_id: Uuid::new_v4(),
                run_id,
                project_id: Some("proj".into()),
                phase_id: Some("P01".into()),
                attempt_id: None,
                process_id: None,
                event_type: "run.tick".into(),
                level: EventLevel::Info,
                timestamp_utc: chrono::Utc::now(),
                message: format!("tick-{i}"),
                payload: json!({ "i": i }),
            };
            let status = if i == 0 { Some("executing") } else { None };
            store.append_event_atomic(status, event).unwrap();
        }
    }

    let store = Store::open(&db_path, &artifacts).unwrap();
    let first = store.replay_events(run_id, 0).unwrap();
    let second = store.replay_events(run_id, 0).unwrap();
    assert_eq!(first.len(), 5);
    assert_eq!(second.len(), 5);
    assert_eq!(
        first.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        second.iter().map(|e| e.sequence).collect::<Vec<_>>()
    );
    assert_eq!(
        first.iter().map(|e| e.event_id).collect::<Vec<_>>(),
        second.iter().map(|e| e.event_id).collect::<Vec<_>>()
    );
    assert_eq!(
        first.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );

    let after_two = store.replay_events(run_id, 2).unwrap();
    assert_eq!(after_two.len(), 3);
    assert_eq!(after_two[0].sequence, 3);
}

#[test]
fn atomic_state_transition_with_event() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "Atomic", "created").unwrap();

    let event = NewEvent {
        event_id: Uuid::new_v4(),
        run_id,
        project_id: None,
        phase_id: None,
        attempt_id: None,
        process_id: None,
        event_type: "run.status_changed".into(),
        level: EventLevel::Info,
        timestamp_utc: chrono::Utc::now(),
        message: "to planning".into(),
        payload: json!({}),
    };
    let envelope = store.append_event_atomic(Some("planning"), event).unwrap();
    assert_eq!(envelope.sequence, 1);
    assert_eq!(store.get_run(run_id).unwrap().unwrap().status, "planning");
    assert_eq!(store.replay_events(run_id, 0).unwrap().len(), 1);
}

#[test]
fn demo_run_survives_store_reopen() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("tiamat.db");
    let artifacts = dir.path().join("artifacts");

    let (run, events) = {
        let store = Store::open(&db_path, &artifacts).unwrap();
        ensure_demo_run(&store).unwrap()
    };

    let store = Store::open(&db_path, &artifacts).unwrap();
    let replayed = store.replay_events(run.run_id, 0).unwrap();
    assert_eq!(replayed.len(), events.len());
    assert!(!store.list_artifacts().unwrap().is_empty());
}

#[test]
fn scheduler_lease_renew_same_holder_or_rejects_foreign() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "lease-integ", "created").unwrap();

    let first = store
        .renew_scheduler_lease(run_id, "holder-a", 3, Some(false))
        .unwrap();
    assert_eq!(first.epoch, 1);
    assert_eq!(first.lease_holder, "holder-a");

    let renewed = store
        .renew_scheduler_lease(run_id, "holder-a", 3, None)
        .expect("same holder renew");
    assert_eq!(renewed.lease_holder, "holder-a");
    assert_eq!(renewed.epoch, 2);

    let foreign = store.renew_scheduler_lease(run_id, "holder-b", 3, None);
    assert!(foreign.is_err(), "foreign unexpired lease must be rejected");
    let msg = foreign.err().unwrap().to_string();
    assert!(
        msg.contains("foreign unexpired") || msg.contains("held by"),
        "unexpected: {msg}"
    );
    let still = store.get_scheduler_lease(run_id).unwrap().unwrap();
    assert_eq!(still.lease_holder, "holder-a");
    assert_eq!(still.epoch, 2);
}
