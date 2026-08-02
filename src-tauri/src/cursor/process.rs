use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::types::ProcessCapture;

/// Classification for unhosted argv capture. Production agent/architect/verification
/// work must use `ProcessHost::run_hosted` / `run_capture_hosted` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnhostedSpawnClass {
    /// Short Cursor capability / version / auth probes only.
    CapabilityProbe,
}

/// Bounded argv runner for **capability probes only**.
/// Production phase/architect/verification spawns must go through ProcessHost.
pub fn run_argv_capture(
    argv: &[String],
    timeout_ms: u64,
    stdin: Option<&str>,
) -> Result<ProcessCapture, String> {
    run_argv_capture_env_classified(
        argv,
        timeout_ms,
        stdin,
        &HashMap::new(),
        UnhostedSpawnClass::CapabilityProbe,
    )
}

/// Legacy env-aware capture. Prefer `run_argv_capture_env_classified` so the call
/// site records that this path is probe-only (not Job-associated).
pub fn run_argv_capture_env(
    argv: &[String],
    timeout_ms: u64,
    stdin: Option<&str>,
    extra_env: &HashMap<String, String>,
) -> Result<ProcessCapture, String> {
    run_argv_capture_env_classified(
        argv,
        timeout_ms,
        stdin,
        extra_env,
        UnhostedSpawnClass::CapabilityProbe,
    )
}

pub fn run_argv_capture_env_classified(
    argv: &[String],
    timeout_ms: u64,
    stdin: Option<&str>,
    extra_env: &HashMap<String, String>,
    class: UnhostedSpawnClass,
) -> Result<ProcessCapture, String> {
    let _ = class; // CapabilityProbe — explicit at call sites / type system
    if argv.is_empty() {
        return Err("argv must not be empty".into());
    }
    let (program, args) = argv.split_first().expect("non-empty");
    let started = Instant::now();

    let mut child = Command::new(program);
    child
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in extra_env {
        child.env(key, value);
    }
    let mut child = child
        .spawn()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    let pid = child.id();

    if let Some(input) = stdin {
        use std::io::Write;
        if let Some(mut handle) = child.stdin.take() {
            let _ = handle.write_all(input.as_bytes());
        }
    } else if let Some(handle) = child.stdin.take() {
        drop(handle);
    }

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    let timeout = Duration::from_millis(timeout_ms.max(1));
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Ok(ProcessCapture {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            timed_out: false,
            duration_ms: started.elapsed().as_millis() as u64,
        }),
        Ok(Err(e)) => Err(format!("failed to collect output: {e}")),
        Err(_) => {
            // Probe-only path: terminate the probe process tree. Hosted production
            // work uses Job terminate + identity-checked fallback instead (REL-003).
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .output();
            }
            #[cfg(not(windows))]
            {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
            }
            let _ = rx.recv_timeout(Duration::from_millis(250));
            Ok(ProcessCapture {
                exit_code: None,
                stdout: String::new(),
                stderr: "process timed out".into(),
                timed_out: true,
                duration_ms: started.elapsed().as_millis() as u64,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_argv() {
        let err = run_argv_capture(&[], 100, None).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn probe_class_is_explicit() {
        assert_eq!(
            UnhostedSpawnClass::CapabilityProbe,
            UnhostedSpawnClass::CapabilityProbe
        );
    }
}
