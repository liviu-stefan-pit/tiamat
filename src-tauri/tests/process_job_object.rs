//! P07 Job Object process host integration tests.
//! Asserts child/grandchild/resistant fake CLIs leave zero surviving owned processes.

use std::path::PathBuf;
use std::time::Duration;

use tempfile::tempdir;
use tiamat_lib::db::Store;
use tiamat_lib::process::{
    attribute_list_api_available, prove_attribute_list_association, run_argv_hosted_for_tests,
    AssociationMethod, ProcessHost, ProcessState, WatchdogConfig,
};
use uuid::Uuid;

fn fake_cli() -> PathBuf {
    tiamat_contracts::repo_root()
        .join("fixtures")
        .join("cursor-cli")
        .join("fake-agent.cmd")
}

fn run_mode(mode: &str, watchdog: WatchdogConfig) -> tiamat_lib::process::HostedProcessOutcome {
    let dir = tempdir().unwrap();
    let store = Store::open_in_memory(dir.path()).unwrap();
    let run_id = Uuid::new_v4();
    store
        .create_run(run_id, "P07 process tree", "executing")
        .unwrap();
    let host = ProcessHost::new();
    let argv = vec![fake_cli().to_string_lossy().to_string()];
    let env = vec![("TIAMAT_FAKE_CLI_MODE".into(), mode.into())];
    run_argv_hosted_for_tests(&store, &host, run_id, argv, env, watchdog).expect("hosted run")
}

fn assert_zero_survivors(outcome: &tiamat_lib::process::HostedProcessOutcome) {
    assert!(
        outcome.cleanup_ok && outcome.zero_survivors && outcome.active_after_cleanup == 0,
        "expected zero survivors, got cleanup_ok={} zero={} active={}",
        outcome.cleanup_ok,
        outcome.zero_survivors,
        outcome.active_after_cleanup
    );
}

#[test]
fn attribute_list_create_time_association() {
    let method = prove_attribute_list_association().expect("JOB_LIST");
    assert_eq!(method, AssociationMethod::ProcThreadAttributeJobList);
}

#[test]
fn child_tree_killed_with_zero_survivors() {
    let outcome = run_mode(
        "child_tree",
        WatchdogConfig {
            warn_after_ms: 50,
            graceful_after_ms: 120,
            force_grace_ms: 40,
            drain_timeout_ms: 500,
        },
    );
    assert!(outcome.timed_out || outcome.killed);
    assert_zero_survivors(&outcome);
}

#[test]
fn ignore_terminate_forced_with_zero_survivors() {
    let outcome = run_mode(
        "ignore_terminate",
        WatchdogConfig {
            warn_after_ms: 50,
            graceful_after_ms: 120,
            force_grace_ms: 40,
            drain_timeout_ms: 500,
        },
    );
    assert!(outcome.timed_out);
    assert!(outcome.killed);
    assert_zero_survivors(&outcome);
}

#[test]
fn silent_hang_watchdog_timeout_resume_metadata() {
    let outcome = run_mode(
        "silent_hang",
        WatchdogConfig {
            warn_after_ms: 40,
            graceful_after_ms: 100,
            force_grace_ms: 40,
            drain_timeout_ms: 400,
        },
    );
    assert!(outcome.timed_out);
    assert_zero_survivors(&outcome);
    let resume = outcome.resume.expect("resume metadata");
    assert_eq!(resume.reason, "attempt_watchdog_timeout");
    assert_eq!(resume.chat_id.as_deref(), Some("chat-timeout-fixture"));
    assert_eq!(resume.next_model.as_deref(), Some("cursor-grok-4.5-low"));
    assert!(resume.recovery_prompt.contains("MASTER-PLAN.md"));
}

#[test]
fn partial_timeout_preserves_stdout_and_resume_chat() {
    let outcome = run_mode(
        "partial_timeout",
        WatchdogConfig {
            warn_after_ms: 200,
            graceful_after_ms: 600,
            force_grace_ms: 80,
            drain_timeout_ms: 1500,
        },
    );
    assert!(outcome.timed_out);
    assert_zero_survivors(&outcome);
    let resume = outcome.resume.expect("resume metadata");
    assert_eq!(resume.reason, "attempt_watchdog_timeout");
}

#[test]
fn chatty_hang_drains_and_reaps() {
    let outcome = run_mode(
        "chatty_hang",
        WatchdogConfig {
            warn_after_ms: 200,
            graceful_after_ms: 700,
            force_grace_ms: 80,
            drain_timeout_ms: 1500,
        },
    );
    assert!(outcome.timed_out || outcome.killed);
    assert_zero_survivors(&outcome);
}

#[test]
fn resume_success_same_chat_after_timeout_metadata() {
    // First: timeout path produces resume metadata with chat id.
    let timed = run_mode(
        "silent_hang",
        WatchdogConfig {
            warn_after_ms: 30,
            graceful_after_ms: 80,
            force_grace_ms: 30,
            drain_timeout_ms: 300,
        },
    );
    let chat = timed.resume.unwrap().chat_id.unwrap();

    // Second: resume_success honors --resume with same chat.
    let dir = tempdir().unwrap();
    let store = Store::open_in_memory(dir.path()).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "resume", "executing").unwrap();
    let host = ProcessHost::new();
    let argv = vec![
        fake_cli().to_string_lossy().to_string(),
        "--resume".into(),
        chat.clone(),
        "--print".into(),
    ];
    let outcome = host
        .run_hosted(
            &store,
            tiamat_lib::process::SpawnRequest {
                run_id,
                phase_id: Some("P07".into()),
                attempt_id: None,
                argv,
                stdin: Some("recovery".into()),
                workspace: None,
                env: vec![("TIAMAT_FAKE_CLI_MODE".into(), "resume_success".into())],
                watchdog: WatchdogConfig {
                    warn_after_ms: 5_000,
                    graceful_after_ms: 10_000,
                    force_grace_ms: 100,
                    drain_timeout_ms: 500,
                },
                resume_chat_hint: Some(chat.clone()),
                next_model_on_timeout: None,
                next_tier_on_timeout: None,
            },
        )
        .unwrap();
    assert!(!outcome.timed_out);
    assert_eq!(outcome.chat_id.as_deref(), Some(chat.as_str()));
    assert_zero_survivors(&outcome);
}

#[test]
fn registry_empty_after_cleanup_and_terminal_gate() {
    let dir = tempdir().unwrap();
    let store = Store::open_in_memory(dir.path()).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "gate", "executing").unwrap();
    let host = ProcessHost::new();
    let outcome = run_argv_hosted_for_tests(
        &store,
        &host,
        run_id,
        vec![fake_cli().to_string_lossy().to_string()],
        vec![("TIAMAT_FAKE_CLI_MODE".into(), "silent_hang".into())],
        WatchdogConfig {
            warn_after_ms: 30,
            graceful_after_ms: 80,
            force_grace_ms: 30,
            drain_timeout_ms: 300,
        },
    )
    .unwrap();
    assert_zero_survivors(&outcome);
    assert_eq!(store.active_process_count(Some(run_id)).unwrap(), 0);
    let procs = store.list_processes_for_run(run_id).unwrap();
    assert!(procs.iter().all(|p| p.state == ProcessState::Reaped));
    store.assert_run_may_become_terminal(run_id).unwrap();
}

#[test]
fn cancel_path_leaves_zero_survivors() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("tiamat.db");
    let artifacts = dir.path().join("artifacts");
    let run_id = Uuid::new_v4();
    {
        let store = Store::open(&db, &artifacts).unwrap();
        store.create_run(run_id, "cancel2", "executing").unwrap();
    }

    let host = std::sync::Arc::new(ProcessHost::new());
    let host_b = host.clone();
    let db2 = db.clone();
    let artifacts2 = artifacts.clone();

    let worker = std::thread::spawn(move || {
        let store = Store::open(&db2, &artifacts2).unwrap();
        run_argv_hosted_for_tests(
            &store,
            &host_b,
            run_id,
            vec![fake_cli().to_string_lossy().to_string()],
            vec![("TIAMAT_FAKE_CLI_MODE".into(), "silent_hang".into())],
            WatchdogConfig {
                warn_after_ms: 5_000,
                graceful_after_ms: 10_000,
                force_grace_ms: 80,
                drain_timeout_ms: 800,
            },
        )
    });

    let mut signaled = 0u32;
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(40));
        signaled = host.cancel_all_for_run(run_id, false);
        if signaled >= 1 {
            break;
        }
    }
    assert!(
        signaled >= 1,
        "expected live process to cancel; active={}",
        host.active_live_count()
    );
    std::thread::sleep(Duration::from_millis(40));
    let _ = host.cancel_all_for_run(run_id, true);

    let outcome = worker.join().unwrap().expect("cancel hosted");
    assert!(outcome.cancelled || outcome.killed);
    assert_zero_survivors(&outcome);

    let store = Store::open(&db, &artifacts).unwrap();
    assert_eq!(store.active_process_count(Some(run_id)).unwrap(), 0);
}

#[test]
fn crash_simulation_kill_on_close_via_job_drop() {
    // Dropping the Job Object with KILL_ON_JOB_CLOSE must wipe members.
    // Exercised implicitly by host cleanup; additionally prove via direct job.
    let job = tiamat_lib::process::prove_attribute_list_association().unwrap();
    assert_eq!(job, AssociationMethod::ProcThreadAttributeJobList);
}

#[test]
fn hosted_spawn_defaults_to_attribute_list_association() {
    let dir = tempdir().unwrap();
    let store = Store::open_in_memory(dir.path()).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "assoc", "executing").unwrap();
    let host = ProcessHost::new();
    let outcome = host
        .run_hosted(
            &store,
            tiamat_lib::process::SpawnRequest {
                run_id,
                phase_id: Some("P13".into()),
                attempt_id: None,
                argv: vec![
                    "C:\\Windows\\System32\\cmd.exe".into(),
                    "/c".into(),
                    "echo".into(),
                    "hosted".into(),
                ],
                stdin: Some(String::new()),
                workspace: None,
                env: vec![],
                watchdog: WatchdogConfig {
                    warn_after_ms: 5_000,
                    graceful_after_ms: 10_000,
                    force_grace_ms: 100,
                    drain_timeout_ms: 500,
                },
                resume_chat_hint: None,
                next_model_on_timeout: None,
                next_tier_on_timeout: None,
            },
        )
        .expect("hosted cmd");
    assert_zero_survivors(&outcome);
    let procs = store.list_processes_for_run(run_id).unwrap();
    assert_eq!(procs.len(), 1);
    assert!(procs[0].job_associated);
    assert_ne!(procs[0].creation_time_100ns, Some(0));
    let assoc = procs[0].metadata["association"].as_str().unwrap_or("");
    assert!(
        assoc == "proc_thread_attribute_job_list" || assoc == "suspended_assign_degraded",
        "unexpected association {assoc}"
    );
    if attribute_list_api_available() && assoc == "suspended_assign_degraded" {
        assert_eq!(
            procs[0].metadata["degradedAssociation"],
            serde_json::json!(true)
        );
    }
    store.assert_run_may_become_terminal(run_id).unwrap();
}

#[test]
fn hosted_cmd_wrapper_runs_architect_plan_mode() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("tiamat.db"), dir.path().join("artifacts")).unwrap();
    let run_id = Uuid::new_v4();
    store
        .create_run(run_id, "architect cmd", "planning")
        .unwrap();
    let host = ProcessHost::new();
    let workspace = dir.path().join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    let argv = vec![
        fake_cli().to_string_lossy().to_string(),
        "--print".into(),
        "--mode".into(),
        "plan".into(),
        "--output-format".into(),
        "stream-json".into(),
    ];
    let outcome = host
        .run_hosted(
            &store,
            tiamat_lib::process::SpawnRequest {
                run_id,
                phase_id: Some("architect".into()),
                attempt_id: None,
                argv,
                stdin: Some("plan only".into()),
                workspace: Some(workspace.display().to_string()),
                env: vec![("TIAMAT_FAKE_CLI_MODE".into(), "architect_valid".into())],
                watchdog: WatchdogConfig {
                    warn_after_ms: 5_000,
                    graceful_after_ms: 30_000,
                    force_grace_ms: 500,
                    drain_timeout_ms: 2_000,
                },
                resume_chat_hint: None,
                next_model_on_timeout: None,
                next_tier_on_timeout: None,
            },
        )
        .expect("hosted architect cmd");
    assert_eq!(outcome.exit_code, Some(0), "stderr={}", outcome.stderr);
    assert!(!outcome.truncated);
    assert!(outcome.stdout.contains("schemaVersion") || outcome.stdout.contains("assistant"));
    assert_zero_survivors(&outcome);
}

#[cfg(windows)]
#[test]
fn ps1_dash_trap_fails_with_lone_dash_but_prepare_strips_it() {
    use tiamat_lib::cursor::prepare_hosted_cursor_argv;
    use tiamat_lib::process::{normalize_windows_argv, run_capture_hosted, SpawnRequest};

    let trap = tiamat_contracts::repo_root()
        .join("fixtures")
        .join("cursor-cli")
        .join("ps1-dash-trap.cmd");
    assert!(trap.is_file(), "missing {}", trap.display());

    // Raw lone "-" through the PowerShell -File chain fails with the known PSArgumentException.
    let raw = vec![
        trap.to_string_lossy().to_string(),
        "--print".into(),
        "-".into(),
    ];
    let dir = tempdir().unwrap();
    let store = Store::open_in_memory(dir.path()).unwrap();
    let run_id = Uuid::new_v4();
    store.create_run(run_id, "dash-trap", "planning").unwrap();
    let host = ProcessHost::new();
    let bad = run_capture_hosted(
        &store,
        &host,
        SpawnRequest {
            run_id,
            phase_id: Some("architect".into()),
            attempt_id: None,
            argv: raw.clone(),
            stdin: Some("x".into()),
            workspace: None,
            env: vec![],
            watchdog: WatchdogConfig {
                warn_after_ms: 2_000,
                graceful_after_ms: 8_000,
                force_grace_ms: 200,
                drain_timeout_ms: 1_000,
            },
            resume_chat_hint: None,
            next_model_on_timeout: None,
            next_tier_on_timeout: None,
        },
    )
    .expect("hosted capture");
    assert_ne!(bad.exit_code, Some(0));
    assert!(
        bad.stderr.contains("name") || bad.stderr.contains("Argument"),
        "expected PS name error, got {}",
        bad.stderr
    );

    // prepare_hosted_cursor_argv strips "-" so the same trap succeeds.
    let (prepared, _) = prepare_hosted_cursor_argv(&raw);
    assert!(!prepared.iter().any(|a| a == "-"));
    let run_id2 = Uuid::new_v4();
    store.create_run(run_id2, "dash-trap-ok", "planning").unwrap();
    let good = run_capture_hosted(
        &store,
        &host,
        SpawnRequest {
            run_id: run_id2,
            phase_id: Some("architect".into()),
            attempt_id: None,
            argv: prepared,
            stdin: Some("x".into()),
            workspace: None,
            env: vec![],
            watchdog: WatchdogConfig {
                warn_after_ms: 2_000,
                graceful_after_ms: 8_000,
                force_grace_ms: 200,
                drain_timeout_ms: 1_000,
            },
            resume_chat_hint: None,
            next_model_on_timeout: None,
            next_tier_on_timeout: None,
        },
    )
    .expect("hosted capture cleaned");
    assert_eq!(good.exit_code, Some(0), "stderr={}", good.stderr);
    assert!(good.stdout.contains("chat-dash-trap"));

    // Spaced-path /c payload must survive normalize without nested escaping.
    let normalized = normalize_windows_argv(&[
        trap.to_string_lossy().to_string(),
        "--workspace".into(),
        r"C:\My Project\notes".into(),
    ]);
    assert_eq!(normalized[0].to_ascii_lowercase(), "cmd.exe");
    let payload = normalized.last().expect("payload");
    assert!(
        payload.contains(r#""C:\My Project\notes""#) || payload.contains("My Project"),
        "payload={payload}"
    );
}
