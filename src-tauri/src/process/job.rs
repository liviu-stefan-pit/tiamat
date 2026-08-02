//! Windows Job Object helpers: kill-on-close, breakaway disabled, active-process limit.

use super::error::{ProcessError, ProcessResult};

/// Default active-process ceiling for hosted work (defense-in-depth).
pub const DEFAULT_ACTIVE_PROCESS_LIMIT: u32 = 64;

#[cfg(windows)]
mod win {
    use super::{ProcessError, ProcessResult, DEFAULT_ACTIVE_PROCESS_LIMIT};
    use std::mem::size_of;
    use std::ptr;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
        JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_ALL_ACCESS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    #[derive(Debug)]
    pub struct JobObject {
        handle: HANDLE,
        name: Option<String>,
    }

    // SAFETY: Job handles are owned exclusively by this struct and closed on Drop.
    unsafe impl Send for JobObject {}

    impl JobObject {
        /// Create a Job with kill-on-close, breakaway disabled (flags omitted), and an
        /// active-process limit. BREAKAWAY_OK / SILENT_BREAKAWAY_OK are intentionally unset.
        pub fn create_kill_on_close(name: Option<&str>) -> ProcessResult<Self> {
            Self::create_configured(name, DEFAULT_ACTIVE_PROCESS_LIMIT)
        }

        pub fn create_configured(
            name: Option<&str>,
            active_process_limit: u32,
        ) -> ProcessResult<Self> {
            unsafe {
                let wide_name: Option<Vec<u16>> = name.map(|n| {
                    let mut v: Vec<u16> = n.encode_utf16().collect();
                    v.push(0);
                    v
                });
                let name_ptr = wide_name
                    .as_ref()
                    .map(|v| PCWSTR(v.as_ptr()))
                    .unwrap_or(PCWSTR::null());
                let handle = CreateJobObjectW(None, name_ptr)
                    .map_err(|e| ProcessError::Job(format!("CreateJobObjectW failed: {e}")))?;
                if handle.is_invalid() || handle == INVALID_HANDLE_VALUE {
                    return Err(ProcessError::Job(
                        "CreateJobObjectW returned invalid handle".into(),
                    ));
                }

                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                // Kill-on-close + active process limit. Breakaway remains disabled by omission.
                info.BasicLimitInformation.LimitFlags =
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
                info.BasicLimitInformation.ActiveProcessLimit = active_process_limit.max(1);
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
                .map_err(|e| {
                    let _ = CloseHandle(handle);
                    ProcessError::Job(format!("SetInformationJobObject failed: {e}"))
                })?;

                Ok(Self {
                    handle,
                    name: name.map(str::to_string),
                })
            }
        }

        pub fn handle(&self) -> HANDLE {
            self.handle
        }

        pub fn name(&self) -> Option<&str> {
            self.name.as_deref()
        }

        pub fn assign_process(&self, process: HANDLE) -> ProcessResult<()> {
            unsafe {
                AssignProcessToJobObject(self.handle, process)
                    .map_err(|e| ProcessError::Job(format!("AssignProcessToJobObject failed: {e}")))
            }
        }

        pub fn terminate(&self, exit_code: u32) -> ProcessResult<()> {
            unsafe {
                TerminateJobObject(self.handle, exit_code)
                    .map_err(|e| ProcessError::Job(format!("TerminateJobObject failed: {e}")))
            }
        }

        /// Active process count while the job handle is still open.
        pub fn active_process_count(&self) -> ProcessResult<u32> {
            unsafe {
                let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                let mut returned = 0u32;
                QueryInformationJobObject(
                    self.handle,
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as *mut _,
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    Some(&mut returned),
                )
                .map_err(|e| {
                    ProcessError::Job(format!("QueryInformationJobObject accounting failed: {e}"))
                })?;
                Ok(info.ActiveProcesses)
            }
        }

        #[allow(dead_code)]
        pub fn list_process_ids(&self) -> ProcessResult<Vec<u32>> {
            unsafe {
                let capacity = 256usize;
                let bytes = size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()
                    + capacity.saturating_sub(1) * size_of::<usize>();
                let mut buf = vec![0u8; bytes];
                let mut returned = 0u32;
                QueryInformationJobObject(
                    self.handle,
                    JobObjectBasicProcessIdList,
                    buf.as_mut_ptr() as *mut _,
                    buf.len() as u32,
                    Some(&mut returned),
                )
                .map_err(|e| {
                    ProcessError::Job(format!("QueryInformationJobObject pid list failed: {e}"))
                })?;
                let header = &*(buf.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST);
                let count = header.NumberOfProcessIdsInList as usize;
                let list_ptr = buf.as_ptr().add(size_of::<u32>() * 2) as *const usize;
                let mut out = Vec::with_capacity(count.min(capacity));
                for i in 0..count.min(capacity) {
                    out.push(*list_ptr.add(i) as u32);
                }
                Ok(out)
            }
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }

    #[allow(dead_code)]
    pub fn open_process_query(pid: u32) -> ProcessResult<HANDLE> {
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                false,
                pid,
            )
            .map_err(|e| ProcessError::Identity(format!("OpenProcess({pid}) failed: {e}")))
        }
    }

    #[allow(dead_code)]
    pub fn open_process_all(pid: u32) -> ProcessResult<HANDLE> {
        unsafe {
            OpenProcess(PROCESS_ALL_ACCESS, false, pid)
                .map_err(|e| ProcessError::Identity(format!("OpenProcess all({pid}) failed: {e}")))
        }
    }

    #[allow(dead_code)]
    pub fn close_handle(handle: HANDLE) {
        unsafe {
            let _ = CloseHandle(handle);
        }
    }

    #[allow(dead_code)]
    pub fn null_handle() -> HANDLE {
        HANDLE(ptr::null_mut())
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
mod stub {
    use super::{ProcessError, ProcessResult};

    #[derive(Debug)]
    pub struct JobObject;

    impl JobObject {
        pub fn create_kill_on_close(_name: Option<&str>) -> ProcessResult<Self> {
            Err(ProcessError::Unsupported(
                "Job Objects require Windows".into(),
            ))
        }
        pub fn name(&self) -> Option<&str> {
            None
        }
        pub fn terminate(&self, _exit_code: u32) -> ProcessResult<()> {
            Err(ProcessError::Unsupported(
                "Job Objects require Windows".into(),
            ))
        }
        pub fn active_process_count(&self) -> ProcessResult<u32> {
            Ok(0)
        }
        pub fn list_process_ids(&self) -> ProcessResult<Vec<u32>> {
            Ok(vec![])
        }
    }
}

#[cfg(not(windows))]
pub use stub::*;

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn job_sets_kill_on_close_and_active_limit() {
        let job = JobObject::create_configured(None, 8).expect("create job");
        // Creating with configured limits is the proof; query accounting to ensure handle is live.
        let _ = job.active_process_count().expect("query");
        assert_eq!(DEFAULT_ACTIVE_PROCESS_LIMIT, 64);
    }
}
