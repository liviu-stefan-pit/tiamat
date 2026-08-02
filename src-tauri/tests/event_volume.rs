//! P09: large event seed + ordered replay integration.

use tempfile::tempdir;
use tiamat_lib::db::{ensure_demo_run, Store};
use uuid::Uuid;

#[test]
fn seed_100k_events_replay_is_monotonic_and_complete() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
    let (run, demo_events) = ensure_demo_run(&store).unwrap();
    assert!(!demo_events.is_empty());

    let seeded = store
        .bulk_seed_events(run.run_id, 100_000, "perf.seed")
        .unwrap();
    assert_eq!(seeded.len(), 100_000);
    assert_eq!(
        store.event_count(run.run_id).unwrap(),
        demo_events.len() as u64 + 100_000
    );

    let replay = store.replay_events(run.run_id, 0).unwrap();
    assert_eq!(replay.len(), demo_events.len() + 100_000);
    for window in replay.windows(2) {
        assert!(window[0].sequence < window[1].sequence);
        assert_ne!(window[0].event_id, window[1].event_id);
    }

    let after = store
        .replay_events(run.run_id, replay[replay.len() / 2].sequence)
        .unwrap();
    assert!(!after.is_empty());
    assert_eq!(after[0].sequence, replay[replay.len() / 2].sequence + 1);
}

#[test]
fn burst_seed_is_persisted_before_emit_contract() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "burst", "created").unwrap();
    let burst = store.bulk_seed_events(run_id, 1_000, "perf.burst").unwrap();
    assert_eq!(burst.len(), 1_000);
    assert_eq!(store.event_count(run_id).unwrap(), 1_000);
    let replay = store.replay_events(run_id, 0).unwrap();
    assert_eq!(replay.len(), 1_000);
    assert!(replay.iter().all(|e| e.r#type.starts_with("perf.burst.")));
}
