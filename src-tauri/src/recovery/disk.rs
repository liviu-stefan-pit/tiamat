//! Disk pressure probing for scheduling safety.

use std::path::Path;

use crate::recovery::types::{DiskPressureReport, DEFAULT_LOW_DISK_THRESHOLD_BYTES};

/// Probe free disk space for the volume containing `path`.
pub fn probe_disk_pressure(path: &Path, threshold_bytes: u64) -> DiskPressureReport {
    let path_str = path.display().to_string();
    let free = free_disk_bytes(path);
    let low = match free {
        Some(bytes) => bytes < threshold_bytes,
        None => false,
    };
    let message = match free {
        Some(bytes) if low => {
            format!("low disk: {bytes} bytes free (threshold {threshold_bytes}) on {path_str}")
        }
        Some(bytes) => format!("{bytes} bytes free on {path_str}"),
        None => format!("unable to probe free disk for {path_str}"),
    };
    DiskPressureReport {
        path: path_str,
        free_bytes: free,
        low_disk: low,
        threshold_bytes,
        message,
    }
}

pub fn probe_disk_default(path: &Path) -> DiskPressureReport {
    probe_disk_pressure(path, DEFAULT_LOW_DISK_THRESHOLD_BYTES)
}

#[cfg(windows)]
fn free_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_available: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free_available),
            Some(&mut total),
            Some(&mut total_free),
        )
    };
    if ok.is_ok() {
        Some(free_available)
    } else {
        None
    }
}

#[cfg(unix)]
fn free_disk_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: zeroed statvfs is a valid out-param; c_path stays alive for the call.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    // f_bavail is space available to unprivileged users, matching the Windows
    // GetDiskFreeSpaceExW "free available to caller" figure used above.
    Some((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

#[cfg(not(any(windows, unix)))]
fn free_disk_bytes(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn probe_temp_dir_reports_non_low_with_tiny_threshold() {
        let dir = tempdir().unwrap();
        let report = probe_disk_pressure(dir.path(), 1);
        assert!(!report.low_disk || report.free_bytes.is_none());
        assert_eq!(report.threshold_bytes, 1);
    }

    #[test]
    fn huge_threshold_marks_low_when_probe_works() {
        let dir = tempdir().unwrap();
        let report = probe_disk_pressure(dir.path(), u64::MAX);
        if report.free_bytes.is_some() {
            assert!(report.low_disk);
        }
    }
}
