//! Single-instance enforcement: a named mutex on Windows, an advisory file lock on Unix.
//! A second instance fails cleanly at startup on both.

/// Opaque guard that keeps the process-wide single-instance lock held until drop.
pub struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    _lock_file: std::fs::File,
}

/// Named mutex used for Tiamat desktop single-instance enforcement.
pub const SINGLE_INSTANCE_MUTEX_NAME: &str = "Local\\Tiamat.Orchestrator.SingleInstance";

/// Acquire the application single-instance lock.
///
/// A second process that finds the lock already held returns `Err`.
pub fn acquire_single_instance_mutex() -> Result<SingleInstanceGuard, String> {
    acquire_named_mutex(SINGLE_INSTANCE_MUTEX_NAME)
}

#[cfg(windows)]
fn acquire_named_mutex(name: &str) -> Result<SingleInstanceGuard, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;

    let wide: Vec<u16> = std::ffi::OsStr::new(name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: null security attributes; name is NUL-terminated wide string we own for the call.
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(wide.as_ptr())) }
        .map_err(|err| format!("failed to create single-instance mutex '{name}': {err}"))?;

    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(format!(
            "Tiamat is already running (single-instance mutex '{name}' held). Exiting."
        ));
    }

    Ok(SingleInstanceGuard { handle })
}

/// Unix: an exclusive `flock` on a file under the runtime/temp dir. The kernel
/// releases it if we crash, so a stale lock never blocks the next launch.
#[cfg(unix)]
fn acquire_named_mutex(name: &str) -> Result<SingleInstanceGuard, String> {
    use std::os::unix::io::AsRawFd;

    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    // XDG_RUNTIME_DIR is preferred but is not always actually present (containers,
    // minimal sessions), so fall back to the temp dir rather than failing to launch.
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let path = dir.join(format!("{slug}.lock"));

    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|err| format!("failed to open single-instance lock {path:?}: {err}"))?;

    // SAFETY: fd is owned by `file`, which outlives the call and the returned guard.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!(
            "Tiamat is already running (single-instance lock {path:?} held). Exiting."
        ));
    }

    Ok(SingleInstanceGuard { _lock_file: file })
}

#[cfg(not(any(windows, unix)))]
fn acquire_named_mutex(_name: &str) -> Result<SingleInstanceGuard, String> {
    Ok(SingleInstanceGuard {})
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::CloseHandle;
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
        // Unix: closing the file descriptor releases the flock.
    }
}

// SAFETY: HANDLE is process-scoped and only closed on Drop; we never share mutable access.
#[cfg(windows)]
unsafe impl Send for SingleInstanceGuard {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn second_acquire_of_same_name_fails_cleanly() {
        let name = format!("Local\\Tiamat.Test.SingleInstance.{}", std::process::id());
        let first = acquire_named_mutex(&name).expect("first acquire");
        let second = acquire_named_mutex(&name);
        let Err(msg) = second else {
            panic!("second instance must fail");
        };
        assert!(
            msg.contains("already running") || msg.contains("single-instance"),
            "unexpected message: {msg}"
        );
        drop(first);
        // After release, a new acquire should succeed.
        let third = acquire_named_mutex(&name).expect("acquire after release");
        drop(third);
    }

    #[test]
    #[cfg(unix)]
    fn second_acquire_of_same_lock_file_fails_cleanly() {
        let name = format!("Tiamat.Test.SingleInstance.{}", std::process::id());
        let first = acquire_named_mutex(&name).expect("first acquire");
        let Err(msg) = acquire_named_mutex(&name) else {
            panic!("second instance must fail");
        };
        assert!(msg.contains("already running"), "unexpected message: {msg}");
        drop(first);
        let third = acquire_named_mutex(&name).expect("acquire after release");
        drop(third);
    }
}
