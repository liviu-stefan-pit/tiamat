use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::db::{DbError, DbResult, Store};
use crate::scheduler::{
    model_tier_str, parse_model_tier, AttemptRecord, AttemptStatus, AttemptTerminalResult,
    FailureKind, PhaseRecord, PhaseRuntimeStatus, SchedulerLease, LEASE_TTL_SECS,
};

impl Store {
    pub fn upsert_phase(&self, phase: &PhaseRecord) -> DbResult<()> {
        self.conn().execute(
            "INSERT INTO phases (
                run_id, phase_id, title, status, project_ids_json, write_roots_json,
                resource_locks_json, dependencies_json, model_tier, estimated_minutes,
                critical_path_length, ready_at_utc, queued_at_utc, attempt_count,
                last_failure_kind, metadata_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,'{}')
             ON CONFLICT(run_id, phase_id) DO UPDATE SET
                title=excluded.title,
                status=excluded.status,
                project_ids_json=excluded.project_ids_json,
                write_roots_json=excluded.write_roots_json,
                resource_locks_json=excluded.resource_locks_json,
                dependencies_json=excluded.dependencies_json,
                model_tier=excluded.model_tier,
                estimated_minutes=excluded.estimated_minutes,
                critical_path_length=excluded.critical_path_length,
                ready_at_utc=excluded.ready_at_utc,
                queued_at_utc=excluded.queued_at_utc,
                attempt_count=excluded.attempt_count,
                last_failure_kind=excluded.last_failure_kind",
            params![
                phase.run_id.to_string(),
                phase.phase_id,
                phase.title,
                phase.status.as_str(),
                serde_json::to_string(&phase.project_ids)?,
                serde_json::to_string(&phase.write_roots)?,
                serde_json::to_string(&phase.resource_locks)?,
                serde_json::to_string(&phase.dependencies)?,
                model_tier_str(&phase.model_tier),
                phase.estimated_minutes as i64,
                phase.critical_path_length as i64,
                phase.ready_at_utc,
                phase.queued_at_utc,
                phase.attempt_count as i64,
                phase.last_failure_kind,
            ],
        )?;
        Ok(())
    }

    pub fn list_phases(&self, run_id: Uuid) -> DbResult<Vec<PhaseRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT run_id, phase_id, title, status, project_ids_json, write_roots_json,
                    resource_locks_json, dependencies_json, model_tier, estimated_minutes,
                    critical_path_length, ready_at_utc, queued_at_utc, attempt_count,
                    last_failure_kind
             FROM phases WHERE run_id = ?1 ORDER BY phase_id ASC",
        )?;
        let rows = stmt.query_map(params![run_id.to_string()], map_phase)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_phase(&self, run_id: Uuid, phase_id: &str) -> DbResult<Option<PhaseRecord>> {
        self.conn()
            .query_row(
                "SELECT run_id, phase_id, title, status, project_ids_json, write_roots_json,
                        resource_locks_json, dependencies_json, model_tier, estimated_minutes,
                        critical_path_length, ready_at_utc, queued_at_utc, attempt_count,
                        last_failure_kind
                 FROM phases WHERE run_id = ?1 AND phase_id = ?2",
                params![run_id.to_string(), phase_id],
                map_phase,
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn update_phase_status(
        &self,
        run_id: Uuid,
        phase_id: &str,
        status: PhaseRuntimeStatus,
        ready_at_utc: Option<&str>,
        queued_at_utc: Option<&str>,
        last_failure_kind: Option<&str>,
    ) -> DbResult<()> {
        self.conn().execute(
            "UPDATE phases SET status = ?1,
                ready_at_utc = COALESCE(?2, ready_at_utc),
                queued_at_utc = COALESCE(?3, queued_at_utc),
                last_failure_kind = COALESCE(?4, last_failure_kind)
             WHERE run_id = ?5 AND phase_id = ?6",
            params![
                status.as_str(),
                ready_at_utc,
                queued_at_utc,
                last_failure_kind,
                run_id.to_string(),
                phase_id
            ],
        )?;
        Ok(())
    }

    pub fn insert_attempt(&self, attempt: &AttemptRecord) -> DbResult<()> {
        let result = self.conn().execute(
            "INSERT INTO attempts (
                attempt_id, run_id, phase_id, attempt_number, status, terminal_result,
                requested_tier, requested_model, selected_model, selection_reason,
                availability_json, resume_parent_attempt_id, progress_useful, failure_kind,
                started_at_utc, finished_at_utc, metadata_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,'{}')",
            params![
                attempt.attempt_id.to_string(),
                attempt.run_id.to_string(),
                attempt.phase_id,
                attempt.attempt_number as i64,
                attempt.status.as_str(),
                attempt.terminal_result.map(|r| r.as_str()),
                model_tier_str(&attempt.requested_tier),
                attempt.requested_model,
                attempt.selected_model,
                attempt.selection_reason,
                serde_json::to_string(&attempt.availability)?,
                attempt.resume_parent_attempt_id.map(|id| id.to_string()),
                attempt.progress_useful as i64,
                attempt.failure_kind.map(|k| k.as_str()),
                attempt.started_at_utc,
                attempt.finished_at_utc,
            ],
        );
        match result {
            Ok(_) => {
                self.conn().execute(
                    "UPDATE phases SET attempt_count = MAX(attempt_count, ?1)
                     WHERE run_id = ?2 AND phase_id = ?3",
                    params![
                        attempt.attempt_number as i64,
                        attempt.run_id.to_string(),
                        attempt.phase_id
                    ],
                )?;
                Ok(())
            }
            Err(rusqlite::Error::SqliteFailure(info, _))
                if info.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(DbError::Integrity(format!(
                    "duplicate or concurrent active attempt for {}/{}#{}",
                    attempt.run_id, attempt.phase_id, attempt.attempt_number
                )))
            }
            Err(err) => Err(DbError::from(err)),
        }
    }

    pub fn update_attempt(&self, attempt: &AttemptRecord) -> DbResult<()> {
        self.conn().execute(
            "UPDATE attempts SET
                status = ?1,
                terminal_result = ?2,
                progress_useful = ?3,
                failure_kind = ?4,
                started_at_utc = ?5,
                finished_at_utc = ?6,
                selected_model = ?7,
                selection_reason = ?8
             WHERE attempt_id = ?9",
            params![
                attempt.status.as_str(),
                attempt.terminal_result.map(|r| r.as_str()),
                attempt.progress_useful as i64,
                attempt.failure_kind.map(|k| k.as_str()),
                attempt.started_at_utc,
                attempt.finished_at_utc,
                attempt.selected_model,
                attempt.selection_reason,
                attempt.attempt_id.to_string(),
            ],
        )?;
        Ok(())
    }

    pub fn list_attempts(&self, run_id: Uuid) -> DbResult<Vec<AttemptRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT attempt_id, run_id, phase_id, attempt_number, status, terminal_result,
                    requested_tier, requested_model, selected_model, selection_reason,
                    availability_json, resume_parent_attempt_id, progress_useful, failure_kind,
                    started_at_utc, finished_at_utc
             FROM attempts WHERE run_id = ?1
             ORDER BY phase_id ASC, attempt_number ASC",
        )?;
        let rows = stmt.query_map(params![run_id.to_string()], map_attempt)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_attempts_for_phase(
        &self,
        run_id: Uuid,
        phase_id: &str,
    ) -> DbResult<Vec<AttemptRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT attempt_id, run_id, phase_id, attempt_number, status, terminal_result,
                    requested_tier, requested_model, selected_model, selection_reason,
                    availability_json, resume_parent_attempt_id, progress_useful, failure_kind,
                    started_at_utc, finished_at_utc
             FROM attempts WHERE run_id = ?1 AND phase_id = ?2
             ORDER BY attempt_number ASC",
        )?;
        let rows = stmt.query_map(params![run_id.to_string(), phase_id], map_attempt)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn active_attempt_count(&self, run_id: Uuid) -> DbResult<u32> {
        let count: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM attempts
             WHERE run_id = ?1 AND status IN ('starting','running','stopping')",
            params![run_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    /// Renew (or take) the scheduler lease when the caller is the current holder
    /// or the existing lease is expired. Foreign unexpired leases are rejected.
    pub fn renew_scheduler_lease(
        &self,
        run_id: Uuid,
        lease_holder: &str,
        max_concurrent: u32,
        paused: Option<bool>,
    ) -> DbResult<SchedulerLease> {
        let now = chrono::Utc::now();
        let expires = now + chrono::Duration::seconds(LEASE_TTL_SECS);
        let now_s = now.to_rfc3339();
        let expires_s = expires.to_rfc3339();

        let existing = self.get_scheduler_lease(run_id)?;
        if let Some(ref lease) = existing {
            if !lease_may_be_taken_by(lease, lease_holder, now) {
                return Err(DbError::Integrity(format!(
                    "scheduler lease held by '{}' until {} (foreign unexpired)",
                    lease.lease_holder, lease.expires_at_utc
                )));
            }
        }

        let epoch = existing.as_ref().map(|l| l.epoch + 1).unwrap_or(1);
        let paused_val =
            paused.unwrap_or_else(|| existing.as_ref().map(|l| l.paused).unwrap_or(false));
        let cleanup = existing
            .as_ref()
            .map(|l| l.cleanup_incomplete)
            .unwrap_or(false);
        let low_disk = existing.as_ref().map(|l| l.low_disk).unwrap_or(false);

        // Conditional upsert: same holder or expired only (race-safe with the check above).
        let changed = self.conn().execute(
            "INSERT INTO scheduler_leases (
                run_id, lease_holder, epoch, renewed_at_utc, expires_at_utc,
                paused, max_concurrent, cleanup_incomplete, low_disk, metadata_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'{}')
             ON CONFLICT(run_id) DO UPDATE SET
                lease_holder=excluded.lease_holder,
                epoch=excluded.epoch,
                renewed_at_utc=excluded.renewed_at_utc,
                expires_at_utc=excluded.expires_at_utc,
                paused=excluded.paused,
                max_concurrent=excluded.max_concurrent
             WHERE scheduler_leases.lease_holder = excluded.lease_holder
                OR scheduler_leases.expires_at_utc <= excluded.renewed_at_utc",
            params![
                run_id.to_string(),
                lease_holder,
                epoch as i64,
                now_s,
                expires_s,
                paused_val as i64,
                max_concurrent as i64,
                cleanup as i64,
                low_disk as i64,
            ],
        )?;

        if changed == 0 {
            let current = self.get_scheduler_lease(run_id)?;
            return Err(DbError::Integrity(format!(
                "scheduler lease renew rejected (foreign unexpired): {:?}",
                current.map(|l| (l.lease_holder, l.expires_at_utc))
            )));
        }

        self.get_scheduler_lease(run_id)?
            .ok_or_else(|| DbError::Integrity("lease missing after renew".into()))
    }

    pub fn get_scheduler_lease(&self, run_id: Uuid) -> DbResult<Option<SchedulerLease>> {
        self.conn()
            .query_row(
                "SELECT run_id, lease_holder, epoch, renewed_at_utc, expires_at_utc,
                        paused, max_concurrent, cleanup_incomplete, low_disk
                 FROM scheduler_leases WHERE run_id = ?1",
                params![run_id.to_string()],
                |row| {
                    Ok(SchedulerLease {
                        run_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or(run_id),
                        lease_holder: row.get(1)?,
                        epoch: row.get::<_, i64>(2)? as u64,
                        renewed_at_utc: row.get(3)?,
                        expires_at_utc: row.get(4)?,
                        paused: row.get::<_, i64>(5)? != 0,
                        max_concurrent: row.get::<_, i64>(6)? as u32,
                        cleanup_incomplete: row.get::<_, i64>(7)? != 0,
                        low_disk: row.get::<_, i64>(8)? != 0,
                    })
                },
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn set_scheduler_paused(&self, run_id: Uuid, paused: bool) -> DbResult<SchedulerLease> {
        let lease = self
            .get_scheduler_lease(run_id)?
            .ok_or_else(|| DbError::RunNotFound(run_id.to_string()))?;
        self.renew_scheduler_lease(
            run_id,
            &lease.lease_holder,
            lease.max_concurrent,
            Some(paused),
        )
    }

    pub fn set_scheduler_flags(
        &self,
        run_id: Uuid,
        cleanup_incomplete: Option<bool>,
        low_disk: Option<bool>,
    ) -> DbResult<()> {
        let lease = self
            .get_scheduler_lease(run_id)?
            .ok_or_else(|| DbError::RunNotFound(run_id.to_string()))?;
        let cleanup = cleanup_incomplete.unwrap_or(lease.cleanup_incomplete);
        let low = low_disk.unwrap_or(lease.low_disk);
        self.conn().execute(
            "UPDATE scheduler_leases SET cleanup_incomplete = ?1, low_disk = ?2 WHERE run_id = ?3",
            params![cleanup as i64, low as i64, run_id.to_string()],
        )?;
        Ok(())
    }

    /// Acquire locks in caller-provided sorted order. All-or-nothing.
    pub fn acquire_locks(
        &self,
        run_id: Uuid,
        phase_id: &str,
        attempt_id: Uuid,
        lock_names: &[String],
    ) -> DbResult<()> {
        let tx = self.conn().unchecked_transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        for name in lock_names {
            let insert = tx.execute(
                "INSERT INTO resource_locks (lock_name, run_id, phase_id, attempt_id, acquired_at_utc)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    name,
                    run_id.to_string(),
                    phase_id,
                    attempt_id.to_string(),
                    now
                ],
            );
            if let Err(rusqlite::Error::SqliteFailure(info, _)) = &insert {
                if info.code == rusqlite::ErrorCode::ConstraintViolation {
                    return Err(DbError::Integrity(format!(
                        "lock busy: {name} (phase {phase_id})"
                    )));
                }
            }
            insert?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn release_locks_for_attempt(&self, attempt_id: Uuid) -> DbResult<()> {
        self.conn().execute(
            "DELETE FROM resource_locks WHERE attempt_id = ?1",
            params![attempt_id.to_string()],
        )?;
        Ok(())
    }

    pub fn list_held_locks(&self, run_id: Uuid) -> DbResult<Vec<String>> {
        let mut stmt = self.conn().prepare(
            "SELECT lock_name FROM resource_locks WHERE run_id = ?1 ORDER BY lock_name ASC",
        )?;
        let rows = stmt.query_map(params![run_id.to_string()], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

/// True when `caller` may renew/take `lease` (same holder or expired).
fn lease_may_be_taken_by(
    lease: &SchedulerLease,
    caller: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    if lease.lease_holder == caller {
        return true;
    }
    match chrono::DateTime::parse_from_rfc3339(&lease.expires_at_utc) {
        Ok(expires_at) => expires_at.with_timezone(&chrono::Utc) <= now,
        // Fail closed on unparseable expiry: treat as still held.
        Err(_) => false,
    }
}

fn map_phase(row: &rusqlite::Row<'_>) -> rusqlite::Result<PhaseRecord> {
    let run_raw: String = row.get(0)?;
    let project_ids: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default();
    let write_roots: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
    let resource_locks: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default();
    let dependencies: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default();
    Ok(PhaseRecord {
        run_id: Uuid::parse_str(&run_raw).unwrap_or_else(|_| Uuid::nil()),
        phase_id: row.get(1)?,
        title: row.get(2)?,
        status: PhaseRuntimeStatus::parse(&row.get::<_, String>(3)?),
        project_ids,
        write_roots,
        resource_locks,
        dependencies,
        model_tier: parse_model_tier(&row.get::<_, String>(8)?),
        estimated_minutes: row.get::<_, i64>(9)? as u32,
        critical_path_length: row.get::<_, i64>(10)? as u32,
        ready_at_utc: row.get(11)?,
        queued_at_utc: row.get(12)?,
        attempt_count: row.get::<_, i64>(13)? as u32,
        last_failure_kind: row.get(14)?,
    })
}

fn map_attempt(row: &rusqlite::Row<'_>) -> rusqlite::Result<AttemptRecord> {
    let attempt_id = Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::nil());
    let run_id = Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::nil());
    let terminal = row
        .get::<_, Option<String>>(5)?
        .and_then(|v| AttemptTerminalResult::parse(&v));
    let availability: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default();
    let parent = row
        .get::<_, Option<String>>(11)?
        .and_then(|v| Uuid::parse_str(&v).ok());
    let failure = row
        .get::<_, Option<String>>(13)?
        .map(|v| FailureKind::parse(&v));
    Ok(AttemptRecord {
        attempt_id,
        run_id,
        phase_id: row.get(2)?,
        attempt_number: row.get::<_, i64>(3)? as u32,
        status: AttemptStatus::parse(&row.get::<_, String>(4)?),
        terminal_result: terminal,
        requested_tier: parse_model_tier(&row.get::<_, String>(6)?),
        requested_model: row.get(7)?,
        selected_model: row.get(8)?,
        selection_reason: row.get(9)?,
        availability,
        resume_parent_attempt_id: parent,
        progress_useful: row.get::<_, i64>(12)? != 0,
        failure_kind: failure,
        started_at_utc: row.get(14)?,
        finished_at_utc: row.get(15)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::migrate;
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().unwrap();
        let store =
            Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
        (dir, store)
    }

    #[test]
    fn renew_allows_same_holder() {
        let (_dir, store) = open_store();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "lease", "created").unwrap();

        let first = store
            .renew_scheduler_lease(run_id, "holder-a", 3, Some(false))
            .unwrap();
        assert_eq!(first.lease_holder, "holder-a");
        assert_eq!(first.epoch, 1);

        let second = store
            .renew_scheduler_lease(run_id, "holder-a", 3, None)
            .unwrap();
        assert_eq!(second.lease_holder, "holder-a");
        assert_eq!(second.epoch, 2);
    }

    #[test]
    fn renew_rejects_foreign_unexpired_holder() {
        let (_dir, store) = open_store();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "lease", "created").unwrap();

        store
            .renew_scheduler_lease(run_id, "holder-a", 3, Some(false))
            .unwrap();

        let err = store
            .renew_scheduler_lease(run_id, "holder-b", 3, None)
            .expect_err("foreign unexpired must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("foreign unexpired") || msg.contains("held by"),
            "unexpected: {msg}"
        );

        let still = store.get_scheduler_lease(run_id).unwrap().unwrap();
        assert_eq!(still.lease_holder, "holder-a");
        assert_eq!(still.epoch, 1);
    }

    #[test]
    fn renew_allows_takeover_when_expired() {
        let (_dir, store) = open_store();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "lease", "created").unwrap();

        store
            .renew_scheduler_lease(run_id, "holder-a", 3, Some(false))
            .unwrap();

        // Force expiry in the past.
        let past = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc3339();
        store
            .conn()
            .execute(
                "UPDATE scheduler_leases SET expires_at_utc = ?1 WHERE run_id = ?2",
                params![past, run_id.to_string()],
            )
            .unwrap();

        let taken = store
            .renew_scheduler_lease(run_id, "holder-b", 2, Some(false))
            .unwrap();
        assert_eq!(taken.lease_holder, "holder-b");
        assert_eq!(taken.epoch, 2);
        assert_eq!(taken.max_concurrent, 2);
    }

    #[test]
    fn lease_may_be_taken_helper() {
        let now = chrono::Utc::now();
        let lease = SchedulerLease {
            run_id: Uuid::nil(),
            lease_holder: "a".into(),
            epoch: 1,
            renewed_at_utc: now.to_rfc3339(),
            expires_at_utc: (now + chrono::Duration::seconds(30)).to_rfc3339(),
            paused: false,
            max_concurrent: 3,
            cleanup_incomplete: false,
            low_disk: false,
        };
        assert!(lease_may_be_taken_by(&lease, "a", now));
        assert!(!lease_may_be_taken_by(&lease, "b", now));

        let mut expired = lease.clone();
        expired.expires_at_utc = (now - chrono::Duration::seconds(1)).to_rfc3339();
        assert!(lease_may_be_taken_by(&expired, "b", now));
    }

    #[test]
    fn migrate_then_lease_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='scheduler_leases'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
