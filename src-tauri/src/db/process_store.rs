use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::db::{DbError, DbResult, Store};
use crate::process::{AbortSettings, CleanupProof, ProcessRecord, ProcessState};

impl Store {
    pub fn upsert_process(&self, rec: &ProcessRecord) -> DbResult<()> {
        self.conn().execute(
            "INSERT INTO processes (
                process_id, run_id, phase_id, attempt_id, executable, args_redacted,
                pid, creation_time_100ns, executable_identity, job_name, job_associated,
                parent_pid, workspace, state, heartbeat_at_utc, registered_at_utc,
                spawned_at_utc, stopped_at_utc, reaped_at_utc, exit_code, terminal_reason,
                chat_id, resume_metadata_json, cleanup_evidence_json, metadata_json
             ) VALUES (
                ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25
             )
             ON CONFLICT(process_id) DO UPDATE SET
                phase_id=excluded.phase_id,
                attempt_id=excluded.attempt_id,
                executable=excluded.executable,
                args_redacted=excluded.args_redacted,
                pid=excluded.pid,
                creation_time_100ns=excluded.creation_time_100ns,
                executable_identity=excluded.executable_identity,
                job_name=excluded.job_name,
                job_associated=excluded.job_associated,
                parent_pid=excluded.parent_pid,
                workspace=excluded.workspace,
                state=excluded.state,
                heartbeat_at_utc=excluded.heartbeat_at_utc,
                spawned_at_utc=excluded.spawned_at_utc,
                stopped_at_utc=excluded.stopped_at_utc,
                reaped_at_utc=excluded.reaped_at_utc,
                exit_code=excluded.exit_code,
                terminal_reason=excluded.terminal_reason,
                chat_id=excluded.chat_id,
                resume_metadata_json=excluded.resume_metadata_json,
                cleanup_evidence_json=excluded.cleanup_evidence_json,
                metadata_json=excluded.metadata_json",
            params![
                rec.process_id.to_string(),
                rec.run_id.to_string(),
                rec.phase_id,
                rec.attempt_id.map(|id| id.to_string()),
                rec.executable,
                serde_json::to_string(&rec.args_redacted)?,
                rec.pid.map(|p| p as i64),
                rec.creation_time_100ns.map(|t| t as i64),
                rec.executable_identity,
                rec.job_name,
                rec.job_associated as i64,
                rec.parent_pid.map(|p| p as i64),
                rec.workspace,
                rec.state.as_str(),
                rec.heartbeat_at_utc,
                rec.registered_at_utc,
                rec.spawned_at_utc,
                rec.stopped_at_utc,
                rec.reaped_at_utc,
                rec.exit_code,
                rec.terminal_reason,
                rec.chat_id,
                serde_json::to_string(&rec.resume_metadata)?,
                serde_json::to_string(&rec.cleanup_evidence)?,
                serde_json::to_string(&rec.metadata)?,
            ],
        )?;
        Ok(())
    }

    pub fn get_process(&self, process_id: Uuid) -> DbResult<Option<ProcessRecord>> {
        self.conn()
            .query_row(
                "SELECT process_id, run_id, phase_id, attempt_id, executable, args_redacted,
                        pid, creation_time_100ns, executable_identity, job_name, job_associated,
                        parent_pid, workspace, state, heartbeat_at_utc, registered_at_utc,
                        spawned_at_utc, stopped_at_utc, reaped_at_utc, exit_code, terminal_reason,
                        chat_id, resume_metadata_json, cleanup_evidence_json, metadata_json
                 FROM processes WHERE process_id = ?1",
                params![process_id.to_string()],
                map_process,
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn list_processes_for_run(&self, run_id: Uuid) -> DbResult<Vec<ProcessRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT process_id, run_id, phase_id, attempt_id, executable, args_redacted,
                    pid, creation_time_100ns, executable_identity, job_name, job_associated,
                    parent_pid, workspace, state, heartbeat_at_utc, registered_at_utc,
                    spawned_at_utc, stopped_at_utc, reaped_at_utc, exit_code, terminal_reason,
                    chat_id, resume_metadata_json, cleanup_evidence_json, metadata_json
             FROM processes WHERE run_id = ?1 ORDER BY registered_at_utc ASC",
        )?;
        let rows = stmt.query_map(params![run_id.to_string()], map_process)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_active_processes(&self) -> DbResult<Vec<ProcessRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT process_id, run_id, phase_id, attempt_id, executable, args_redacted,
                    pid, creation_time_100ns, executable_identity, job_name, job_associated,
                    parent_pid, workspace, state, heartbeat_at_utc, registered_at_utc,
                    spawned_at_utc, stopped_at_utc, reaped_at_utc, exit_code, terminal_reason,
                    chat_id, resume_metadata_json, cleanup_evidence_json, metadata_json
             FROM processes WHERE state != 'reaped' ORDER BY registered_at_utc ASC",
        )?;
        let rows = stmt.query_map([], map_process)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn active_process_count(&self, run_id: Option<Uuid>) -> DbResult<u32> {
        let count: i64 = match run_id {
            Some(id) => self.conn().query_row(
                "SELECT COUNT(*) FROM processes WHERE run_id = ?1 AND state != 'reaped'",
                params![id.to_string()],
                |row| row.get(0),
            )?,
            None => self.conn().query_row(
                "SELECT COUNT(*) FROM processes WHERE state != 'reaped'",
                [],
                |row| row.get(0),
            )?,
        };
        Ok(count as u32)
    }

    pub fn insert_cleanup_proof(&self, proof: &CleanupProof) -> DbResult<()> {
        self.conn().execute(
            "INSERT INTO process_cleanup_proofs (
                proof_id, run_id, process_id, observed_at_utc, active_process_count,
                job_handle_open, handles_closed, zero_active_observed, success, detail_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                proof.proof_id.to_string(),
                proof.run_id.to_string(),
                proof.process_id.map(|id| id.to_string()),
                proof.observed_at_utc,
                proof.active_process_count as i64,
                proof.job_handle_open as i64,
                proof.handles_closed as i64,
                proof.zero_active_observed as i64,
                proof.success as i64,
                serde_json::to_string(&proof.detail)?,
            ],
        )?;
        Ok(())
    }

    pub fn latest_cleanup_proof(&self, run_id: Uuid) -> DbResult<Option<CleanupProof>> {
        self.conn()
            .query_row(
                "SELECT proof_id, run_id, process_id, observed_at_utc, active_process_count,
                        job_handle_open, handles_closed, zero_active_observed, success, detail_json
                 FROM process_cleanup_proofs WHERE run_id = ?1
                 ORDER BY observed_at_utc DESC LIMIT 1",
                params![run_id.to_string()],
                map_proof,
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn get_abort_settings(&self) -> DbResult<AbortSettings> {
        self.conn()
            .query_row(
                "SELECT shortcut, registered, degraded, collision_reason, degraded_acknowledged,
                        tray_fallback_enabled, second_press_force_ms, updated_at_utc
                 FROM abort_settings WHERE id = 1",
                [],
                |row| {
                    Ok(AbortSettings {
                        shortcut: row.get(0)?,
                        registered: row.get::<_, i64>(1)? != 0,
                        degraded: row.get::<_, i64>(2)? != 0,
                        collision_reason: row.get(3)?,
                        degraded_acknowledged: row.get::<_, i64>(4)? != 0,
                        tray_fallback_enabled: row.get::<_, i64>(5)? != 0,
                        second_press_force_ms: row.get::<_, i64>(6)? as u64,
                        updated_at_utc: row.get(7)?,
                    })
                },
            )
            .map_err(DbError::from)
    }

    pub fn save_abort_settings(&self, settings: &AbortSettings) -> DbResult<()> {
        self.conn().execute(
            "UPDATE abort_settings SET
                shortcut = ?1,
                registered = ?2,
                degraded = ?3,
                collision_reason = ?4,
                degraded_acknowledged = ?5,
                tray_fallback_enabled = ?6,
                second_press_force_ms = ?7,
                updated_at_utc = ?8
             WHERE id = 1",
            params![
                settings.shortcut,
                settings.registered as i64,
                settings.degraded as i64,
                settings.collision_reason,
                settings.degraded_acknowledged as i64,
                settings.tray_fallback_enabled as i64,
                settings.second_press_force_ms as i64,
                settings.updated_at_utc,
            ],
        )?;
        Ok(())
    }

    /// Terminal run statuses require zero active process registry entries + successful cleanup proof
    /// whenever any process for the run was Job-associated (hosted).
    pub fn assert_run_may_become_terminal(&self, run_id: Uuid) -> DbResult<()> {
        let active = self.active_process_count(Some(run_id))?;
        if active > 0 {
            return Err(DbError::Validation(format!(
                "cannot mark run terminal: {active} active process registry entries remain"
            )));
        }
        let procs = self.list_processes_for_run(run_id)?;
        let hosted_any = procs.iter().any(|p| p.job_associated);
        if hosted_any {
            let Some(proof) = self.latest_cleanup_proof(run_id)? else {
                return Err(DbError::Validation(
                    "cannot mark run terminal: hosted processes require a cleanup proof".into(),
                ));
            };
            if !proof.success || !proof.zero_active_observed {
                return Err(DbError::Validation(
                    "cannot mark run terminal: cleanup proof incomplete".into(),
                ));
            }
        } else if let Some(proof) = self.latest_cleanup_proof(run_id)? {
            if !proof.success || !proof.zero_active_observed {
                return Err(DbError::Validation(
                    "cannot mark run terminal: cleanup proof incomplete".into(),
                ));
            }
        }
        Ok(())
    }
}

fn map_process(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProcessRecord> {
    let process_id = Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::nil());
    let run_id = Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::nil());
    let attempt_id = row
        .get::<_, Option<String>>(3)?
        .and_then(|s| Uuid::parse_str(&s).ok());
    let args_redacted: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
    let resume_metadata: Value =
        serde_json::from_str(&row.get::<_, String>(22)?).unwrap_or_else(|_| json!({}));
    let cleanup_evidence: Value =
        serde_json::from_str(&row.get::<_, String>(23)?).unwrap_or_else(|_| json!({}));
    let metadata: Value =
        serde_json::from_str(&row.get::<_, String>(24)?).unwrap_or_else(|_| json!({}));
    Ok(ProcessRecord {
        process_id,
        run_id,
        phase_id: row.get(2)?,
        attempt_id,
        executable: row.get(4)?,
        args_redacted,
        pid: row.get::<_, Option<i64>>(6)?.map(|v| v as u32),
        creation_time_100ns: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
        executable_identity: row.get(8)?,
        job_name: row.get(9)?,
        job_associated: row.get::<_, i64>(10)? != 0,
        parent_pid: row.get::<_, Option<i64>>(11)?.map(|v| v as u32),
        workspace: row.get(12)?,
        state: ProcessState::parse(&row.get::<_, String>(13)?),
        heartbeat_at_utc: row.get(14)?,
        registered_at_utc: row.get(15)?,
        spawned_at_utc: row.get(16)?,
        stopped_at_utc: row.get(17)?,
        reaped_at_utc: row.get(18)?,
        exit_code: row.get(19)?,
        terminal_reason: row.get(20)?,
        chat_id: row.get(21)?,
        resume_metadata,
        cleanup_evidence,
        metadata,
    })
}

fn map_proof(row: &rusqlite::Row<'_>) -> rusqlite::Result<CleanupProof> {
    Ok(CleanupProof {
        proof_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::nil()),
        run_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::nil()),
        process_id: row
            .get::<_, Option<String>>(2)?
            .and_then(|s| Uuid::parse_str(&s).ok()),
        observed_at_utc: row.get(3)?,
        active_process_count: row.get::<_, i64>(4)? as u32,
        job_handle_open: row.get::<_, i64>(5)? != 0,
        handles_closed: row.get::<_, i64>(6)? != 0,
        zero_active_observed: row.get::<_, i64>(7)? != 0,
        success: row.get::<_, i64>(8)? != 0,
        detail: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_else(|_| json!({})),
    })
}
