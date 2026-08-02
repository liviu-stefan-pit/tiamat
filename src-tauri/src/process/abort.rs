//! Global abort: first press cancel, second press force within 3s, degraded shortcut.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::db::{NewEvent, Store};
use tiamat_contracts::EventLevel;

use super::error::{ProcessError, ProcessResult};
use super::host::ProcessHost;
use super::types::{AbortAction, AbortPressResult, AbortSettings, ClosePolicyChoice};

pub struct AbortController {
    last_press: Mutex<Option<Instant>>,
    keep_running: AtomicBool,
    tray_available: AtomicBool,
}

impl Default for AbortController {
    fn default() -> Self {
        Self::new()
    }
}

impl AbortController {
    pub fn new() -> Self {
        Self {
            last_press: Mutex::new(None),
            keep_running: AtomicBool::new(false),
            tray_available: AtomicBool::new(false),
        }
    }

    pub fn set_tray_available(&self, available: bool) {
        self.tray_available.store(available, Ordering::SeqCst);
    }

    pub fn tray_available(&self) -> bool {
        self.tray_available.load(Ordering::SeqCst)
    }

    pub fn set_keep_running(&self, keep: bool) {
        self.keep_running.store(keep, Ordering::SeqCst);
    }

    pub fn keep_running(&self) -> bool {
        self.keep_running.load(Ordering::SeqCst)
    }

    /// Handle Ctrl+Shift+F12 (or rebound) / UI emergency stop / tray abort.
    pub fn handle_press(
        &self,
        store: &Store,
        host: &ProcessHost,
        run_id: Option<Uuid>,
        active_run: bool,
        force: bool,
    ) -> ProcessResult<AbortPressResult> {
        let settings = store.get_abort_settings()?;
        let second_window = Duration::from_millis(settings.second_press_force_ms);

        let mut forced = force;
        if !forced {
            let mut last = self.last_press.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(prev) = *last {
                if prev.elapsed() <= second_window {
                    forced = true;
                }
            }
            *last = Some(Instant::now());
        }

        if !active_run && !forced {
            // First press with no active run → confirmation/countdown only.
            return Ok(AbortPressResult {
                action: AbortAction::PromptConfirm,
                forced: false,
                active_run: false,
                message: "No active run. Confirm emergency stop readiness.".into(),
                processes_stopped: 0,
                cleanup_ok: true,
            });
        }

        // Active run: begin emergency cancellation immediately (or force).
        let stopped = if let Some(id) = run_id {
            host.cancel_all_for_run(id, forced)
        } else {
            host.cancel_all(forced)
        };

        // After force, poll until registry empties or deadline — never claim success early.
        let poll_budget = if forced {
            Duration::from_millis(2_000)
        } else {
            Duration::from_millis(50)
        };
        let deadline = Instant::now() + poll_budget;
        let mut active_remaining = store.active_process_count(run_id)?;
        while active_remaining > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(40));
            active_remaining = store.active_process_count(run_id)?;
        }

        // First press: cleanup_ok only when registry already empty (normally false / pending).
        // Forced: re-query and hard-fail if nonzero remain.
        let cleanup_ok = active_remaining == 0;
        if forced && !cleanup_ok {
            return Err(ProcessError::Cleanup(format!(
                "forced abort cleanup failed: {active_remaining} active process(es) remain"
            )));
        }

        if let Some(id) = run_id {
            // REL-001: never claim terminal cancelled without cleanup-proof gate.
            let status_update = if forced && cleanup_ok {
                store.assert_run_may_become_terminal(id).map_err(|e| {
                    ProcessError::Cleanup(format!(
                        "abort terminal gate failed after force cleanup: {e}"
                    ))
                })?;
                Some("cancelled")
            } else if !forced && (stopped > 0 || active_remaining > 0) {
                Some("cancelling")
            } else {
                None
            };
            let _ = store.append_event_atomic(
                status_update,
                NewEvent {
                    event_id: Uuid::new_v4(),
                    run_id: id,
                    project_id: None,
                    phase_id: None,
                    attempt_id: None,
                    process_id: None,
                    event_type: if forced {
                        "process.forced_abort".into()
                    } else {
                        "process.emergency_abort".into()
                    },
                    level: EventLevel::Warning,
                    timestamp_utc: chrono::Utc::now(),
                    message: if forced {
                        "Second-press forced Job Object termination".into()
                    } else {
                        "Emergency cancellation started (Ctrl+Shift+F12 / UI)".into()
                    },
                    payload: serde_json::json!({
                        "forced": forced,
                        "processesSignaled": stopped,
                        "cleanupOk": cleanup_ok,
                        "cleanupPending": !cleanup_ok && !forced,
                        "activeRemaining": active_remaining,
                        "shortcut": settings.shortcut,
                    }),
                },
            );
        }

        Ok(AbortPressResult {
            action: if forced {
                AbortAction::ForceTerminate
            } else {
                AbortAction::BeginEmergencyCancel
            },
            forced,
            active_run,
            message: if forced {
                format!("Forced abort signaled for {stopped} process(es); cleanup_ok={cleanup_ok}")
            } else if cleanup_ok {
                format!("Emergency cancel completed for {stopped} process(es)")
            } else {
                format!(
                    "Emergency cancel started for {stopped} process(es); cleanup pending ({active_remaining} active)"
                )
            },
            processes_stopped: stopped,
            cleanup_ok,
        })
    }

    pub fn apply_close_policy(
        &self,
        store: &Store,
        host: &ProcessHost,
        run_id: Option<Uuid>,
        choice: ClosePolicyChoice,
    ) -> ProcessResult<AbortPressResult> {
        match choice {
            ClosePolicyChoice::KeepRunning => {
                self.set_keep_running(true);
                Ok(AbortPressResult {
                    action: AbortAction::Acknowledged,
                    forced: false,
                    active_run: run_id.is_some(),
                    message: "Keep Tiamat running — work continues in background.".into(),
                    processes_stopped: 0,
                    cleanup_ok: true,
                })
            }
            ClosePolicyChoice::StopAllAndExit => {
                self.set_keep_running(false);
                let result = self.handle_press(store, host, run_id, run_id.is_some(), true)?;
                Ok(AbortPressResult {
                    action: AbortAction::ForceTerminate,
                    forced: true,
                    active_run: result.active_run,
                    message: "Stop all and exit — forced termination requested.".into(),
                    processes_stopped: result.processes_stopped,
                    cleanup_ok: result.cleanup_ok,
                })
            }
        }
    }
}

pub fn mark_shortcut_registered(
    store: &Store,
    registered: bool,
    collision_reason: Option<String>,
) -> ProcessResult<AbortSettings> {
    let mut settings = store.get_abort_settings()?;
    settings.registered = registered;
    settings.degraded = !registered;
    settings.collision_reason = collision_reason;
    if registered {
        settings.degraded_acknowledged = false;
    }
    settings.updated_at_utc = chrono::Utc::now().to_rfc3339();
    store.save_abort_settings(&settings)?;
    Ok(settings)
}

pub fn acknowledge_degraded(store: &Store) -> ProcessResult<AbortSettings> {
    let mut settings = store.get_abort_settings()?;
    settings.degraded_acknowledged = true;
    settings.updated_at_utc = chrono::Utc::now().to_rfc3339();
    store.save_abort_settings(&settings)?;
    Ok(settings)
}

pub fn rebind_shortcut(store: &Store, shortcut: &str) -> ProcessResult<AbortSettings> {
    let mut settings = store.get_abort_settings()?;
    settings.shortcut = shortcut.to_string();
    settings.registered = false;
    settings.degraded = true;
    settings.collision_reason = Some("rebinding pending native registration".into());
    settings.degraded_acknowledged = false;
    settings.updated_at_utc = chrono::Utc::now().to_rfc3339();
    store.save_abort_settings(&settings)?;
    Ok(settings)
}

/// Start is blocked while global abort is degraded unless the user acknowledges.
pub fn can_start_with_abort_policy(settings: &AbortSettings) -> bool {
    if settings.degraded {
        settings.degraded_acknowledged && settings.tray_fallback_enabled
    } else {
        settings.registered || settings.tray_fallback_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use crate::process::types::ProcessRecord;
    use crate::process::ProcessState;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn degraded_requires_ack_to_start() {
        let s = AbortSettings {
            degraded: true,
            degraded_acknowledged: false,
            ..Default::default()
        };
        assert!(!can_start_with_abort_policy(&s));
        let s2 = AbortSettings {
            degraded: true,
            degraded_acknowledged: true,
            ..Default::default()
        };
        assert!(can_start_with_abort_policy(&s2));
    }

    #[test]
    fn second_press_force_window_constant() {
        let settings = AbortSettings::default();
        assert_eq!(settings.second_press_force_ms, 3_000);
    }

    #[test]
    fn first_press_does_not_claim_cleanup_while_active() {
        let dir = tempdir().unwrap();
        let store = Store::open_in_memory(dir.path()).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "abort", "executing").unwrap();
        let rec = ProcessRecord {
            process_id: Uuid::new_v4(),
            run_id,
            phase_id: Some("P07".into()),
            attempt_id: None,
            executable: "fake.exe".into(),
            args_redacted: vec![],
            pid: Some(42),
            creation_time_100ns: Some(1),
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
        let host = ProcessHost::new();
        let ctrl = AbortController::new();
        let result = ctrl
            .handle_press(&store, &host, Some(run_id), true, false)
            .unwrap();
        assert!(!result.forced);
        assert!(
            !result.cleanup_ok,
            "first press must not claim cleanup success"
        );
        assert_eq!(result.action, AbortAction::BeginEmergencyCancel);
    }

    #[test]
    fn forced_abort_hard_fails_when_active_remain() {
        let dir = tempdir().unwrap();
        let store = Store::open_in_memory(dir.path()).unwrap();
        let run_id = Uuid::new_v4();
        store.create_run(run_id, "abort", "executing").unwrap();
        let rec = ProcessRecord {
            process_id: Uuid::new_v4(),
            run_id,
            phase_id: Some("P07".into()),
            attempt_id: None,
            executable: "fake.exe".into(),
            args_redacted: vec![],
            pid: Some(42),
            creation_time_100ns: Some(1),
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
        let host = ProcessHost::new();
        let ctrl = AbortController::new();
        let err = ctrl
            .handle_press(&store, &host, Some(run_id), true, true)
            .expect_err("forced abort must hard-fail while actives remain");
        assert!(err.to_string().contains("cleanup failed"));
    }
}
