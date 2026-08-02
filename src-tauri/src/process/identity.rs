//! PID + creation-time + executable identity checks (Windows PID reuse defense).

use super::error::{ProcessError, ProcessResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub creation_time_100ns: u64,
    pub executable_path: String,
}

#[cfg(windows)]
pub fn query_identity(pid: u32) -> ProcessResult<ProcessIdentity> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(|e| {
            ProcessError::Identity(format!("OpenProcess({pid}) for identity failed: {e}"))
        })?;

        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user).is_err() {
            let _ = CloseHandle(handle);
            return Err(ProcessError::Identity(format!(
                "GetProcessTimes({pid}) failed"
            )));
        }

        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let path = match QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        ) {
            Ok(()) => String::from_utf16_lossy(&buf[..size as usize]),
            Err(e) => {
                let _ = CloseHandle(handle);
                return Err(ProcessError::Identity(format!(
                    "QueryFullProcessImageNameW({pid}) failed: {e}"
                )));
            }
        };
        let _ = CloseHandle(handle);

        let creation_time_100ns =
            ((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64);

        Ok(ProcessIdentity {
            pid,
            creation_time_100ns,
            executable_path: path,
        })
    }
}

/// Unix identity: PID plus the kernel's start time, which makes a recycled PID
/// distinguishable from the original process.
#[cfg(unix)]
pub fn query_identity(pid: u32) -> ProcessResult<ProcessIdentity> {
    let creation_time_100ns = query_start_time(pid).ok_or_else(|| {
        ProcessError::Identity(format!("could not read /proc/{pid}/stat for identity"))
    })?;
    let executable_path = std::fs::read_link(format!("/proc/{pid}/exe"))
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    Ok(ProcessIdentity {
        pid,
        creation_time_100ns,
        executable_path,
    })
}

/// Process start time in clock ticks since boot (field 22 of `/proc/<pid>/stat`).
/// Used as the creation-time discriminator; the unit differs from Windows but the
/// only requirement is that it is stable per process and differs across PID reuse.
#[cfg(unix)]
pub fn query_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The comm field is parenthesised and may contain spaces, so split after the last ')'.
    let (_, rest) = stat.rsplit_once(") ")?;
    rest.split_whitespace().nth(19)?.parse::<u64>().ok()
}

#[cfg(not(unix))]
pub fn query_start_time(_pid: u32) -> Option<u64> {
    None
}

#[cfg(not(any(windows, unix)))]
pub fn query_identity(pid: u32) -> ProcessResult<ProcessIdentity> {
    Err(ProcessError::Unsupported(format!(
        "process identity query unsupported for pid {pid}"
    )))
}

pub fn identities_match(expected: &ProcessIdentity, observed: &ProcessIdentity) -> bool {
    expected.pid == observed.pid
        && expected.creation_time_100ns == observed.creation_time_100ns
        && paths_equivalent(&expected.executable_path, &observed.executable_path)
}

/// Windows paths are case-insensitive and separator-agnostic; Unix paths are neither.
fn paths_equivalent(a: &str, b: &str) -> bool {
    #[cfg(windows)]
    let norm = |s: &str| {
        s.replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
    #[cfg(not(windows))]
    let norm = |s: &str| s.trim_end_matches('/').to_string();
    norm(a) == norm(b)
}

/// Verify a live PID still matches the durable registry identity.
pub fn verify_live(
    pid: u32,
    creation_time_100ns: u64,
    executable_identity: &str,
) -> ProcessResult<bool> {
    let live = match query_identity(pid) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let expected = ProcessIdentity {
        pid,
        creation_time_100ns,
        executable_path: executable_identity.to_string(),
    };
    Ok(identities_match(&expected, &live))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn path_equivalence_is_case_insensitive() {
        assert!(paths_equivalent(
            r"C:\Program Files\App\agent.exe",
            r"c:\program files\app\agent.exe"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn path_equivalence_is_case_sensitive_on_unix() {
        assert!(paths_equivalent("/usr/bin/agent", "/usr/bin/agent/"));
        assert!(!paths_equivalent("/usr/bin/Agent", "/usr/bin/agent"));
    }

    #[cfg(unix)]
    #[test]
    fn start_time_is_stable_for_current_process() {
        let pid = std::process::id();
        let first = query_start_time(pid).expect("own start time readable");
        let second = query_start_time(pid).expect("own start time readable");
        assert_eq!(first, second);
    }
}
