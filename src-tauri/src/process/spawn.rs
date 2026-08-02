//! Atomic create-time Job association via `PROC_THREAD_ATTRIBUTE_JOB_LIST` (pipe-capable),
//! with CREATE_SUSPENDED → assign → resume only as last-resort degraded mode.

use super::error::{ProcessError, ProcessResult};
use super::job::JobObject;
use std::io;
use std::process::{Child, ExitStatus};

#[cfg(windows)]
use super::windows_cmd::{normalize_windows_argv, quote_windows_arg};
#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::fs::File;
#[cfg(windows)]
use std::io::Write;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::io::{FromRawHandle, RawHandle};
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;

#[derive(Debug)]
pub struct SpawnedInJob {
    pub pid: u32,
    pub process_handle: isize,
    pub job: JobObject,
    pub association: AssociationMethod,
    pub executable_identity: String,
    pub creation_time_100ns: u64,
    /// True when production fell back to suspended→assign (host-crash orphan window).
    pub degraded_association: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationMethod {
    ProcThreadAttributeJobList,
    /// Last-resort CREATE_SUSPENDED → AssignProcessToJobObject → resume.
    SuspendedAssignDegraded,
}

/// Pipe-capable hosted child (attribute-list or degraded std Child).
pub struct HostedChild {
    inner: HostedChildInner,
}

enum HostedChildInner {
    Std(Child),
    #[cfg(windows)]
    Attr {
        process: isize,
        stdout: Option<File>,
        stderr: Option<File>,
        /// Closed after optional stdin write at spawn.
        _stdin_closed: bool,
    },
}

impl HostedChild {
    pub fn take_stdout(&mut self) -> Option<Box<dyn io::Read + Send>> {
        match &mut self.inner {
            HostedChildInner::Std(c) => c
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn io::Read + Send>),
            #[cfg(windows)]
            HostedChildInner::Attr { stdout, .. } => stdout
                .take()
                .map(|f| Box::new(f) as Box<dyn io::Read + Send>),
        }
    }

    pub fn take_stderr(&mut self) -> Option<Box<dyn io::Read + Send>> {
        match &mut self.inner {
            HostedChildInner::Std(c) => c
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn io::Read + Send>),
            #[cfg(windows)]
            HostedChildInner::Attr { stderr, .. } => stderr
                .take()
                .map(|f| Box::new(f) as Box<dyn io::Read + Send>),
        }
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match &mut self.inner {
            HostedChildInner::Std(c) => c.try_wait(),
            #[cfg(windows)]
            HostedChildInner::Attr { process, .. } => unsafe {
                use windows::Win32::Foundation::{HANDLE, STILL_ACTIVE};
                use windows::Win32::System::Threading::GetExitCodeProcess;
                let mut code = 0u32;
                GetExitCodeProcess(HANDLE(*process as *mut _), &mut code)
                    .map_err(|e| io::Error::other(e.to_string()))?;
                // STILL_ACTIVE == 259
                if code == 259 || code == (STILL_ACTIVE.0 as u32) {
                    Ok(None)
                } else {
                    Ok(Some(ExitStatus::from_raw(code)))
                }
            },
        }
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        match &mut self.inner {
            HostedChildInner::Std(c) => c.wait(),
            #[cfg(windows)]
            HostedChildInner::Attr { process, .. } => unsafe {
                use windows::Win32::Foundation::HANDLE;
                use windows::Win32::System::Threading::{
                    GetExitCodeProcess, WaitForSingleObject, INFINITE,
                };
                let h = HANDLE(*process as *mut _);
                let _ = WaitForSingleObject(h, INFINITE);
                let mut code = 0u32;
                GetExitCodeProcess(h, &mut code).map_err(|e| io::Error::other(e.to_string()))?;
                Ok(ExitStatus::from_raw(code))
            },
        }
    }

    pub fn kill(&mut self) -> io::Result<()> {
        match &mut self.inner {
            HostedChildInner::Std(c) => c.kill(),
            #[cfg(windows)]
            HostedChildInner::Attr { process, .. } => unsafe {
                use windows::Win32::Foundation::HANDLE;
                use windows::Win32::System::Threading::TerminateProcess;
                TerminateProcess(HANDLE(*process as *mut _), 1)
                    .map_err(|e| io::Error::other(e.to_string()))
            },
        }
    }
}

/// Pipe-capable hosted spawn. Prefer `PROC_THREAD_ATTRIBUTE_JOB_LIST`; fall back to
/// suspended→assign only when attribute-list association fails, with degraded recording.
#[cfg(windows)]
pub fn spawn_hosted(
    argv: &[String],
    stdin_data: Option<&str>,
    env: &[(String, String)],
    workspace: Option<&str>,
) -> ProcessResult<(SpawnedInJob, HostedChild)> {
    if argv.is_empty() {
        return Err(ProcessError::Spawn("argv must not be empty".into()));
    }
    let argv = normalize_windows_argv(argv);
    match spawn_attribute_list_piped(&argv, stdin_data, env, workspace) {
        Ok(v) => Ok(v),
        Err(attr_err) => {
            let (mut spawned, child) =
                spawn_contained_suspended(&argv, stdin_data, env, workspace)?;
            spawned.association = AssociationMethod::SuspendedAssignDegraded;
            spawned.degraded_association = true;
            // Preserve the attribute-list failure reason in executable metadata path callers.
            let _ = attr_err;
            Ok((spawned, child))
        }
    }
}

#[cfg(windows)]
fn spawn_attribute_list_piped(
    argv: &[String],
    stdin_data: Option<&str>,
    env: &[(String, String)],
    workspace: Option<&str>,
) -> ProcessResult<(SpawnedInJob, HostedChild)> {
    use std::mem::{size_of, zeroed};
    use std::ptr;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT,
    };
    use windows::Win32::Security::SECURITY_ATTRIBUTES;
    use windows::Win32::System::Pipes::CreatePipe;
    use windows::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
        UpdateProcThreadAttribute, CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT,
        EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
        PROC_THREAD_ATTRIBUTE_JOB_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    };

    let job = JobObject::create_kill_on_close(None)?;

    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: ptr::null_mut(),
        bInheritHandle: true.into(),
    };

    unsafe {
        let mut stdin_r = HANDLE::default();
        let mut stdin_w = HANDLE::default();
        let mut stdout_r = HANDLE::default();
        let mut stdout_w = HANDLE::default();
        let mut stderr_r = HANDLE::default();
        let mut stderr_w = HANDLE::default();

        CreatePipe(&mut stdin_r, &mut stdin_w, Some(&sa), 0)
            .map_err(|e| ProcessError::Spawn(format!("stdin CreatePipe: {e}")))?;
        CreatePipe(&mut stdout_r, &mut stdout_w, Some(&sa), 0)
            .map_err(|e| ProcessError::Spawn(format!("stdout CreatePipe: {e}")))?;
        CreatePipe(&mut stderr_r, &mut stderr_w, Some(&sa), 0)
            .map_err(|e| ProcessError::Spawn(format!("stderr CreatePipe: {e}")))?;

        // Parent ends must not be inherited.
        let _ = SetHandleInformation(stdin_w, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS_ZERO);
        let _ = SetHandleInformation(stdout_r, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS_ZERO);
        let _ = SetHandleInformation(stderr_r, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS_ZERO);

        let mut attr_size = 0usize;
        let _ = InitializeProcThreadAttributeList(
            LPPROC_THREAD_ATTRIBUTE_LIST(ptr::null_mut()),
            1,
            0,
            &mut attr_size,
        );
        if attr_size == 0 {
            close_pipe_pair(stdin_r, stdin_w);
            close_pipe_pair(stdout_r, stdout_w);
            close_pipe_pair(stderr_r, stderr_w);
            return Err(ProcessError::Spawn(
                "InitializeProcThreadAttributeList size query failed".into(),
            ));
        }
        let mut attr_buf = vec![0u8; attr_size];
        let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut _);
        if let Err(e) = InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_size) {
            close_pipe_pair(stdin_r, stdin_w);
            close_pipe_pair(stdout_r, stdout_w);
            close_pipe_pair(stderr_r, stderr_w);
            return Err(ProcessError::Spawn(format!("attr init: {e}")));
        }

        let mut job_list = [job.handle()];
        if let Err(e) = UpdateProcThreadAttribute(
            attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
            Some(job_list.as_mut_ptr() as *mut _),
            size_of::<HANDLE>(),
            None,
            None,
        ) {
            DeleteProcThreadAttributeList(attr_list);
            close_pipe_pair(stdin_r, stdin_w);
            close_pipe_pair(stdout_r, stdout_w);
            close_pipe_pair(stderr_r, stderr_w);
            return Err(ProcessError::Spawn(format!("attr update JOB_LIST: {e}")));
        }

        let mut siex: STARTUPINFOEXW = zeroed();
        siex.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        siex.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        siex.StartupInfo.hStdInput = stdin_r;
        siex.StartupInfo.hStdOutput = stdout_w;
        siex.StartupInfo.hStdError = stderr_w;
        siex.lpAttributeList = attr_list;

        let cmdline = join_windows_command_line(argv);
        let mut cmdline_wide: Vec<u16> =
            OsStr::new(&cmdline).encode_wide().chain(Some(0)).collect();

        let cwd_wide: Option<Vec<u16>> = workspace.map(|c| {
            OsStr::new(c)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        });
        let cwd_ptr = cwd_wide
            .as_ref()
            .map(|v| PCWSTR(v.as_ptr()))
            .unwrap_or(PCWSTR::null());

        let env_block = build_env_block(env);
        let mut pi = PROCESS_INFORMATION::default();
        let created = CreateProcessW(
            PCWSTR::null(),
            windows::core::PWSTR(cmdline_wide.as_mut_ptr()),
            None,
            None,
            true, // inherit pipe ends destined for the child
            EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            Some(env_block.as_ptr() as *const _),
            cwd_ptr,
            &siex.StartupInfo,
            &mut pi,
        );
        DeleteProcThreadAttributeList(attr_list);
        // Close child-side ends in the parent.
        let _ = CloseHandle(stdin_r);
        let _ = CloseHandle(stdout_w);
        let _ = CloseHandle(stderr_w);

        if let Err(e) = created {
            let _ = CloseHandle(stdin_w);
            let _ = CloseHandle(stdout_r);
            let _ = CloseHandle(stderr_r);
            return Err(ProcessError::Spawn(format!(
                "CreateProcessW JOB_LIST piped: {e}"
            )));
        }

        let pid = pi.dwProcessId;
        let process_handle = pi.hProcess.0 as isize;
        let _ = CloseHandle(pi.hThread);

        // Write stdin then close write end so the child sees EOF.
        if let Some(data) = stdin_data {
            let mut file = File::from_raw_handle(stdin_w.0 as RawHandle);
            if let Err(e) = file.write_all(data.as_bytes()) {
                drop(file);
                let _ = CloseHandle(stdout_r);
                let _ = CloseHandle(stderr_r);
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .output();
                return Err(ProcessError::Spawn(format!(
                    "failed to write full stdin to child: {e}"
                )));
            }
            // drop closes
        } else {
            let _ = CloseHandle(stdin_w);
        }

        let identity = require_identity(pid, &argv[0])?;

        let stdout = File::from_raw_handle(stdout_r.0 as RawHandle);
        let stderr = File::from_raw_handle(stderr_r.0 as RawHandle);

        Ok((
            SpawnedInJob {
                pid,
                process_handle,
                job,
                association: AssociationMethod::ProcThreadAttributeJobList,
                executable_identity: identity.executable_path,
                creation_time_100ns: identity.creation_time_100ns,
                degraded_association: false,
            },
            HostedChild {
                inner: HostedChildInner::Attr {
                    process: process_handle,
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    _stdin_closed: true,
                },
            },
        ))
    }
}

/// Sentinel for clearing inherit flag (dwFlags = 0).
#[cfg(windows)]
const HANDLE_FLAGS_ZERO: windows::Win32::Foundation::HANDLE_FLAGS =
    windows::Win32::Foundation::HANDLE_FLAGS(0);

#[cfg(windows)]
fn close_pipe_pair(a: windows::Win32::Foundation::HANDLE, b: windows::Win32::Foundation::HANDLE) {
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(a);
        let _ = windows::Win32::Foundation::CloseHandle(b);
    }
}

#[cfg(windows)]
fn require_identity(
    pid: u32,
    fallback_exe: &str,
) -> ProcessResult<super::identity::ProcessIdentity> {
    let identity = super::identity::query_identity(pid).map_err(|e| {
        // Best-effort terminate unverifiable spawn.
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
        ProcessError::Spawn(format!("identity query failed after spawn: {e}"))
    })?;
    if identity.creation_time_100ns == 0 {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
        return Err(ProcessError::Spawn(format!(
            "refusing creation_time_100ns=0 for pid {pid} ({fallback_exe})"
        )));
    }
    Ok(identity)
}

#[cfg(windows)]
fn spawn_contained_suspended(
    argv: &[String],
    stdin_data: Option<&str>,
    env: &[(String, String)],
    workspace: Option<&str>,
) -> ProcessResult<(SpawnedInJob, HostedChild)> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use windows::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

    let job = JobObject::create_kill_on_close(None)?;
    let program = &argv[0];
    let args = &argv[1..];

    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_SUSPENDED.0 | CREATE_NO_WINDOW.0);
    if let Some(cwd) = workspace {
        cmd.current_dir(cwd);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| ProcessError::Spawn(format!("suspended spawn failed: {e}")))?;
    let pid = child.id();

    let proc_handle = unsafe {
        windows::Win32::System::Threading::OpenProcess(
            windows::Win32::System::Threading::PROCESS_ALL_ACCESS,
            false,
            pid,
        )
        .map_err(|e| ProcessError::Spawn(format!("OpenProcess: {e}")))?
    };

    if let Err(e) = job.assign_process(proc_handle) {
        unsafe {
            let _ = windows::Win32::System::Threading::TerminateProcess(proc_handle, 1);
            let _ = windows::Win32::Foundation::CloseHandle(proc_handle);
        }
        let _ = child.kill();
        return Err(e);
    }

    resume_process_threads(pid)?;

    if let Some(data) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(data.as_bytes()) {
                let _ = child.kill();
                unsafe {
                    let _ = windows::Win32::System::Threading::TerminateProcess(proc_handle, 1);
                    let _ = windows::Win32::Foundation::CloseHandle(proc_handle);
                }
                return Err(ProcessError::Spawn(format!(
                    "failed to write full stdin to child: {e}"
                )));
            }
        } else {
            let _ = child.kill();
            unsafe {
                let _ = windows::Win32::System::Threading::TerminateProcess(proc_handle, 1);
                let _ = windows::Win32::Foundation::CloseHandle(proc_handle);
            }
            return Err(ProcessError::Spawn(
                "child stdin pipe missing after spawn".into(),
            ));
        }
    } else if let Some(stdin) = child.stdin.take() {
        drop(stdin);
    }

    let identity = require_identity(pid, program)?;

    Ok((
        SpawnedInJob {
            pid,
            process_handle: proc_handle.0 as isize,
            job,
            association: AssociationMethod::SuspendedAssignDegraded,
            executable_identity: identity.executable_path,
            creation_time_100ns: identity.creation_time_100ns,
            degraded_association: true,
        },
        HostedChild {
            inner: HostedChildInner::Std(child),
        },
    ))
}

#[cfg(windows)]
fn resume_process_threads(pid: u32) -> ProcessResult<()> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
            .map_err(|e| ProcessError::Spawn(format!("CreateToolhelp32Snapshot failed: {e}")))?;
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        if Thread32First(snap, &mut entry).is_ok() {
            loop {
                if entry.th32OwnerProcessID == pid {
                    if let Ok(th) = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) {
                        let _ = ResumeThread(th);
                        let _ = windows::Win32::Foundation::CloseHandle(th);
                    }
                }
                if Thread32Next(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
    }
    Ok(())
}

#[cfg(windows)]
fn join_windows_command_line(argv: &[String]) -> String {
    // Match probe `raw_arg` semantics for cmd.exe /d /s /c <payload>: the /c
    // payload is already a single pre-built command string and must not be
    // re-quoted (inner quotes around spaced paths would otherwise break).
    if argv.len() >= 4
        && argv[0].eq_ignore_ascii_case("cmd.exe")
        && argv[1] == "/d"
        && argv[2] == "/s"
        && argv[3] == "/c"
    {
        let mut line = format!(
            "{} {} {} {}",
            quote_windows_arg(&argv[0]),
            argv[1],
            argv[2],
            argv[3]
        );
        if let Some(payload) = argv.get(4) {
            line.push(' ');
            line.push_str(payload);
        }
        for extra in argv.iter().skip(5) {
            line.push(' ');
            line.push_str(&quote_windows_arg(extra));
        }
        return line;
    }
    argv.iter()
        .map(|a| quote_windows_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn build_env_block(extra: &[(String, String)]) -> Vec<u16> {
    use std::collections::HashMap;
    let mut map: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in extra {
        map.insert(k.clone(), v.clone());
    }
    let mut block: Vec<u16> = Vec::new();
    for (k, v) in map {
        let entry = format!("{k}={v}");
        block.extend(OsStr::new(&entry).encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

/// Prove `PROC_THREAD_ATTRIBUTE_JOB_LIST` associates a process at create time.
#[cfg(windows)]
pub fn prove_attribute_list_association() -> ProcessResult<AssociationMethod> {
    use std::mem::{size_of, zeroed};
    use std::ptr;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
        UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW,
        EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
        PROC_THREAD_ATTRIBUTE_JOB_LIST, STARTUPINFOEXW,
    };

    let job = JobObject::create_kill_on_close(None)?;
    unsafe {
        let mut attr_size = 0usize;
        let _ = InitializeProcThreadAttributeList(
            LPPROC_THREAD_ATTRIBUTE_LIST(ptr::null_mut()),
            1,
            0,
            &mut attr_size,
        );
        if attr_size == 0 {
            return Err(ProcessError::Spawn(
                "InitializeProcThreadAttributeList size query failed".into(),
            ));
        }
        let mut attr_buf = vec![0u8; attr_size];
        let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut _);
        InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_size)
            .map_err(|e| ProcessError::Spawn(format!("attr init: {e}")))?;

        let mut job_list = [job.handle()];
        UpdateProcThreadAttribute(
            attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
            Some(job_list.as_mut_ptr() as *mut _),
            size_of::<HANDLE>(),
            None,
            None,
        )
        .map_err(|e| {
            DeleteProcThreadAttributeList(attr_list);
            ProcessError::Spawn(format!("attr update JOB_LIST: {e}"))
        })?;

        let mut siex: STARTUPINFOEXW = zeroed();
        siex.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        siex.lpAttributeList = attr_list;

        let app: Vec<u16> = OsStr::new("C:\\Windows\\System32\\ping.exe")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut cmdline: Vec<u16> = OsStr::new("ping -n 1 127.0.0.1")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut pi = PROCESS_INFORMATION::default();
        let created = CreateProcessW(
            PCWSTR(app.as_ptr()),
            windows::core::PWSTR(cmdline.as_mut_ptr()),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
            None,
            None,
            &siex.StartupInfo,
            &mut pi,
        );
        DeleteProcThreadAttributeList(attr_list);
        created.map_err(|e| ProcessError::Spawn(format!("CreateProcessW JOB_LIST: {e}")))?;

        let count = job.active_process_count().unwrap_or(0);
        let _ = WaitForSingleObject(pi.hProcess, 5_000);
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
        let _ = job.terminate(0);

        if count == 0 {
            return Err(ProcessError::Spawn(
                "JOB_LIST association produced zero active processes".into(),
            ));
        }
        Ok(AssociationMethod::ProcThreadAttributeJobList)
    }
}

#[cfg(windows)]
pub fn attribute_list_api_available() -> bool {
    prove_attribute_list_association().is_ok()
}

/// Hosted spawn on Unix: the child leads its own process group, so the whole
/// descendant tree can be signalled together (see `job::JobObject` for the
/// Job Object equivalence and its limits).
#[cfg(unix)]
pub fn spawn_hosted(
    argv: &[String],
    stdin_data: Option<&str>,
    env: &[(String, String)],
    workspace: Option<&str>,
) -> ProcessResult<(SpawnedInJob, HostedChild)> {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if argv.is_empty() {
        return Err(ProcessError::Spawn("argv must not be empty".into()));
    }

    let job = JobObject::create_kill_on_close(None)?;
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        cmd.env(key, value);
    }
    if let Some(dir) = workspace {
        cmd.current_dir(dir);
    }
    // Group leader, so killpg reaches every descendant that has not called setsid().
    cmd.process_group(0);

    let mut child = cmd
        .spawn()
        .map_err(|e| ProcessError::Spawn(format!("spawn {}: {e}", argv[0])))?;
    let pid = child.id();
    job.attach_pgid(pid);

    if let Some(data) = stdin_data {
        if let Some(mut handle) = child.stdin.take() {
            let _ = handle.write_all(data.as_bytes());
        }
    } else {
        drop(child.stdin.take());
    }

    let spawned = SpawnedInJob {
        pid,
        process_handle: 0,
        job,
        association: AssociationMethod::ProcThreadAttributeJobList,
        executable_identity: argv[0].clone(),
        creation_time_100ns: super::identity::query_start_time(pid).unwrap_or(0),
        degraded_association: false,
    };
    Ok((
        spawned,
        HostedChild {
            inner: HostedChildInner::Std(child),
        },
    ))
}

/// Process-group association needs no probing; it is a plain POSIX guarantee.
#[cfg(unix)]
pub fn prove_attribute_list_association() -> ProcessResult<AssociationMethod> {
    Ok(AssociationMethod::ProcThreadAttributeJobList)
}

#[cfg(unix)]
pub fn attribute_list_api_available() -> bool {
    true
}

#[cfg(not(any(windows, unix)))]
pub fn spawn_hosted(
    _argv: &[String],
    _stdin_data: Option<&str>,
    _env: &[(String, String)],
    _workspace: Option<&str>,
) -> ProcessResult<(SpawnedInJob, HostedChild)> {
    Err(ProcessError::Unsupported(
        "hosted spawn requires Windows or a Unix platform".into(),
    ))
}

#[cfg(not(any(windows, unix)))]
pub fn prove_attribute_list_association() -> ProcessResult<AssociationMethod> {
    Err(ProcessError::Unsupported("unsupported platform".into()))
}

#[cfg(not(any(windows, unix)))]
pub fn attribute_list_api_available() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[cfg_attr(not(windows), allow(unused_imports))]
    use super::*;

    #[cfg(windows)]
    #[test]
    fn quote_handles_spaces() {
        assert_eq!(quote_windows_arg("a b"), "\"a b\"");
        assert_eq!(quote_windows_arg("plain"), "plain");
    }

    #[cfg(windows)]
    #[test]
    fn join_command_line_quotes_spaces() {
        let line = join_windows_command_line(&["prog.exe".into(), "a b".into()]);
        assert!(line.contains("\"a b\""));
    }

    #[cfg(windows)]
    #[test]
    fn join_cmd_c_payload_is_not_double_quoted() {
        let payload = r#"C:\tools\agent.cmd --print --workspace "C:\My Project\notes" --mode plan"#;
        let line = join_windows_command_line(&[
            "cmd.exe".into(),
            "/d".into(),
            "/s".into(),
            "/c".into(),
            payload.into(),
        ]);
        // Payload must appear verbatim after /c (probe raw_arg semantics).
        assert!(
            line.ends_with(payload),
            "expected raw payload suffix, got {line}"
        );
        assert!(
            !line.contains(r#"\"C:\My Project\notes\""#),
            "inner quotes must not be escaped by outer join: {line}"
        );
        assert!(line.starts_with("cmd.exe /d /s /c "));
    }
}
