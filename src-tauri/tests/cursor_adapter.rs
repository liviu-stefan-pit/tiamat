use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tiamat_lib::cursor::builder::{build_cursor_command, preview_built_command};
use tiamat_lib::cursor::probe::{invalidate_probe_cache, probe_with_deps};
use tiamat_lib::cursor::stream::parse_stream_json;
use tiamat_lib::cursor::types::{
    CursorAuthStatus, CursorCapabilityStatus, CursorFeatureFlags, CursorInvokeRequest,
    ProcessCapture,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn fake_agent_js() -> PathBuf {
    repo_root().join("fixtures/cursor-cli/fake-agent.mjs")
}

fn run_fake(mode: &str, extra: &[&str], timeout_ms: u64, stdin: Option<&str>) -> ProcessCapture {
    use std::sync::mpsc;

    let started = Instant::now();
    let mut child = Command::new("node")
        .arg(fake_agent_js())
        .args(extra)
        .env("TIAMAT_FAKE_CLI_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fake agent");
    let pid = child.id();
    if let Some(input) = stdin {
        use std::io::Write;
        if let Some(mut handle) = child.stdin.take() {
            let _ = handle.write_all(input.as_bytes());
        }
    } else {
        drop(child.stdin.take());
    }

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(Ok(output)) => ProcessCapture {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            timed_out: false,
            duration_ms: started.elapsed().as_millis() as u64,
        },
        Ok(Err(e)) => panic!("failed to collect output: {e}"),
        Err(_) => {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output();
            let drained = rx.recv_timeout(Duration::from_millis(500)).ok();
            let (stdout, stderr) = match drained {
                Some(Ok(output)) => (
                    String::from_utf8_lossy(&output.stdout).into_owned(),
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                ),
                _ => (String::new(), "process timed out".into()),
            };
            ProcessCapture {
                exit_code: None,
                stdout,
                stderr,
                timed_out: true,
                duration_ms: started.elapsed().as_millis() as u64,
            }
        }
    }
}

fn probe_fake(mode: &str) -> tiamat_lib::cursor::CursorCapabilityReport {
    invalidate_probe_cache();
    let js = fake_agent_js();
    let mode_owned = mode.to_string();
    let js_for_run = js.clone();
    let run = move |argv: &[String], _timeout_ms: u64| {
        let rest: Vec<String> = if argv.len() > 1 {
            argv[1..].to_vec()
        } else {
            Vec::new()
        };
        let output = Command::new("node")
            .arg(&js_for_run)
            .args(&rest)
            .env("TIAMAT_FAKE_CLI_MODE", &mode_owned)
            .output()
            .map_err(|e| e.to_string())?;
        Ok((
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    };
    let mut env = HashMap::new();
    env.insert("TIAMAT_CURSOR_CLI".into(), format!("node|{}", js.display()));
    probe_with_deps(None, &env, &|_| None, &run)
}

#[test]
fn fake_modes_matrix_covers_all_behaviors() {
    let ok = run_fake("success", &["--print"], 5_000, Some("prompt"));
    assert_eq!(ok.exit_code, Some(0));
    let parsed = parse_stream_json(&ok.stdout, &ok.stderr, &[]);
    assert_eq!(parsed.chat_id.as_deref(), Some("chat-fake-001"));
    assert!(parsed.usage.is_some());

    let fail = run_fake("nonzero_exit", &["--print"], 5_000, None);
    assert_ne!(fail.exit_code, Some(0));

    let mixed = run_fake("malformed_mixed", &["--print"], 5_000, None);
    let parsed = parse_stream_json(&mixed.stdout, &mixed.stderr, &[]);
    assert!(!parsed.diagnostics.is_empty());
    assert_eq!(parsed.chat_id.as_deref(), Some("chat-mixed"));

    let hang = run_fake("silent_hang", &["--print"], 400, None);
    assert!(hang.timed_out);

    let chatty = run_fake("chatty_hang", &["--print"], 400, None);
    assert!(chatty.timed_out || !chatty.stdout.is_empty());

    let child = run_fake("child_tree", &["--print"], 400, None);
    assert!(child.timed_out || child.stdout.contains("chat-child"));

    let ignore = run_fake("ignore_terminate", &["--print"], 400, None);
    assert!(ignore.timed_out);

    let partial = run_fake("partial_timeout", &["--print"], 400, None);
    assert!(partial.timed_out || partial.stdout.contains("partial"));

    let resume = run_fake(
        "resume_success",
        &["--print", "--resume", "chat-resume-9"],
        5_000,
        Some("continue"),
    );
    assert_eq!(resume.exit_code, Some(0));
    let parsed = parse_stream_json(&resume.stdout, &resume.stderr, &[]);
    assert_eq!(parsed.chat_id.as_deref(), Some("chat-resume-9"));

    let unavailable = run_fake(
        "model_unavailable",
        &["--print", "--model", "nope-model"],
        5_000,
        None,
    );
    assert_ne!(unavailable.exit_code, Some(0));
    assert!(unavailable.stderr.to_lowercase().contains("unavailable"));

    let auth = run_fake("auth_failure", &["--print"], 5_000, None);
    assert_eq!(auth.exit_code, Some(3));
    assert!(auth.stderr.to_lowercase().contains("auth"));

    let flood = run_fake("flood_oversized", &["--print"], 10_000, None);
    assert_eq!(flood.exit_code, Some(0));
    assert!(flood.stdout.len() > 100_000);

    let secret = run_fake("secret_echo", &["--print"], 5_000, None);
    let parsed = parse_stream_json(&secret.stdout, &secret.stderr, &["fixture-secret-value"]);
    let joined = parsed
        .events
        .iter()
        .map(|e| e.redacted_line.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!joined.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(!joined.contains("fixture-secret-value"));
}

#[test]
fn probe_against_fake_cli_reports_features_models_and_auth() {
    let report = probe_fake("success");
    assert_eq!(report.status, CursorCapabilityStatus::Available);
    assert_eq!(report.version.as_deref(), Some("1.2.3"));
    assert!(report.features.print_mode);
    assert!(report.features.stream_json);
    assert!(report.features.resume);
    assert!(report.features.has_noninteractive_approval());
    assert!(report.models.iter().any(|m| m.id == "composer-2.5"));
    assert_eq!(report.auth, CursorAuthStatus::Ready);
}

#[test]
fn probe_auth_failure_mode() {
    let report = probe_fake("auth_failure");
    assert_eq!(report.status, CursorCapabilityStatus::Available);
    assert_eq!(report.auth, CursorAuthStatus::Unauthenticated);
}

#[test]
fn builder_preview_never_shell_concatenates_or_leaks_secrets() {
    let features = CursorFeatureFlags {
        print_mode: true,
        output_format: true,
        stream_json: true,
        workspace: true,
        model: true,
        force: true,
        trust: true,
        api_key: true,
        resume: true,
        ..CursorFeatureFlags::default()
    };

    let root = tempfile::tempdir().unwrap();
    let request = CursorInvokeRequest {
        workspace: root.path().display().to_string(),
        model: Some("composer-2.5".into()),
        prompt: "use key fixture-secret-value".into(),
        api_key: Some("super-secret-key".into()),
        force: true,
        trust: true,
        ..CursorInvokeRequest::default()
    };
    let built = build_cursor_command("agent", &features, &request, Some(root.path())).unwrap();
    assert!(built.argv.iter().all(|a| !a.contains(" && ")));
    let preview = preview_built_command(&built, &["fixture-secret-value"]);
    assert!(!preview.spawned);
    assert!(!preview.command_display.contains("super-secret-key"));
    assert!(!preview.stdin_preview.contains("fixture-secret-value"));
    assert!(!preview.stdin_preview.contains("super-secret-key"));
}

#[test]
fn unavailable_model_list_mode_surfaces_error() {
    let capture = run_fake("model_unavailable", &["--list-models"], 5_000, None);
    assert_ne!(capture.exit_code, Some(0));
}
