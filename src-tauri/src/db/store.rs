use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tiamat_contracts::{EventEnvelope, EventLevel};
use uuid::Uuid;

use crate::db::error::{DbError, DbResult};
use crate::db::migrations::{self, latest_migration_version};
use crate::db::types::{level_from_str, level_to_str, ArtifactRecord, NewEvent, RunRecord};

const BUSY_TIMEOUT_MS: i64 = 5_000;

pub struct Store {
    conn: Connection,
    artifact_root: PathBuf,
}

impl Store {
    pub fn open(db_path: impl AsRef<Path>, artifact_root: impl AsRef<Path>) -> DbResult<Self> {
        let db_path = db_path.as_ref();
        let artifact_root = artifact_root.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&artifact_root)?;

        let conn = Connection::open(db_path)?;
        configure_connection(&conn)?;
        migrations::migrate(&conn)?;
        integrity_check(&conn)?;

        Ok(Self {
            conn,
            artifact_root,
        })
    }

    pub fn open_in_memory(artifact_root: impl AsRef<Path>) -> DbResult<Self> {
        let artifact_root = artifact_root.as_ref().to_path_buf();
        fs::create_dir_all(&artifact_root)?;
        let conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;
        migrations::migrate(&conn)?;
        Ok(Self {
            conn,
            artifact_root,
        })
    }

    pub fn schema_version(&self) -> DbResult<i64> {
        migrations::current_version(&self.conn)
    }

    pub fn create_run(&self, run_id: Uuid, title: &str, status: &str) -> DbResult<RunRecord> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO runs (run_id, status, title, created_at_utc, updated_at_utc, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, '{}')",
            params![run_id.to_string(), status, title, now, now],
        )?;
        self.get_run(run_id)?
            .ok_or_else(|| DbError::RunNotFound(run_id.to_string()))
    }

    pub fn get_run(&self, run_id: Uuid) -> DbResult<Option<RunRecord>> {
        self.conn
            .query_row(
                "SELECT run_id, status, title, created_at_utc, updated_at_utc, metadata_json
                 FROM runs WHERE run_id = ?1",
                params![run_id.to_string()],
                |row| {
                    let metadata_json: String = row.get(5)?;
                    Ok(RunRecord {
                        run_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or(run_id),
                        status: row.get(1)?,
                        title: row.get(2)?,
                        created_at_utc: row.get(3)?,
                        updated_at_utc: row.get(4)?,
                        metadata: serde_json::from_str(&metadata_json).unwrap_or(Value::Null),
                    })
                },
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn list_runs(&self) -> DbResult<Vec<RunRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT run_id, status, title, created_at_utc, updated_at_utc, metadata_json
             FROM runs ORDER BY created_at_utc ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let run_id_raw: String = row.get(0)?;
            let metadata_json: String = row.get(5)?;
            Ok(RunRecord {
                run_id: Uuid::parse_str(&run_id_raw).unwrap_or_else(|_| Uuid::nil()),
                status: row.get(1)?,
                title: row.get(2)?,
                created_at_utc: row.get(3)?,
                updated_at_utc: row.get(4)?,
                metadata: serde_json::from_str(&metadata_json).unwrap_or(Value::Null),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Atomically update run status (optional) and append the next monotonic event.
    pub fn append_event_atomic(
        &self,
        new_status: Option<&str>,
        event: NewEvent,
    ) -> DbResult<EventEnvelope> {
        let tx = self.conn.unchecked_transaction()?;
        let run_id = event.run_id;
        let run_id_str = run_id.to_string();

        let current_status: String = tx
            .query_row(
                "SELECT status FROM runs WHERE run_id = ?1",
                params![run_id_str],
                |row| row.get(0),
            )
            .map_err(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => DbError::RunNotFound(run_id_str.clone()),
                other => DbError::from(other),
            })?;

        if let Some(status) = new_status {
            let now = chrono::Utc::now().to_rfc3339();
            tx.execute(
                "UPDATE runs SET status = ?1, updated_at_utc = ?2 WHERE run_id = ?3",
                params![status, now, run_id_str],
            )?;
            let _ = current_status;
        }

        let next_sequence: u64 = tx.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM events WHERE run_id = ?1",
            params![run_id_str],
            |row| row.get::<_, i64>(0),
        )? as u64;

        let envelope = event.into_envelope(next_sequence);
        let payload_json = serde_json::to_string(&envelope.payload)?;

        let insert = tx.execute(
            "INSERT INTO events (
                event_id, run_id, sequence, project_id, phase_id, attempt_id, process_id,
                type, level, timestamp_utc, message, payload_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                envelope.event_id.to_string(),
                run_id_str,
                envelope.sequence as i64,
                envelope.project_id,
                envelope.phase_id,
                envelope.attempt_id.map(|id| id.to_string()),
                envelope.process_id.map(|id| id.to_string()),
                envelope.r#type,
                level_to_str(&envelope.level),
                envelope.timestamp_utc,
                envelope.message,
                payload_json,
            ],
        );

        if let Err(rusqlite::Error::SqliteFailure(info, _)) = &insert {
            if info.code == rusqlite::ErrorCode::ConstraintViolation {
                return Err(DbError::DuplicateEvent(envelope.event_id.to_string()));
            }
        }
        insert?;

        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO event_outbox (event_id, run_id, sequence, created_at_utc, delivered)
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![
                envelope.event_id.to_string(),
                run_id_str,
                envelope.sequence as i64,
                now
            ],
        )?;

        tx.commit()?;
        Ok(envelope)
    }

    /// Bulk-insert persisted fake events for performance fixtures (single transaction).
    pub fn bulk_seed_events(
        &self,
        run_id: Uuid,
        count: u64,
        type_prefix: &str,
    ) -> DbResult<Vec<EventEnvelope>> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let tx = self.conn.unchecked_transaction()?;
        let run_id_str = run_id.to_string();
        let _exists: String = tx
            .query_row(
                "SELECT status FROM runs WHERE run_id = ?1",
                params![run_id_str],
                |row| row.get(0),
            )
            .map_err(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => DbError::RunNotFound(run_id_str.clone()),
                other => DbError::from(other),
            })?;

        let start_sequence: u64 = tx.query_row(
            "SELECT COALESCE(MAX(sequence), 0) FROM events WHERE run_id = ?1",
            params![run_id_str],
            |row| row.get::<_, i64>(0),
        )? as u64;

        let mut envelopes = Vec::with_capacity(count as usize);
        let base = chrono::Utc::now();
        for i in 0..count {
            let sequence = start_sequence + i + 1;
            let event_id = Uuid::new_v4();
            let phase_id = format!("P{:02}", (i % 8) + 1);
            let event_type = format!("{type_prefix}.{}", (i % 5) + 1);
            let level = match i % 4 {
                0 => EventLevel::Debug,
                1 => EventLevel::Info,
                2 => EventLevel::Warning,
                _ => EventLevel::Error,
            };
            let timestamp = (base + chrono::Duration::milliseconds(i as i64)).to_rfc3339();
            let message = format!("perf event {sequence} phase={phase_id}");
            let payload = serde_json::json!({
                "perf": true,
                "index": sequence,
                "seeded": true,
            });
            let payload_json = serde_json::to_string(&payload)?;
            tx.execute(
                "INSERT INTO events (
                    event_id, run_id, sequence, project_id, phase_id, attempt_id, process_id,
                    type, level, timestamp_utc, message, payload_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    event_id.to_string(),
                    run_id_str,
                    sequence as i64,
                    "tiamat",
                    phase_id,
                    Option::<String>::None,
                    Option::<String>::None,
                    event_type,
                    level_to_str(&level),
                    timestamp,
                    message,
                    payload_json,
                ],
            )?;
            let now = chrono::Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO event_outbox (event_id, run_id, sequence, created_at_utc, delivered)
                 VALUES (?1, ?2, ?3, ?4, 1)",
                params![event_id.to_string(), run_id_str, sequence as i64, now],
            )?;
            envelopes.push(EventEnvelope {
                schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
                event_id,
                sequence,
                run_id,
                project_id: Some("tiamat".into()),
                phase_id: Some(phase_id),
                attempt_id: None,
                process_id: None,
                r#type: event_type,
                level,
                timestamp_utc: timestamp,
                message,
                payload,
            });
        }
        tx.commit()?;
        Ok(envelopes)
    }

    pub fn event_count(&self, run_id: Uuid) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE run_id = ?1",
            params![run_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn replay_events(&self, run_id: Uuid, after_sequence: u64) -> DbResult<Vec<EventEnvelope>> {
        let mut stmt = self.conn.prepare(
            "SELECT event_id, run_id, sequence, project_id, phase_id, attempt_id, process_id,
                    type, level, timestamp_utc, message, payload_json
             FROM events
             WHERE run_id = ?1 AND sequence > ?2
             ORDER BY sequence ASC",
        )?;
        let rows = stmt.query_map(
            params![run_id.to_string(), after_sequence as i64],
            map_event,
        )?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn mark_outbox_delivered(&self, event_ids: &[Uuid]) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        for event_id in event_ids {
            tx.execute(
                "UPDATE event_outbox SET delivered = 1 WHERE event_id = ?1",
                params![event_id.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn undelivered_outbox(&self, run_id: Option<Uuid>) -> DbResult<Vec<EventEnvelope>> {
        let mut sql = String::from(
            "SELECT e.event_id, e.run_id, e.sequence, e.project_id, e.phase_id, e.attempt_id,
                    e.process_id, e.type, e.level, e.timestamp_utc, e.message, e.payload_json
             FROM event_outbox o
             JOIN events e ON e.event_id = o.event_id
             WHERE o.delivered = 0",
        );
        if run_id.is_some() {
            sql.push_str(" AND o.run_id = ?1");
        }
        sql.push_str(" ORDER BY o.outbox_id ASC");

        let mut stmt = self.conn.prepare(&sql)?;
        let mapped = if let Some(run_id) = run_id {
            stmt.query_map(params![run_id.to_string()], map_event)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], map_event)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(mapped)
    }

    pub fn put_artifact(
        &self,
        bytes: &[u8],
        media_type: Option<&str>,
        relative_path: Option<&str>,
        metadata: Value,
    ) -> DbResult<ArtifactRecord> {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let hash = hex::encode(hasher.finalize());
        let artifact_id = hash.clone();
        let dest = self.artifact_root.join(&hash);

        if !dest.exists() {
            let tmp = self.artifact_root.join(format!("{hash}.tmp"));
            fs::write(&tmp, bytes)?;
            fs::rename(&tmp, &dest)?;
        }

        let now = chrono::Utc::now().to_rfc3339();
        let metadata_json = serde_json::to_string(&metadata)?;
        self.conn.execute(
            "INSERT INTO artifacts (
                artifact_id, content_hash, byte_size, media_type, relative_path,
                created_at_utc, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(artifact_id) DO NOTHING",
            params![
                artifact_id,
                hash,
                bytes.len() as i64,
                media_type,
                relative_path,
                now,
                metadata_json
            ],
        )?;

        self.get_artifact(&artifact_id)?.ok_or_else(|| {
            DbError::Integrity(format!("artifact missing after insert: {artifact_id}"))
        })
    }

    pub fn get_artifact(&self, artifact_id: &str) -> DbResult<Option<ArtifactRecord>> {
        self.conn
            .query_row(
                "SELECT artifact_id, content_hash, byte_size, media_type, relative_path,
                        created_at_utc, metadata_json
                 FROM artifacts WHERE artifact_id = ?1",
                params![artifact_id],
                map_artifact,
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn list_artifacts(&self) -> DbResult<Vec<ArtifactRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT artifact_id, content_hash, byte_size, media_type, relative_path,
                    created_at_utc, metadata_json
             FROM artifacts ORDER BY created_at_utc ASC",
        )?;
        let rows = stmt.query_map([], map_artifact)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn artifact_root(&self) -> &Path {
        &self.artifact_root
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

fn configure_connection(conn: &Connection) -> DbResult<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)?;
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") && !mode.eq_ignore_ascii_case("memory") {
        // in-memory connections may report "memory"; file DBs must be WAL
        if mode != "memory" {
            return Err(DbError::Integrity(format!(
                "expected WAL journal mode, got {mode}"
            )));
        }
    }
    Ok(())
}

fn integrity_check(conn: &Connection) -> DbResult<()> {
    let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(DbError::Integrity(result));
    }
    let version = migrations::current_version(conn)?;
    if version != latest_migration_version() {
        return Err(DbError::Migration(format!(
            "schema version {version} != {}",
            latest_migration_version()
        )));
    }
    Ok(())
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventEnvelope> {
    let event_id = Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::nil());
    let run_id = Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::nil());
    let attempt_id = row
        .get::<_, Option<String>>(5)?
        .and_then(|v| Uuid::parse_str(&v).ok());
    let process_id = row
        .get::<_, Option<String>>(6)?
        .and_then(|v| Uuid::parse_str(&v).ok());
    let payload_json: String = row.get(11)?;
    let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Null);
    let level_raw: String = row.get(8)?;

    Ok(EventEnvelope {
        schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
        event_id,
        sequence: row.get::<_, i64>(2)? as u64,
        run_id,
        project_id: row.get(3)?,
        phase_id: row.get(4)?,
        attempt_id,
        process_id,
        r#type: row.get(7)?,
        level: level_from_str(&level_raw),
        timestamp_utc: row.get(9)?,
        message: row.get(10)?,
        payload,
    })
}

fn map_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactRecord> {
    let metadata_json: String = row.get(6)?;
    Ok(ArtifactRecord {
        artifact_id: row.get(0)?,
        content_hash: row.get(1)?,
        byte_size: row.get::<_, i64>(2)? as u64,
        media_type: row.get(3)?,
        relative_path: row.get(4)?,
        created_at_utc: row.get(5)?,
        metadata: serde_json::from_str(&metadata_json).unwrap_or(Value::Null),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;
    use tiamat_contracts::EventLevel;

    fn sample_event(run_id: Uuid, message: &str) -> NewEvent {
        NewEvent {
            event_id: Uuid::new_v4(),
            run_id,
            project_id: Some("demo".into()),
            phase_id: Some("P01".into()),
            attempt_id: None,
            process_id: None,
            event_type: "phase.started".into(),
            level: EventLevel::Info,
            timestamp_utc: chrono::Utc::now(),
            message: message.into(),
            payload: json!({}),
        }
    }

    #[test]
    fn wal_mode_and_monotonic_sequences() {
        let dir = tempdir().unwrap();
        let store =
            Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
        assert_eq!(store.schema_version().unwrap(), latest_migration_version());

        let run_id = Uuid::new_v4();
        store.create_run(run_id, "Demo", "created").unwrap();

        let e1 = store
            .append_event_atomic(Some("executing"), sample_event(run_id, "one"))
            .unwrap();
        let e2 = store
            .append_event_atomic(None, sample_event(run_id, "two"))
            .unwrap();
        assert_eq!(e1.sequence, 1);
        assert_eq!(e2.sequence, 2);

        let replay = store.replay_events(run_id, 0).unwrap();
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].sequence, 1);
        assert_eq!(replay[1].sequence, 2);
        assert_eq!(store.get_run(run_id).unwrap().unwrap().status, "executing");
    }

    #[test]
    fn state_and_event_commit_atomically() {
        let dir = tempdir().unwrap();
        let store =
            Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "Atomic", "created").unwrap();

        let event = sample_event(run_id, "transition");
        let event_id = event.event_id;
        store.append_event_atomic(Some("planning"), event).unwrap();

        // Re-inserting the same event id must fail without changing status again.
        let duplicate = NewEvent {
            event_id,
            ..sample_event(run_id, "dup")
        };
        let err = store
            .append_event_atomic(Some("executing"), duplicate)
            .unwrap_err();
        assert!(matches!(err, DbError::DuplicateEvent(_)));
        assert_eq!(store.get_run(run_id).unwrap().unwrap().status, "planning");
        assert_eq!(store.replay_events(run_id, 0).unwrap().len(), 1);
    }

    #[test]
    fn restart_replay_has_no_duplicates() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("tiamat.db");
        let artifacts = dir.path().join("artifacts");
        let run_id = Uuid::new_v4();

        {
            let store = Store::open(&db_path, &artifacts).unwrap();
            store.create_run(run_id, "Replay", "created").unwrap();
            store
                .append_event_atomic(Some("executing"), sample_event(run_id, "a"))
                .unwrap();
            store
                .append_event_atomic(None, sample_event(run_id, "b"))
                .unwrap();
            store
                .append_event_atomic(None, sample_event(run_id, "c"))
                .unwrap();
        }

        let store = Store::open(&db_path, &artifacts).unwrap();
        let events = store.replay_events(run_id, 0).unwrap();
        assert_eq!(events.len(), 3);
        let sequences: Vec<_> = events.iter().map(|e| e.sequence).collect();
        assert_eq!(sequences, vec![1, 2, 3]);
        let ids: Vec<_> = events.iter().map(|e| e.event_id).collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn artifact_metadata_is_content_addressed() {
        let dir = tempdir().unwrap();
        let store =
            Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
        let bytes = b"hello-artifact";
        let a1 = store
            .put_artifact(
                bytes,
                Some("text/plain"),
                Some("hello.txt"),
                json!({"k": 1}),
            )
            .unwrap();
        let a2 = store
            .put_artifact(
                bytes,
                Some("text/plain"),
                Some("hello.txt"),
                json!({"k": 1}),
            )
            .unwrap();
        assert_eq!(a1.artifact_id, a2.artifact_id);
        assert_eq!(a1.content_hash, a2.content_hash);
        assert_eq!(a1.byte_size, bytes.len() as u64);
        assert!(store.artifact_root().join(&a1.content_hash).exists());
        assert_eq!(store.list_artifacts().unwrap().len(), 1);
    }
}
