//! Full startup recovery: DB → processes → attempts → side effects → offer.

use serde_json::json;
use uuid::Uuid;

use crate::db::{NewEvent, Store};
use crate::process::{self, ReconcileReport};
use crate::recovery::disk::probe_disk_default;
use crate::recovery::error::RecoveryResult;
use crate::recovery::ledger::reconcile_side_effect;
use crate::recovery::types::{
    InterruptedAttemptSummary, RecoveryOffer, RecoveryOfferStatus, RecoveryScanReport,
    RetentionSettings,
};
use crate::scheduler::{AttemptStatus, AttemptTerminalResult, FailureKind, PhaseRuntimeStatus};
use tiamat_contracts::EventLevel;

impl Store {
    pub fn upsert_recovery_offer(&self, offer: &RecoveryOffer) -> crate::db::DbResult<()> {
        self.conn().execute(
            "INSERT INTO recovery_offers (
                run_id, offer_id, status, reason, db_integrity_ok, process_hard_failure,
                interrupted_attempt_count, unreconciled_side_effects, low_disk,
                corrupt_db_backup_path, details_json, created_at_utc, resolved_at_utc, resolution
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(run_id) DO UPDATE SET
                offer_id = excluded.offer_id,
                status = excluded.status,
                reason = excluded.reason,
                db_integrity_ok = excluded.db_integrity_ok,
                process_hard_failure = excluded.process_hard_failure,
                interrupted_attempt_count = excluded.interrupted_attempt_count,
                unreconciled_side_effects = excluded.unreconciled_side_effects,
                low_disk = excluded.low_disk,
                corrupt_db_backup_path = excluded.corrupt_db_backup_path,
                details_json = excluded.details_json,
                created_at_utc = excluded.created_at_utc,
                resolved_at_utc = excluded.resolved_at_utc,
                resolution = excluded.resolution",
            rusqlite::params![
                offer.run_id.to_string(),
                offer.offer_id,
                offer.status.as_str(),
                offer.reason,
                offer.db_integrity_ok as i64,
                offer.process_hard_failure as i64,
                offer.interrupted_attempt_count as i64,
                offer.unreconciled_side_effects as i64,
                offer.low_disk as i64,
                offer.corrupt_db_backup_path,
                serde_json::to_string(&offer.details)?,
                offer.created_at_utc,
                offer.resolved_at_utc,
                offer.resolution,
            ],
        )?;
        Ok(())
    }

    pub fn get_recovery_offer(&self, run_id: Uuid) -> crate::db::DbResult<Option<RecoveryOffer>> {
        use rusqlite::OptionalExtension;
        self.conn()
            .query_row(
                "SELECT run_id, offer_id, status, reason, db_integrity_ok, process_hard_failure,
                        interrupted_attempt_count, unreconciled_side_effects, low_disk,
                        corrupt_db_backup_path, details_json, created_at_utc, resolved_at_utc,
                        resolution
                 FROM recovery_offers WHERE run_id = ?1",
                rusqlite::params![run_id.to_string()],
                map_offer,
            )
            .optional()
            .map_err(crate::db::DbError::from)
    }

    pub fn list_pending_recovery_offers(&self) -> crate::db::DbResult<Vec<RecoveryOffer>> {
        let mut stmt = self.conn().prepare(
            "SELECT run_id, offer_id, status, reason, db_integrity_ok, process_hard_failure,
                    interrupted_attempt_count, unreconciled_side_effects, low_disk,
                    corrupt_db_backup_path, details_json, created_at_utc, resolved_at_utc,
                    resolution
             FROM recovery_offers WHERE status = 'pending' OR status = 'blocked'
             ORDER BY created_at_utc ASC",
        )?;
        let rows = stmt.query_map([], map_offer)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_retention_settings(&self) -> crate::db::DbResult<RetentionSettings> {
        self.conn()
            .query_row(
                "SELECT retain_run_metadata_days, retain_redacted_logs_days,
                    retain_unpromoted_workspaces, allow_destructive_cleanup, updated_at_utc
             FROM retention_settings WHERE id = 1",
                [],
                |row| {
                    Ok(RetentionSettings {
                        retain_run_metadata_days: row.get::<_, i64>(0)? as u32,
                        retain_redacted_logs_days: row.get::<_, i64>(1)? as u32,
                        retain_unpromoted_workspaces: row.get::<_, i64>(2)? != 0,
                        allow_destructive_cleanup: row.get::<_, i64>(3)? != 0,
                        updated_at_utc: row.get(4)?,
                    })
                },
            )
            .map_err(crate::db::DbError::from)
    }

    pub fn save_retention_settings(&self, settings: &RetentionSettings) -> crate::db::DbResult<()> {
        self.conn().execute(
            "UPDATE retention_settings SET
                retain_run_metadata_days = ?1,
                retain_redacted_logs_days = ?2,
                retain_unpromoted_workspaces = ?3,
                allow_destructive_cleanup = ?4,
                updated_at_utc = ?5
             WHERE id = 1",
            rusqlite::params![
                settings.retain_run_metadata_days as i64,
                settings.retain_redacted_logs_days as i64,
                settings.retain_unpromoted_workspaces as i64,
                settings.allow_destructive_cleanup as i64,
                settings.updated_at_utc,
            ],
        )?;
        Ok(())
    }
}

fn map_offer(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecoveryOffer> {
    let run_raw: String = row.get(0)?;
    let status_raw: String = row.get(2)?;
    let details_json: String = row.get(10)?;
    let db_ok = row.get::<_, i64>(4)? != 0;
    let hard = row.get::<_, i64>(5)? != 0;
    let low_disk = row.get::<_, i64>(8)? != 0;
    let status = RecoveryOfferStatus::parse(&status_raw);
    let resume_allowed = db_ok && !hard && matches!(status, RecoveryOfferStatus::Pending);
    Ok(RecoveryOffer {
        run_id: Uuid::parse_str(&run_raw).unwrap_or_else(|_| Uuid::nil()),
        offer_id: row.get(1)?,
        status,
        reason: row.get(3)?,
        db_integrity_ok: db_ok,
        process_hard_failure: hard,
        interrupted_attempt_count: row.get::<_, i64>(6)? as u32,
        unreconciled_side_effects: row.get::<_, i64>(7)? as u32,
        low_disk,
        corrupt_db_backup_path: row.get(9)?,
        details: serde_json::from_str(&details_json).unwrap_or(json!({})),
        created_at_utc: row.get(11)?,
        resolved_at_utc: row.get(12)?,
        resolution: row.get(13)?,
        requires_user_choice: matches!(
            status,
            RecoveryOfferStatus::Pending | RecoveryOfferStatus::Blocked
        ),
        resume_allowed,
    })
}

/// Run the full §16 startup recovery pipeline against an open store.
pub fn run_startup_recovery(
    store: &Store,
    disk_probe_path: Option<&std::path::Path>,
) -> RecoveryResult<RecoveryScanReport> {
    let mut messages = Vec::new();
    let scanned_at = chrono::Utc::now().to_rfc3339();

    // 1. DB integrity (already open — re-verify).
    let db_integrity_ok = match crate::recovery::integrity::verify_store_integrity(store) {
        Ok(()) => {
            messages.push("DB integrity check passed".into());
            true
        }
        Err(e) => {
            messages.push(format!("DB integrity failed: {e}"));
            false
        }
    };
    let schema_version_ok = store
        .schema_version()
        .map(|v| v == crate::db::latest_migration_version())
        .unwrap_or(false);

    // 2. Process reconcile.
    let process_reconcile = match process::reconcile_owned_processes(store) {
        Ok(r) => {
            messages.extend(r.messages.iter().cloned());
            Some(r)
        }
        Err(e) => {
            messages.push(format!("process reconcile error: {e}"));
            None
        }
    };
    let process_hard_failure = process_reconcile
        .as_ref()
        .map(|r| r.hard_failure)
        .unwrap_or(true);

    // 3. Mark lost active attempts interrupted.
    let interrupted = interrupt_active_attempts(store)?;
    messages.push(format!(
        "marked {} active attempt(s) interrupted",
        interrupted.len()
    ));

    // 4. Reconcile incomplete side effects from external facts (best-effort; do not re-run).
    let mut unreconciled = store.list_unreconciled_side_effects(None)?;
    for effect in &unreconciled {
        // Without external proof of success, collapse executing/observed → prepared for Resume.
        let _ = reconcile_side_effect(
            store,
            &effect.idempotency_key,
            false,
            json!({ "startupReconcile": true, "priorState": effect.state.as_str() }),
        )?;
    }
    unreconciled = store.list_unreconciled_side_effects(None)?;

    // 5. Disk pressure.
    let disk_path = disk_probe_path.map(|p| p.to_path_buf()).or_else(|| {
        store
            .artifact_root()
            .parent()
            .map(|p| p.to_path_buf())
            .or_else(|| Some(store.artifact_root().to_path_buf()))
    });
    let disk_report = disk_path
        .as_ref()
        .map(|p| probe_disk_default(p))
        .unwrap_or_else(|| crate::recovery::types::DiskPressureReport {
            path: String::new(),
            free_bytes: None,
            low_disk: false,
            threshold_bytes: crate::recovery::types::DEFAULT_LOW_DISK_THRESHOLD_BYTES,
            message: "no disk path".into(),
        });
    if disk_report.low_disk {
        messages.push(disk_report.message.clone());
        // Mark leases low_disk for all non-terminal runs.
        for run in store.list_runs()? {
            if matches!(
                run.status.as_str(),
                "completed" | "failed" | "cancelled" | "created"
            ) {
                continue;
            }
            ensure_lease(store, run.run_id);
            let _ = store.set_scheduler_flags(run.run_id, None, Some(true));
        }
    }

    // 6. Build offer — default no new execution until user chooses.
    let needs_offer = !interrupted.is_empty()
        || !unreconciled.is_empty()
        || process_hard_failure
        || !db_integrity_ok
        || disk_report.low_disk
        || has_recoverable_run(store)?;

    let offer = if needs_offer {
        let run_id = select_offer_run_id(store, &interrupted)?;
        // Transition recoverable runs to interrupted and pause scheduling.
        if let Some(run) = store.get_run(run_id)? {
            if !matches!(
                run.status.as_str(),
                "completed" | "failed" | "cancelled" | "interrupted"
            ) {
                let _ = store.append_event_atomic(
                    Some("interrupted"),
                    NewEvent {
                        event_id: Uuid::new_v4(),
                        run_id,
                        project_id: None,
                        phase_id: None,
                        attempt_id: None,
                        process_id: None,
                        event_type: "recovery.offer_created".into(),
                        level: EventLevel::Warning,
                        timestamp_utc: chrono::Utc::now(),
                        message: "Startup recovery requires Resume or Cancel before new work"
                            .into(),
                        payload: json!({
                            "interruptedAttempts": interrupted.len(),
                            "unreconciledSideEffects": unreconciled.len(),
                            "processHardFailure": process_hard_failure,
                            "lowDisk": disk_report.low_disk,
                        }),
                    },
                );
            }
            ensure_lease(store, run_id);
            let _ = store.set_scheduler_flags(run_id, Some(true), None);
            let _ = store.set_scheduler_paused(run_id, true);
        }

        let resume_allowed = db_integrity_ok && !process_hard_failure;
        let status = if resume_allowed {
            RecoveryOfferStatus::Pending
        } else {
            RecoveryOfferStatus::Blocked
        };
        let reason = if !db_integrity_ok {
            "database integrity failure — preserved corrupt copy; Cancel only".into()
        } else if process_hard_failure {
            "process cleanup hard failure — cannot resume until cleanup is verifiable".into()
        } else if disk_report.low_disk {
            "low disk pressure — Resume will remain paused until space is available".into()
        } else if !interrupted.is_empty() || !unreconciled.is_empty() {
            "interrupted run detected — choose Resume or Cancel".into()
        } else {
            "recoverable run state requires explicit user choice".into()
        };

        let offer = RecoveryOffer {
            offer_id: Uuid::new_v4().to_string(),
            run_id,
            status,
            reason,
            db_integrity_ok,
            process_hard_failure,
            interrupted_attempt_count: interrupted.len() as u32,
            unreconciled_side_effects: unreconciled.len() as u32,
            low_disk: disk_report.low_disk,
            corrupt_db_backup_path: None,
            details: json!({
                "messages": messages,
                "schemaVersionOk": schema_version_ok,
            }),
            created_at_utc: scanned_at.clone(),
            resolved_at_utc: None,
            resolution: None,
            requires_user_choice: true,
            resume_allowed,
        };
        store.upsert_recovery_offer(&offer)?;
        Some(offer)
    } else {
        None
    };

    Ok(RecoveryScanReport {
        schema_version: 1,
        scanned_at_utc: scanned_at,
        db_integrity_ok,
        schema_version_ok,
        process_reconcile,
        interrupted_attempts: interrupted,
        unreconciled_side_effects: unreconciled,
        low_disk: disk_report.low_disk,
        free_disk_bytes: disk_report.free_bytes,
        disk_path: disk_path.map(|p| p.display().to_string()),
        offer,
        messages,
    })
}

fn has_recoverable_run(store: &Store) -> RecoveryResult<bool> {
    Ok(store.list_runs()?.into_iter().any(|r| {
        matches!(
            r.status.as_str(),
            "executing"
                | "planning"
                | "reviewing"
                | "paused"
                | "interrupted"
                | "preflighting"
                | "awaiting_confirmation"
        )
    }))
}

fn select_offer_run_id(
    store: &Store,
    interrupted: &[InterruptedAttemptSummary],
) -> RecoveryResult<Uuid> {
    if let Some(first) = interrupted.first() {
        return Ok(first.run_id);
    }
    if let Some(run) = store.list_runs()?.into_iter().rev().find(|r| {
        !matches!(
            r.status.as_str(),
            "completed" | "failed" | "cancelled" | "created"
        )
    }) {
        return Ok(run.run_id);
    }
    if let Some(run) = store.list_runs()?.into_iter().next() {
        return Ok(run.run_id);
    }
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "Recovery placeholder", "interrupted")?;
    Ok(run_id)
}

fn interrupt_active_attempts(store: &Store) -> RecoveryResult<Vec<InterruptedAttemptSummary>> {
    let mut out = Vec::new();
    for run in store.list_runs()? {
        let attempts = store.list_attempts(run.run_id)?;
        for mut attempt in attempts {
            if !attempt.status.is_active() {
                continue;
            }
            let prior = attempt.status.as_str().to_string();
            attempt.status = AttemptStatus::Completed;
            attempt.terminal_result = Some(AttemptTerminalResult::Lost);
            attempt.failure_kind = Some(FailureKind::Interrupted);
            attempt.finished_at_utc = Some(chrono::Utc::now().to_rfc3339());
            store.update_attempt(&attempt)?;

            // Return phase to ready/failed-interrupted so readiness can rebuild.
            if let Some(mut phase) = store.get_phase(run.run_id, &attempt.phase_id)? {
                if matches!(
                    phase.status,
                    PhaseRuntimeStatus::Running
                        | PhaseRuntimeStatus::Queued
                        | PhaseRuntimeStatus::Verifying
                ) {
                    phase.status = PhaseRuntimeStatus::Ready;
                    phase.last_failure_kind = Some(FailureKind::Interrupted.as_str().into());
                    store.upsert_phase(&phase)?;
                }
            }

            let _ = store.release_locks_for_attempt(attempt.attempt_id);

            out.push(InterruptedAttemptSummary {
                attempt_id: attempt.attempt_id,
                run_id: run.run_id,
                phase_id: attempt.phase_id.clone(),
                prior_status: prior,
                terminal_result: AttemptTerminalResult::Lost.as_str().into(),
            });

            let _ = store.append_event_atomic(
                None,
                NewEvent {
                    event_id: Uuid::new_v4(),
                    run_id: run.run_id,
                    project_id: None,
                    phase_id: Some(attempt.phase_id.clone()),
                    attempt_id: Some(attempt.attempt_id),
                    process_id: None,
                    event_type: "recovery.attempt_interrupted".into(),
                    level: EventLevel::Warning,
                    timestamp_utc: chrono::Utc::now(),
                    message: format!(
                        "Active attempt for {} marked interrupted/lost at startup",
                        attempt.phase_id
                    ),
                    payload: json!({
                        "attemptId": attempt.attempt_id,
                        "priorStatus": out.last().map(|s| s.prior_status.clone()),
                    }),
                },
            );
        }
    }
    Ok(out)
}

/// User chose Resume — clear offer, unpause scheduling (unless low disk / blocked).
pub fn resolve_resume(store: &Store, run_id: Uuid) -> RecoveryResult<RecoveryOffer> {
    let mut offer = store.get_recovery_offer(run_id)?.ok_or_else(|| {
        crate::recovery::error::RecoveryError::Blocked("no recovery offer".into())
    })?;
    if !offer.resume_allowed {
        return Err(crate::recovery::error::RecoveryError::Blocked(
            offer.reason.clone(),
        ));
    }
    offer.status = RecoveryOfferStatus::Resumed;
    offer.resolved_at_utc = Some(chrono::Utc::now().to_rfc3339());
    offer.resolution = Some("resume".into());
    offer.requires_user_choice = false;
    store.upsert_recovery_offer(&offer)?;

    // Move run back to executing and unpause if not low_disk.
    let low = store
        .get_scheduler_lease(run_id)?
        .map(|l| l.low_disk)
        .unwrap_or(false);
    let _ = store.append_event_atomic(
        Some("executing"),
        NewEvent {
            event_id: Uuid::new_v4(),
            run_id,
            project_id: None,
            phase_id: None,
            attempt_id: None,
            process_id: None,
            event_type: "recovery.resumed".into(),
            level: EventLevel::Info,
            timestamp_utc: chrono::Utc::now(),
            message: "User resumed after startup recovery".into(),
            payload: json!({ "lowDisk": low }),
        },
    );
    if !low {
        ensure_lease(store, run_id);
        let _ = store.set_scheduler_paused(run_id, false);
        let _ = store.set_scheduler_flags(run_id, Some(false), None);
    }
    Ok(offer)
}

fn ensure_lease(store: &Store, run_id: Uuid) {
    if store.get_scheduler_lease(run_id).ok().flatten().is_some() {
        return;
    }
    let _ = store.renew_scheduler_lease(run_id, "tiamat-recovery", 3, Some(true));
}

/// User chose Cancel — terminal cancel, keep workspaces per retention.
pub fn resolve_cancel(store: &Store, run_id: Uuid) -> RecoveryResult<RecoveryOffer> {
    let mut offer = store.get_recovery_offer(run_id)?.ok_or_else(|| {
        crate::recovery::error::RecoveryError::Blocked("no recovery offer".into())
    })?;
    // REL-001: fail closed before any write that would mark the run terminal.
    store.assert_run_may_become_terminal(run_id)?;

    offer.status = RecoveryOfferStatus::Cancelled;
    offer.resolved_at_utc = Some(chrono::Utc::now().to_rfc3339());
    offer.resolution = Some("cancel".into());
    offer.requires_user_choice = false;
    offer.resume_allowed = false;
    store.upsert_recovery_offer(&offer)?;

    store.append_event_atomic(
        Some("cancelled"),
        NewEvent {
            event_id: Uuid::new_v4(),
            run_id,
            project_id: None,
            phase_id: None,
            attempt_id: None,
            process_id: None,
            event_type: "recovery.cancelled".into(),
            level: EventLevel::Warning,
            timestamp_utc: chrono::Utc::now(),
            message: "User cancelled after startup recovery; no new execution".into(),
            payload: json!({}),
        },
    )?;
    ensure_lease(store, run_id);
    let _ = store.set_scheduler_paused(run_id, true);
    Ok(offer)
}

/// Whether scheduling/new work is allowed (no pending recovery offer).
pub fn execution_allowed(store: &Store, run_id: Uuid) -> RecoveryResult<bool> {
    if let Some(offer) = store.get_recovery_offer(run_id)? {
        if offer.requires_user_choice {
            return Ok(false);
        }
        if offer.status == RecoveryOfferStatus::Cancelled
            || offer.status == RecoveryOfferStatus::Blocked
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[allow(dead_code)]
fn _use_reconcile_report(_: &ReconcileReport) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::{AttemptRecord, PhaseRecord, PhaseRuntimeStatus as PRS};
    use tempfile::tempdir;
    use tiamat_contracts::ModelTier;

    #[test]
    fn startup_marks_active_attempts_and_creates_offer() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("a")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "active", "executing").unwrap();
        store
            .upsert_phase(&PhaseRecord {
                run_id,
                phase_id: "P01".into(),
                title: "One".into(),
                status: PRS::Running,
                project_ids: vec!["app".into()],
                write_roots: vec![".".into()],
                resource_locks: vec![],
                dependencies: vec![],
                model_tier: ModelTier::Composer,
                estimated_minutes: 5,
                critical_path_length: 1,
                ready_at_utc: None,
                queued_at_utc: None,
                attempt_count: 1,
                last_failure_kind: None,
            })
            .unwrap();
        let attempt_id = Uuid::new_v4();
        store
            .insert_attempt(&AttemptRecord {
                attempt_id,
                run_id,
                phase_id: "P01".into(),
                attempt_number: 1,
                status: AttemptStatus::Running,
                terminal_result: None,
                requested_tier: ModelTier::Composer,
                requested_model: "composer-2.5".into(),
                selected_model: "composer-2.5".into(),
                selection_reason: "test".into(),
                availability: vec![],
                resume_parent_attempt_id: None,
                progress_useful: true,
                failure_kind: None,
                started_at_utc: Some(chrono::Utc::now().to_rfc3339()),
                finished_at_utc: None,
            })
            .unwrap();

        let report = run_startup_recovery(&store, Some(dir.path())).unwrap();
        assert_eq!(report.interrupted_attempts.len(), 1);
        assert!(report.offer.is_some());
        let offer = report.offer.unwrap();
        assert!(offer.requires_user_choice);
        assert!(offer.resume_allowed);
        assert!(!execution_allowed(&store, run_id).unwrap());

        let resumed = resolve_resume(&store, run_id).unwrap();
        assert_eq!(resumed.status, RecoveryOfferStatus::Resumed);
        assert!(execution_allowed(&store, run_id).unwrap());
    }

    #[test]
    fn cancel_blocks_further_execution() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("a")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "active", "executing").unwrap();
        let report = run_startup_recovery(&store, Some(dir.path())).unwrap();
        assert!(report.offer.is_some());
        resolve_cancel(&store, run_id).unwrap();
        assert!(!execution_allowed(&store, run_id).unwrap());
        assert_eq!(store.get_run(run_id).unwrap().unwrap().status, "cancelled");
    }

    fn seed_recovery_offer(store: &Store, _run_id: Uuid) {
        let report = run_startup_recovery(store, None).unwrap();
        assert!(report.offer.is_some());
    }

    fn active_hosted_process(run_id: Uuid) -> crate::process::ProcessRecord {
        let now = chrono::Utc::now().to_rfc3339();
        crate::process::ProcessRecord {
            process_id: Uuid::new_v4(),
            run_id,
            phase_id: Some("P01".into()),
            attempt_id: None,
            executable: "fake".into(),
            args_redacted: vec![],
            pid: Some(4242),
            creation_time_100ns: Some(1),
            executable_identity: None,
            job_name: Some("job".into()),
            job_associated: true,
            parent_pid: None,
            workspace: None,
            state: crate::process::ProcessState::Active,
            heartbeat_at_utc: Some(now.clone()),
            registered_at_utc: now.clone(),
            spawned_at_utc: Some(now),
            stopped_at_utc: None,
            reaped_at_utc: None,
            exit_code: None,
            terminal_reason: None,
            chat_id: None,
            resume_metadata: json!({}),
            cleanup_evidence: json!({}),
            metadata: json!({}),
        }
    }

    #[test]
    fn recovery_cancel_blocked_while_active_processes_remain() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("a")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "active", "executing").unwrap();
        seed_recovery_offer(&store, run_id);
        store
            .upsert_process(&active_hosted_process(run_id))
            .unwrap();

        let prior = store.get_run(run_id).unwrap().unwrap().status;
        let err = resolve_cancel(&store, run_id).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("active process") || msg.contains("cannot mark run terminal"),
            "unexpected error: {msg}"
        );
        assert_ne!(store.get_run(run_id).unwrap().unwrap().status, "cancelled");
        assert_eq!(store.get_run(run_id).unwrap().unwrap().status, prior);
        let offer = store.get_recovery_offer(run_id).unwrap().unwrap();
        assert_eq!(offer.status, RecoveryOfferStatus::Pending);
    }

    #[test]
    fn recovery_cancel_blocked_when_cleanup_proof_incomplete() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db"), dir.path().join("a")).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "active", "executing").unwrap();
        seed_recovery_offer(&store, run_id);

        let mut proc = active_hosted_process(run_id);
        proc.state = crate::process::ProcessState::Reaped;
        proc.stopped_at_utc = Some(chrono::Utc::now().to_rfc3339());
        proc.reaped_at_utc = Some(chrono::Utc::now().to_rfc3339());
        store.upsert_process(&proc).unwrap();
        // Hosted process existed but no successful cleanup proof → fail closed.
        store
            .assert_run_may_become_terminal(run_id)
            .expect_err("missing cleanup proof must block");

        let prior = store.get_run(run_id).unwrap().unwrap().status;
        let err = resolve_cancel(&store, run_id).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("cleanup proof") || msg.contains("cannot mark run terminal"),
            "unexpected error: {msg}"
        );
        assert_ne!(store.get_run(run_id).unwrap().unwrap().status, "cancelled");
        assert_eq!(store.get_run(run_id).unwrap().unwrap().status, prior);
    }
}
