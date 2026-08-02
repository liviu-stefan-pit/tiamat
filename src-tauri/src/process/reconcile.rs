//! Startup identity-safe reconciliation of nonterminal process records.

use serde_json::json;
use uuid::Uuid;

use crate::db::{NewEvent, Store};
use tiamat_contracts::EventLevel;

use super::error::ProcessResult;
use super::identity;
use super::types::{ProcessState, ReconcileReport};

pub fn reconcile_owned_processes(store: &Store) -> ProcessResult<ReconcileReport> {
    let active = store.list_active_processes()?;
    let mut report = ReconcileReport {
        inspected: active.len() as u32,
        terminated: 0,
        already_gone: 0,
        unverifiable: 0,
        interrupted_attempts: 0,
        hard_failure: false,
        messages: Vec::new(),
    };

    for mut rec in active {
        let Some(pid) = rec.pid else {
            report.unverifiable += 1;
            report.hard_failure = true;
            report
                .messages
                .push(format!("{}: missing pid — unverifiable", rec.process_id));
            continue;
        };
        let Some(ctime) = rec.creation_time_100ns else {
            report.unverifiable += 1;
            report.hard_failure = true;
            report.messages.push(format!(
                "{}: missing creation time — unverifiable",
                rec.process_id
            ));
            continue;
        };
        if ctime == 0 {
            report.unverifiable += 1;
            report.hard_failure = true;
            report.messages.push(format!(
                "{}: creation_time_100ns=0 is unverifiable — hard failure (not already_gone)",
                rec.process_id
            ));
            continue;
        }
        let identity_path = rec
            .executable_identity
            .clone()
            .unwrap_or_else(|| rec.executable.clone());

        match identity::verify_live(pid, ctime, &identity_path) {
            Ok(false) => {
                // PID gone or reused by different process — mark reaped/lost.
                report.already_gone += 1;
                rec.state = ProcessState::Reaped;
                rec.terminal_reason = Some("reconcile_already_gone".into());
                rec.reaped_at_utc = Some(chrono::Utc::now().to_rfc3339());
                store.upsert_process(&rec)?;
                report.messages.push(format!(
                    "{}: pid {pid} no longer matches identity — marked reaped",
                    rec.process_id
                ));
            }
            Ok(true) => {
                // Verifiably owned leftover — terminate.
                let killed = identity_checked_terminate(pid, ctime, &identity_path);
                if killed {
                    report.terminated += 1;
                    rec.state = ProcessState::Reaped;
                    rec.terminal_reason = Some("reconcile_terminated".into());
                    rec.stopped_at_utc = Some(chrono::Utc::now().to_rfc3339());
                    rec.reaped_at_utc = Some(chrono::Utc::now().to_rfc3339());
                    store.upsert_process(&rec)?;
                    report
                        .messages
                        .push(format!("{}: terminated leftover pid {pid}", rec.process_id));
                } else {
                    report.unverifiable += 1;
                    report.hard_failure = true;
                    report.messages.push(format!(
                        "{}: failed to terminate verifiable pid {pid}",
                        rec.process_id
                    ));
                }
            }
            Err(e) => {
                report.unverifiable += 1;
                report.hard_failure = true;
                report
                    .messages
                    .push(format!("{}: identity check error: {e}", rec.process_id));
            }
        }

        if let Some(attempt_id) = rec.attempt_id {
            // Best-effort: mark attempt interrupted via metadata event.
            report.interrupted_attempts += 1;
            let _ = store.append_event_atomic(
                None,
                NewEvent {
                    event_id: Uuid::new_v4(),
                    run_id: rec.run_id,
                    project_id: None,
                    phase_id: rec.phase_id.clone(),
                    attempt_id: Some(attempt_id),
                    process_id: Some(rec.process_id),
                    event_type: "recovery.process_reconciled".into(),
                    level: EventLevel::Warning,
                    timestamp_utc: chrono::Utc::now(),
                    message: "Startup reconciliation interrupted owned attempt".into(),
                    payload: json!({
                        "attemptId": attempt_id,
                        "processId": rec.process_id,
                        "hardFailure": report.hard_failure,
                    }),
                },
            );
        }
    }

    Ok(report)
}

fn identity_checked_terminate(
    pid: u32,
    creation_time_100ns: u64,
    executable_identity: &str,
) -> bool {
    match identity::verify_live(pid, creation_time_100ns, executable_identity) {
        Ok(true) => std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use crate::process::types::ProcessRecord;
    use tempfile::tempdir;

    #[test]
    fn reconcile_marks_missing_identity_unverifiable() {
        let dir = tempdir().unwrap();
        let store = Store::open_in_memory(dir.path()).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "reconcile", "executing").unwrap();
        let rec = ProcessRecord {
            process_id: Uuid::new_v4(),
            run_id,
            phase_id: Some("P07".into()),
            attempt_id: None,
            executable: "fake.exe".into(),
            args_redacted: vec![],
            pid: Some(1),
            creation_time_100ns: None,
            executable_identity: None,
            job_name: None,
            job_associated: true,
            parent_pid: None,
            workspace: None,
            state: ProcessState::Active,
            heartbeat_at_utc: None,
            registered_at_utc: chrono::Utc::now().to_rfc3339(),
            spawned_at_utc: None,
            stopped_at_utc: None,
            reaped_at_utc: None,
            exit_code: None,
            terminal_reason: None,
            chat_id: None,
            resume_metadata: json!({}),
            cleanup_evidence: json!({}),
            metadata: json!({}),
        };
        store.upsert_process(&rec).unwrap();
        let report = reconcile_owned_processes(&store).unwrap();
        assert!(report.hard_failure);
        assert_eq!(report.unverifiable, 1);
    }

    #[test]
    fn reconcile_ctime_zero_is_hard_failure_not_already_gone() {
        let dir = tempdir().unwrap();
        let store = Store::open_in_memory(dir.path()).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "reconcile", "executing").unwrap();
        let rec = ProcessRecord {
            process_id: Uuid::new_v4(),
            run_id,
            phase_id: Some("P07".into()),
            attempt_id: None,
            executable: "fake.exe".into(),
            args_redacted: vec![],
            pid: Some(1),
            creation_time_100ns: Some(0),
            executable_identity: Some("fake.exe".into()),
            job_name: None,
            job_associated: true,
            parent_pid: None,
            workspace: None,
            state: ProcessState::Active,
            heartbeat_at_utc: None,
            registered_at_utc: chrono::Utc::now().to_rfc3339(),
            spawned_at_utc: None,
            stopped_at_utc: None,
            reaped_at_utc: None,
            exit_code: None,
            terminal_reason: None,
            chat_id: None,
            resume_metadata: json!({}),
            cleanup_evidence: json!({}),
            metadata: json!({}),
        };
        store.upsert_process(&rec).unwrap();
        let report = reconcile_owned_processes(&store).unwrap();
        assert!(report.hard_failure);
        assert_eq!(report.unverifiable, 1);
        assert_eq!(report.already_gone, 0);
        let still = store.list_active_processes().unwrap();
        assert_eq!(
            still.len(),
            1,
            "ctime=0 must not be marked already_gone/reaped"
        );
    }
}
