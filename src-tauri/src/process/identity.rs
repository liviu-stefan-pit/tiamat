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

#[cfg(not(windows))]
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

fn paths_equivalent(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
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

    #[test]
    fn path_equivalence_is_case_insensitive() {
        assert!(paths_equivalent(
            r"C:\Program Files\App\agent.exe",
            r"c:\program files\app\agent.exe"
        ));
    }
}
