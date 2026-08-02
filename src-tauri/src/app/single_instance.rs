//! Windows named single-instance mutex. Second instance fails cleanly at startup.

/// Opaque guard that keeps the process-wide single-instance lock held until drop.
pub struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
}

/// Named mutex used for Tiamat desktop single-instance enforcement.
pub const SINGLE_INSTANCE_MUTEX_NAME: &str = "Local\\Tiamat.Orchestrator.SingleInstance";

/// Acquire the application single-instance mutex.
///
/// On Windows, a second process that finds the mutex already held returns `Err`.
/// Non-Windows platforms always succeed (no-op guard).
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

#[cfg(not(windows))]
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
    #[cfg(not(windows))]
    fn non_windows_acquire_is_noop_ok() {
        assert!(acquire_single_instance_mutex().is_ok());
    }
}
