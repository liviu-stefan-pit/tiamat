//! Transactional side-effect ledger with stable idempotency keys.

use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::db::{DbResult, Store};
use crate::recovery::error::{RecoveryError, RecoveryResult};
use crate::recovery::fault::{self, FaultAction, FaultPoint};
use crate::recovery::types::{SideEffectKind, SideEffectRecord, SideEffectState};

impl Store {
    pub fn upsert_side_effect(&self, record: &SideEffectRecord) -> DbResult<()> {
        self.conn().execute(
            "INSERT INTO side_effects (
                idempotency_key, run_id, kind, state, external_fact_json,
                created_at_utc, updated_at_utc, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(idempotency_key) DO UPDATE SET
                state = excluded.state,
                external_fact_json = excluded.external_fact_json,
                updated_at_utc = excluded.updated_at_utc,
                metadata_json = excluded.metadata_json",
            rusqlite::params![
                record.idempotency_key,
                record.run_id.to_string(),
                record.kind.as_str(),
                record.state.as_str(),
                serde_json::to_string(&record.external_fact)?,
                record.created_at_utc,
                record.updated_at_utc,
                serde_json::to_string(&record.metadata)?,
            ],
        )?;
        Ok(())
    }

    pub fn get_side_effect(&self, key: &str) -> DbResult<Option<SideEffectRecord>> {
        self.conn()
            .query_row(
                "SELECT idempotency_key, run_id, kind, state, external_fact_json,
                        created_at_utc, updated_at_utc, metadata_json
                 FROM side_effects WHERE idempotency_key = ?1",
                rusqlite::params![key],
                map_side_effect,
            )
            .optional()
            .map_err(crate::db::DbError::from)
    }

    pub fn list_side_effects(&self, run_id: Uuid) -> DbResult<Vec<SideEffectRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT idempotency_key, run_id, kind, state, external_fact_json,
                    created_at_utc, updated_at_utc, metadata_json
             FROM side_effects WHERE run_id = ?1
             ORDER BY created_at_utc ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![run_id.to_string()], map_side_effect)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_unreconciled_side_effects(
        &self,
        run_id: Option<Uuid>,
    ) -> DbResult<Vec<SideEffectRecord>> {
        let mut out = Vec::new();
        if let Some(run_id) = run_id {
            for rec in self.list_side_effects(run_id)? {
                if rec.state.needs_reconcile() {
                    out.push(rec);
                }
            }
        } else {
            let mut stmt = self.conn().prepare(
                "SELECT idempotency_key, run_id, kind, state, external_fact_json,
                        created_at_utc, updated_at_utc, metadata_json
                 FROM side_effects WHERE state != 'reconciled'
                 ORDER BY created_at_utc ASC",
            )?;
            let rows = stmt.query_map([], map_side_effect)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }
}

fn map_side_effect(row: &rusqlite::Row<'_>) -> rusqlite::Result<SideEffectRecord> {
    let run_raw: String = row.get(1)?;
    let kind_raw: String = row.get(2)?;
    let state_raw: String = row.get(3)?;
    let fact_json: String = row.get(4)?;
    let meta_json: String = row.get(7)?;
    Ok(SideEffectRecord {
        idempotency_key: row.get(0)?,
        run_id: Uuid::parse_str(&run_raw).unwrap_or_else(|_| Uuid::nil()),
        kind: SideEffectKind::parse(&kind_raw).unwrap_or(SideEffectKind::DbCommit),
        state: SideEffectState::parse(&state_raw).unwrap_or(SideEffectState::Prepared),
        external_fact: serde_json::from_str(&fact_json).unwrap_or(Value::Null),
        created_at_utc: row.get(5)?,
        updated_at_utc: row.get(6)?,
        metadata: serde_json::from_str(&meta_json).unwrap_or(Value::Null),
    })
}

/// Build a stable idempotency key for a side effect.
pub fn make_idempotency_key(kind: SideEffectKind, run_id: Uuid, scope: &str) -> String {
    format!("{}:{}:{}", kind.as_str(), run_id, scope)
}

/// Execute a side effect under the prepared→executing→observed→reconciled protocol.
///
/// If a prior record exists in `reconciled` state with the same key, the operation is
/// skipped and the existing record is returned (idempotent no-op).
pub fn execute_idempotent<T, F>(
    store: &Store,
    run_id: Uuid,
    kind: SideEffectKind,
    scope: &str,
    metadata: Value,
    mut op: F,
) -> RecoveryResult<(SideEffectRecord, Option<T>)>
where
    F: FnMut() -> RecoveryResult<T>,
{
    let key = make_idempotency_key(kind, run_id, scope);
    if let Some(existing) = store.get_side_effect(&key)? {
        if existing.state == SideEffectState::Reconciled {
            return Ok((existing, None));
        }
        // Incomplete prior attempt — continue from prepared/executing and reconcile.
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut record = SideEffectRecord {
        idempotency_key: key.clone(),
        run_id,
        kind,
        state: SideEffectState::Prepared,
        external_fact: json!({}),
        created_at_utc: now.clone(),
        updated_at_utc: now.clone(),
        metadata: metadata.clone(),
    };
    store.upsert_side_effect(&record)?;

    // Before-fault
    match fault::check_fault(FaultPoint::for_kind_before(kind))? {
        Some(FaultAction::Skip) => {
            return Ok((record, None));
        }
        Some(FaultAction::Crash) => {
            // check_fault returns Err for Crash; this arm is unreachable but keeps match exhaustive.
            return Err(RecoveryError::FaultInjected(format!(
                "fault injected at {}",
                FaultPoint::for_kind_before(kind).as_str()
            )));
        }
        None => {}
    }

    record.state = SideEffectState::Executing;
    record.updated_at_utc = chrono::Utc::now().to_rfc3339();
    store.upsert_side_effect(&record)?;

    let result = op()?;

    record.state = SideEffectState::Observed;
    record.external_fact = json!({ "observed": true });
    record.updated_at_utc = chrono::Utc::now().to_rfc3339();
    store.upsert_side_effect(&record)?;

    if let Some(after) = FaultPoint::for_kind_after(kind) {
        match fault::check_fault(after)? {
            Some(FaultAction::Skip) => {
                // Leave as observed — startup reconcile must finish.
                return Ok((record, Some(result)));
            }
            Some(FaultAction::Crash) => {
                return Err(RecoveryError::FaultInjected(format!(
                    "fault injected at {}",
                    after.as_str()
                )));
            }
            None => {}
        }
    }

    record.state = SideEffectState::Reconciled;
    record.updated_at_utc = chrono::Utc::now().to_rfc3339();
    store.upsert_side_effect(&record)?;
    Ok((record, Some(result)))
}

/// Reconcile a non-terminal side effect by inspecting an external fact predicate.
/// When `external_succeeded` is true, mark reconciled without re-running the op.
/// When false and state is prepared/executing, leave for retry after user Resume.
pub fn reconcile_side_effect(
    store: &Store,
    key: &str,
    external_succeeded: bool,
    fact: Value,
) -> RecoveryResult<SideEffectRecord> {
    let mut record = store
        .get_side_effect(key)?
        .ok_or_else(|| RecoveryError::Validation(format!("unknown side effect key {key}")))?;
    if record.state == SideEffectState::Reconciled {
        return Ok(record);
    }
    record.external_fact = fact;
    record.updated_at_utc = chrono::Utc::now().to_rfc3339();
    if external_succeeded {
        record.state = SideEffectState::Reconciled;
    } else if record.state == SideEffectState::Executing
        || record.state == SideEffectState::Observed
    {
        // Collapse to prepared so Resume can safely retry.
        record.state = SideEffectState::Prepared;
    }
    store.upsert_side_effect(&record)?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn idempotent_execute_does_not_duplicate() {
        fault::clear_faults();
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("a")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "idem", "created").unwrap();

        let mut calls = 0u32;
        let (r1, v1) = execute_idempotent(
            &store,
            run_id,
            SideEffectKind::PlanWrite,
            "P01",
            json!({}),
            || {
                calls += 1;
                Ok(42u32)
            },
        )
        .unwrap();
        assert_eq!(v1, Some(42));
        assert_eq!(r1.state, SideEffectState::Reconciled);

        let (r2, v2) = execute_idempotent(
            &store,
            run_id,
            SideEffectKind::PlanWrite,
            "P01",
            json!({}),
            || {
                calls += 1;
                Ok(99u32)
            },
        )
        .unwrap();
        assert!(v2.is_none());
        assert_eq!(r2.state, SideEffectState::Reconciled);
        assert_eq!(calls, 1);
        fault::clear_faults();
    }

    #[test]
    fn crash_before_leaves_prepared_or_no_reconcile() {
        fault::clear_faults();
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("a")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "fault", "created").unwrap();

        fault::set_fault(fault::FaultRule {
            point: FaultPoint::BeforeDbCommit,
            action: FaultAction::Crash,
            once: true,
        });

        let err = execute_idempotent(
            &store,
            run_id,
            SideEffectKind::DbCommit,
            "tx-1",
            json!({}),
            || Ok(()),
        );
        assert!(err.is_err());

        let key = make_idempotency_key(SideEffectKind::DbCommit, run_id, "tx-1");
        let rec = store.get_side_effect(&key).unwrap().unwrap();
        assert_eq!(rec.state, SideEffectState::Prepared);
        fault::clear_faults();
    }
}
