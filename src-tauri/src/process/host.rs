//! Process host: spawn into Job Object, watchdog, graceful/forced stop, drain/reap.

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use uuid::Uuid;

use crate::db::{NewEvent, Store};
use crate::security::{
    apply_output_limits, redact_for_persistence, redact_line, OutputLimitConfig,
};
use tiamat_contracts::EventLevel;

use super::error::{ProcessError, ProcessResult};
use super::identity;
use super::spawn::{self, AssociationMethod, HostedChild, SpawnedInJob};
use super::types::{
    CleanupProof, HostedProcessOutcome, ProcessRecord, ProcessState, ResumeMetadata, SpawnRequest,
    WatchdogConfig,
};

/// In-memory live handle paired with a durable registry row.
struct LiveProcess {
    #[allow(dead_code)]
    process_id: Uuid,
    run_id: Uuid,
    spawned: SpawnedInJob,
    cancel: Arc<AtomicBool>,
    force: Arc<AtomicBool>,
}

pub struct ProcessHost {
    lives: Mutex<HashMap<Uuid, LiveProcess>>,
}

/// Borrowed ProcessHost + Store for production Cursor/agent/verification spawns.
pub struct HostedSpawnContext<'a> {
    pub store: &'a Store,
    pub host: &'a ProcessHost,
}

impl Default for ProcessHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessHost {
    pub fn new() -> Self {
        Self {
            lives: Mutex::new(HashMap::new()),
        }
    }

    pub fn active_live_count(&self) -> usize {
        self.lives.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Register + spawn into a kill-on-close Job Object, run watchdog, stop/reap, persist cleanup.
    pub fn run_hosted(
        &self,
        store: &Store,
        request: SpawnRequest,
    ) -> ProcessResult<HostedProcessOutcome> {
        let process_id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        let redacted_args: Vec<String> = request.argv.iter().map(|a| redact_line(a)).collect();

        let mut record = ProcessRecord {
            process_id,
            run_id: request.run_id,
            phase_id: request.phase_id.clone(),
            attempt_id: request.attempt_id,
            executable: request.argv.first().cloned().unwrap_or_default(),
            args_redacted: redacted_args,
            pid: None,
            creation_time_100ns: None,
            executable_identity: None,
            job_name: None,
            job_associated: false,
            parent_pid: None,
            workspace: request.workspace.clone(),
            state: ProcessState::Registered,
            heartbeat_at_utc: Some(now.clone()),
            registered_at_utc: now.clone(),
            spawned_at_utc: None,
            stopped_at_utc: None,
            reaped_at_utc: None,
            exit_code: None,
            terminal_reason: None,
            chat_id: request.resume_chat_hint.clone(),
            resume_metadata: json!({}),
            cleanup_evidence: json!({}),
            metadata: json!({
                "watchdog": request.watchdog,
            }),
        };
        store.upsert_process(&record)?;
        emit_process_event(
            store,
            &record,
            "process.registered",
            "Process registered before spawn",
            json!({}),
        )?;

        let (spawned, mut child) = spawn::spawn_hosted(
            &request.argv,
            request.stdin.as_deref(),
            &request.env,
            request.workspace.as_deref(),
        )?;
        if spawned.creation_time_100ns == 0 {
            let _ = child.kill();
            return Err(ProcessError::Spawn(
                "refusing to register process with creation_time_100ns=0".into(),
            ));
        }

        record.pid = Some(spawned.pid);
        record.creation_time_100ns = Some(spawned.creation_time_100ns);
        record.executable_identity = Some(spawned.executable_identity.clone());
        record.job_name = spawned.job.name().map(str::to_string);
        record.job_associated = true;
        record.state = ProcessState::Spawned;
        record.spawned_at_utc = Some(chrono::Utc::now().to_rfc3339());
        let association_label = match spawned.association {
            AssociationMethod::ProcThreadAttributeJobList => "proc_thread_attribute_job_list",
            AssociationMethod::SuspendedAssignDegraded => "suspended_assign_degraded",
        };
        record.metadata = json!({
            "watchdog": request.watchdog,
            "association": association_label,
            "degradedAssociation": spawned.degraded_association,
        });
        store.upsert_process(&record)?;
        emit_process_event(
            store,
            &record,
            "process.spawned",
            &format!("Process spawned pid={}", spawned.pid),
            json!({
                "pid": spawned.pid,
                "association": association_label,
                "degradedAssociation": spawned.degraded_association,
            }),
        )?;
        if spawned.degraded_association {
            emit_process_event(
                store,
                &record,
                "process.association_degraded",
                "PROC_THREAD_ATTRIBUTE_JOB_LIST unavailable; used CREATE_SUSPENDED→assign→resume (orphan window on host crash)",
                json!({ "association": association_label }),
            )?;
        }

        record.state = ProcessState::Active;
        store.upsert_process(&record)?;
        emit_process_event(
            store,
            &record,
            "process.active",
            "Process active under Job Object",
            json!({}),
        )?;

        let cancel = Arc::new(AtomicBool::new(false));
        let force = Arc::new(AtomicBool::new(false));
        {
            let mut lives = self
                .lives
                .lock()
                .map_err(|e| ProcessError::Registry(format!("live process lock poisoned: {e}")))?;
            lives.insert(
                process_id,
                LiveProcess {
                    process_id,
                    run_id: request.run_id,
                    spawned: SpawnedInJob {
                        pid: spawned.pid,
                        process_handle: spawned.process_handle,
                        job: spawned.job,
                        association: spawned.association,
                        executable_identity: spawned.executable_identity.clone(),
                        creation_time_100ns: spawned.creation_time_100ns,
                        degraded_association: spawned.degraded_association,
                    },
                    cancel: cancel.clone(),
                    force: force.clone(),
                },
            );
        }

        // Re-take job from live map for watchdog stop — ownership stays in LiveProcess.
        let started = Instant::now();
        let outcome = self.wait_with_watchdog(
            store,
            &mut record,
            &mut child,
            process_id,
            &request.watchdog,
            &cancel,
            &force,
            request.resume_chat_hint.clone(),
            request.next_model_on_timeout.clone(),
            request.next_tier_on_timeout.clone(),
            request.attempt_id,
            started,
        );

        // Remove live entry; job drop triggers kill-on-close for any survivors.
        let live = {
            let mut lives = self
                .lives
                .lock()
                .map_err(|e| ProcessError::Registry(format!("live process lock poisoned: {e}")))?;
            lives.remove(&process_id)
        };

        let mut outcome = outcome?;
        if let Some(live) = live {
            let (cleanup_ok, active_after, proof) = finalize_cleanup(
                store,
                &record,
                &live.spawned,
                outcome.killed || outcome.timed_out || outcome.cancelled,
            )?;
            outcome.cleanup_ok = cleanup_ok;
            outcome.zero_survivors = active_after == 0;
            outcome.active_after_cleanup = active_after;
            record.cleanup_evidence = json!({
                "proofId": proof.proof_id,
                "activeAfter": active_after,
                "success": cleanup_ok,
            });
            record.state = ProcessState::Reaped;
            record.reaped_at_utc = Some(chrono::Utc::now().to_rfc3339());
            record.exit_code = outcome.exit_code;
            store.upsert_process(&record)?;
            emit_process_event(
                store,
                &record,
                "cleanup.proof",
                if cleanup_ok {
                    "Cleanup proof: zero active Job processes observed"
                } else {
                    "Cleanup proof FAILED: survivors or unverifiable state"
                },
                json!({
                    "activeAfter": active_after,
                    "success": cleanup_ok,
                    "proofId": proof.proof_id,
                }),
            )?;
        }

        if !outcome.cleanup_ok || !outcome.zero_survivors {
            return Err(ProcessError::Cleanup(format!(
                "owned processes remain after stop: active={}",
                outcome.active_after_cleanup
            )));
        }

        Ok(outcome)
    }

    pub fn request_cancel(&self, process_id: Uuid, forced: bool) -> bool {
        let Ok(lives) = self.lives.lock() else {
            return false;
        };
        if let Some(live) = lives.get(&process_id) {
            live.cancel.store(true, Ordering::SeqCst);
            if forced {
                live.force.store(true, Ordering::SeqCst);
                let _ = live.spawned.job.terminate(1);
            }
            true
        } else {
            false
        }
    }

    pub fn cancel_all_for_run(&self, run_id: Uuid, forced: bool) -> u32 {
        let Ok(lives) = self.lives.lock() else {
            return 0;
        };
        let mut n = 0u32;
        for live in lives.values() {
            if live.run_id == run_id {
                live.cancel.store(true, Ordering::SeqCst);
                if forced {
                    live.force.store(true, Ordering::SeqCst);
                    let _ = live.spawned.job.terminate(1);
                }
                n += 1;
            }
        }
        n
    }

    pub fn cancel_all(&self, forced: bool) -> u32 {
        let Ok(lives) = self.lives.lock() else {
            return 0;
        };
        let mut n = 0u32;
        for live in lives.values() {
            live.cancel.store(true, Ordering::SeqCst);
            if forced {
                live.force.store(true, Ordering::SeqCst);
                let _ = live.spawned.job.terminate(1);
            }
            n += 1;
        }
        n
    }

    #[allow(clippy::too_many_arguments)]
    fn wait_with_watchdog(
        &self,
        store: &Store,
        record: &mut ProcessRecord,
        child: &mut HostedChild,
        process_id: Uuid,
        watchdog: &WatchdogConfig,
        cancel: &AtomicBool,
        force: &AtomicBool,
        resume_chat_hint: Option<String>,
        next_model: Option<String>,
        next_tier: Option<String>,
        attempt_id: Option<Uuid>,
        started: Instant,
    ) -> ProcessResult<HostedProcessOutcome> {
        let stdout = child.take_stdout();
        let stderr = child.take_stderr();
        let stdout_buf = Arc::new(Mutex::new(String::new()));
        let stderr_buf = Arc::new(Mutex::new(String::new()));
        let stdout_done = Arc::new(AtomicBool::new(false));
        let stderr_done = Arc::new(AtomicBool::new(false));

        if let Some(mut out) = stdout {
            let buf = stdout_buf.clone();
            let done = stdout_done.clone();
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = out.read_to_end(&mut bytes);
                if let Ok(mut g) = buf.lock() {
                    *g = String::from_utf8_lossy(&bytes).into_owned();
                }
                done.store(true, Ordering::SeqCst);
            });
        } else {
            stdout_done.store(true, Ordering::SeqCst);
        }
        if let Some(mut err) = stderr {
            let buf = stderr_buf.clone();
            let done = stderr_done.clone();
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = err.read_to_end(&mut bytes);
                if let Ok(mut g) = buf.lock() {
                    *g = String::from_utf8_lossy(&bytes).into_owned();
                }
                done.store(true, Ordering::SeqCst);
            });
        } else {
            stderr_done.store(true, Ordering::SeqCst);
        }

        let mut warned = false;
        let mut graceful_requested = false;
        let mut timed_out = false;
        let mut cancelled = false;
        let mut killed = false;
        let mut exit_code = None;

        loop {
            if force.load(Ordering::SeqCst) {
                killed = true;
                cancelled = true;
                record.state = ProcessState::ForcedStop;
                record.stopped_at_utc = Some(chrono::Utc::now().to_rfc3339());
                record.terminal_reason = Some("forced_abort".into());
                let _ = store.upsert_process(record);
                break;
            }
            if cancel.load(Ordering::SeqCst) && !graceful_requested {
                cancelled = true;
                graceful_requested = true;
                record.state = ProcessState::GracefulStop;
                record.stopped_at_utc = Some(chrono::Utc::now().to_rfc3339());
                record.terminal_reason = Some("cancelled".into());
                let _ = store.upsert_process(record);
                let _ = emit_process_event(
                    store,
                    record,
                    "process.graceful_stop",
                    "Graceful stop requested",
                    json!({}),
                );
                // Cooperative: wait force_grace then kill job.
                let grace_deadline =
                    Instant::now() + Duration::from_millis(watchdog.force_grace_ms);
                while Instant::now() < grace_deadline {
                    if let Ok(Some(status)) = child.try_wait() {
                        exit_code = status.code();
                        break;
                    }
                    if force.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                if exit_code.is_none() {
                    killed = true;
                    record.state = ProcessState::ForcedStop;
                    record.terminal_reason = Some("forced_after_grace".into());
                    let _ = store.upsert_process(record);
                    force_terminate_live(self, process_id);
                }
                break;
            }

            let elapsed = started.elapsed().as_millis() as u64;
            if !warned && elapsed >= watchdog.warn_after_ms {
                warned = true;
                let _ = emit_process_event(
                    store,
                    record,
                    "watchdog.warning",
                    "Attempt watchdog warning threshold reached",
                    json!({ "elapsedMs": elapsed, "warnAfterMs": watchdog.warn_after_ms }),
                );
                let _ = emit_process_event(
                    store,
                    record,
                    "attempt.warning",
                    "attempt.warning",
                    json!({ "elapsedMs": elapsed }),
                );
            }
            if !graceful_requested && elapsed >= watchdog.graceful_after_ms {
                timed_out = true;
                graceful_requested = true;
                record.state = ProcessState::GracefulStop;
                record.stopped_at_utc = Some(chrono::Utc::now().to_rfc3339());
                record.terminal_reason = Some("timed_out".into());
                let _ = store.upsert_process(record);
                let _ = emit_process_event(
                    store,
                    record,
                    "watchdog.graceful_stop",
                    "Watchdog requested graceful stop",
                    json!({ "elapsedMs": elapsed }),
                );
                let grace_deadline =
                    Instant::now() + Duration::from_millis(watchdog.force_grace_ms);
                while Instant::now() < grace_deadline {
                    if let Ok(Some(status)) = child.try_wait() {
                        exit_code = status.code();
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                if exit_code.is_none() {
                    killed = true;
                    record.state = ProcessState::ForcedStop;
                    let _ = store.upsert_process(record);
                    let _ = emit_process_event(
                        store,
                        record,
                        "watchdog.forced_stop",
                        "Watchdog forced Job Object termination",
                        json!({}),
                    );
                    force_terminate_live(self, process_id);
                }
                break;
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    exit_code = status.code();
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(e) => return Err(ProcessError::Stop(format!("try_wait failed: {e}"))),
            }
        }

        // Keep graceful_requested meaningful for clippy.
        let _ = graceful_requested;

        // Ensure process is reaped.
        if exit_code.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => exit_code = status.code(),
                Ok(None) => {
                    force_terminate_live(self, process_id);
                    // Identity-checked taskkill fallback if still alive (never with ctime 0).
                    if let (Some(pid), Some(ctime), Some(exe)) = (
                        record.pid,
                        record.creation_time_100ns,
                        record.executable_identity.as_deref(),
                    ) {
                        if ctime != 0 {
                            identity_checked_taskkill(pid, ctime, exe);
                        }
                    }
                    let _ = child.wait();
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }

        // Drain pipes with bound.
        let drain_deadline = Instant::now() + Duration::from_millis(watchdog.drain_timeout_ms);
        while Instant::now() < drain_deadline {
            if stdout_done.load(Ordering::SeqCst) && stderr_done.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let stdout_text = stdout_buf.lock().map(|g| g.clone()).unwrap_or_default();
        let stderr_text = stderr_buf.lock().map(|g| g.clone()).unwrap_or_default();
        let limits = OutputLimitConfig::default();
        let stdout_limited = apply_output_limits(&stdout_text, &limits);
        let stderr_limited = apply_output_limits(&stderr_text, &limits);
        let (stdout_redacted, _) = redact_for_persistence(&stdout_limited.text, &[]);
        let (stderr_redacted, _) = redact_for_persistence(&stderr_limited.text, &[]);
        let chat_id = extract_chat_id_heuristic(&stdout_text).or(resume_chat_hint);

        let resume = if timed_out {
            Some(ResumeMetadata::timeout_resume(
                chat_id.clone(),
                attempt_id,
                next_model,
                next_tier,
                !stdout_text.is_empty(),
            ))
        } else {
            None
        };
        if let Some(ref meta) = resume {
            record.resume_metadata = meta.to_value();
            record.chat_id = chat_id.clone();
            let _ = store.upsert_process(record);
            let _ = emit_process_event(
                store,
                record,
                "watchdog.timeout_resume",
                "Timeout resume metadata persisted for same-chat continuation",
                meta.to_value(),
            );
        }

        Ok(HostedProcessOutcome {
            process_id,
            exit_code,
            timed_out,
            cancelled,
            killed,
            stdout: stdout_redacted,
            stderr: stderr_redacted,
            duration_ms: started.elapsed().as_millis() as u64,
            chat_id,
            resume,
            cleanup_ok: false,
            zero_survivors: false,
            active_after_cleanup: u32::MAX,
        })
    }
}

fn force_terminate_live(host: &ProcessHost, process_id: Uuid) {
    if let Ok(lives) = host.lives.lock() {
        if let Some(live) = lives.get(&process_id) {
            let _ = live.spawned.job.terminate(1);
        }
    }
}

fn finalize_cleanup(
    store: &Store,
    record: &ProcessRecord,
    spawned: &SpawnedInJob,
    _was_stopped: bool,
) -> ProcessResult<(bool, u32, CleanupProof)> {
    // Observe active count WHILE job handle is still open.
    let active = spawned.job.active_process_count().unwrap_or(u32::MAX);
    if active > 0 {
        let _ = spawned.job.terminate(1);
        thread::sleep(Duration::from_millis(50));
    }
    let active_after = spawned.job.active_process_count().unwrap_or(u32::MAX);
    let zero = active_after == 0;
    let proof = CleanupProof {
        proof_id: Uuid::new_v4(),
        run_id: record.run_id,
        process_id: Some(record.process_id),
        observed_at_utc: chrono::Utc::now().to_rfc3339(),
        active_process_count: active_after,
        job_handle_open: true,
        handles_closed: false,
        zero_active_observed: zero,
        success: zero,
        detail: json!({
            "pid": spawned.pid,
            "association": match spawned.association {
                AssociationMethod::ProcThreadAttributeJobList => "proc_thread_attribute_job_list",
                AssociationMethod::SuspendedAssignDegraded => "suspended_assign_degraded",
            },
            "degradedAssociation": spawned.degraded_association,
            "observedBeforeClose": active_after,
        }),
    };
    store.insert_cleanup_proof(&proof)?;

    // Close process handle; job drops after this function (kill-on-close).
    #[cfg(windows)]
    unsafe {
        if spawned.process_handle != 0 {
            let h = windows::Win32::Foundation::HANDLE(spawned.process_handle as *mut _);
            let _ = windows::Win32::Foundation::CloseHandle(h);
        }
    }

    let mut proof_closed = proof.clone();
    proof_closed.handles_closed = true;
    proof_closed.proof_id = Uuid::new_v4();
    proof_closed.observed_at_utc = chrono::Utc::now().to_rfc3339();
    proof_closed.detail = json!({
        "handlesClosed": true,
        "zeroActiveObserved": zero,
    });
    store.insert_cleanup_proof(&proof_closed)?;

    let _ = emit_process_event(
        store,
        record,
        if zero {
            "cleanup.succeeded"
        } else {
            "cleanup.failed"
        },
        if zero {
            "Job Object reported zero active processes before handle close"
        } else {
            "Job Object still reported active processes"
        },
        json!({ "active": active_after }),
    );

    Ok((zero, active_after, proof_closed))
}

fn identity_checked_taskkill(pid: u32, creation_time_100ns: u64, executable_identity: &str) {
    if creation_time_100ns == 0 {
        return;
    }
    match identity::verify_live(pid, creation_time_100ns, executable_identity) {
        Ok(true) => {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output();
        }
        _ => {
            // Do not kill when identity cannot be verified.
        }
    }
}

fn extract_chat_id_heuristic(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(id) = v.get("session_id").and_then(|x| x.as_str()) {
                return Some(id.to_string());
            }
            if let Some(id) = v.pointer("/message/session_id").and_then(|x| x.as_str()) {
                return Some(id.to_string());
            }
        }
    }
    None
}

fn emit_process_event(
    store: &Store,
    record: &ProcessRecord,
    event_type: &str,
    message: &str,
    payload: serde_json::Value,
) -> ProcessResult<()> {
    let level = if event_type.contains("failed") || event_type.contains("forced") {
        EventLevel::Warning
    } else {
        EventLevel::Info
    };
    let _ = store.append_event_atomic(
        None,
        NewEvent {
            event_id: Uuid::new_v4(),
            run_id: record.run_id,
            project_id: None,
            phase_id: record.phase_id.clone(),
            attempt_id: record.attempt_id,
            process_id: Some(record.process_id),
            event_type: event_type.into(),
            level,
            timestamp_utc: chrono::Utc::now(),
            message: message.into(),
            payload,
        },
    )?;
    Ok(())
}

/// Run argv under ProcessHost and map to a simple capture for Cursor/executor callers.
pub fn run_capture_hosted(
    store: &Store,
    host: &ProcessHost,
    request: SpawnRequest,
) -> ProcessResult<crate::cursor::ProcessCapture> {
    let timeout_hint = request.watchdog.graceful_after_ms;
    let outcome = host.run_hosted(store, request)?;
    Ok(crate::cursor::ProcessCapture {
        exit_code: outcome.exit_code,
        stdout: outcome.stdout,
        stderr: outcome.stderr,
        timed_out: outcome.timed_out,
        duration_ms: if outcome.duration_ms == 0 {
            timeout_hint
        } else {
            outcome.duration_ms
        },
    })
}

/// Build watchdog timings from a caller timeout budget.
pub fn watchdog_for_timeout(timeout_ms: u64) -> WatchdogConfig {
    let timeout_ms = timeout_ms.max(50);
    WatchdogConfig {
        warn_after_ms: (timeout_ms.saturating_mul(8) / 10).max(1),
        graceful_after_ms: timeout_ms,
        force_grace_ms: (timeout_ms / 10).clamp(50, 15_000),
        drain_timeout_ms: 2_000.min(timeout_ms).max(100),
    }
}

/// Convenience: run argv under the process host with test watchdog timings.
pub fn run_argv_hosted_for_tests(
    store: &Store,
    host: &ProcessHost,
    run_id: Uuid,
    argv: Vec<String>,
    env: Vec<(String, String)>,
    watchdog: WatchdogConfig,
) -> ProcessResult<HostedProcessOutcome> {
    host.run_hosted(
        store,
        SpawnRequest {
            run_id,
            phase_id: Some("P07".into()),
            attempt_id: None,
            argv,
            stdin: Some(String::new()),
            workspace: None,
            env,
            watchdog,
            resume_chat_hint: Some("chat-timeout-fixture".into()),
            next_model_on_timeout: Some("cursor-grok-4.5-low".into()),
            next_tier_on_timeout: Some("grok-low".into()),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_id_extracted_from_stream_json() {
        let stdout = r#"{"type":"system","session_id":"chat-abc"}
{"type":"result"}
"#;
        assert_eq!(
            extract_chat_id_heuristic(stdout).as_deref(),
            Some("chat-abc")
        );
    }
}
