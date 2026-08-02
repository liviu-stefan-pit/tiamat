use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use uuid::Uuid;

use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::types::{QuarantineRecord, RunWorkspaceManifest};

/// Move (or copy-fallback) a bad attempt tree into quarantine under the managed run root.
pub fn quarantine_path(
    manifest: &mut RunWorkspaceManifest,
    project_id: &str,
    source_path: &Path,
    reason: &str,
    from_checkpoint_id: Option<&str>,
) -> WorkspaceResult<QuarantineRecord> {
    let quarantine_root = if crate::workspace::greenfield::is_flat_layout(manifest) {
        PathBuf::from(&manifest.managed_run_root)
            .join(".tiamat")
            .join("quarantine")
    } else {
        PathBuf::from(&manifest.managed_run_root).join("quarantine")
    };
    fs::create_dir_all(&quarantine_root)?;
    let quarantine_id = format!("q-{}", Uuid::new_v4());
    let dest = quarantine_root.join(&quarantine_id);
    if source_path.exists() {
        // Prefer rename; fall back to recursive copy + leave source for operator inspection.
        if fs::rename(source_path, &dest).is_err() {
            copy_recursive(source_path, &dest)?;
        }
    } else {
        return Err(WorkspaceError::NotFound(source_path.to_path_buf()));
    }

    let record = QuarantineRecord {
        quarantine_id: quarantine_id.clone(),
        project_id: project_id.to_string(),
        reason: reason.to_string(),
        source_path: source_path.display().to_string(),
        quarantine_path: dest.display().to_string(),
        created_at_utc: Utc::now(),
        from_checkpoint_id: from_checkpoint_id.map(|s| s.to_string()),
    };
    manifest.quarantines.push(record.clone());

    // Enforce retention cap on quarantine entries (oldest first) without touching unpromoted projects.
    let max = manifest.retention.max_quarantine_entries as usize;
    while manifest.quarantines.len() > max {
        let oldest = manifest.quarantines.remove(0);
        let _ = fs::remove_dir_all(&oldest.quarantine_path);
    }

    Ok(record)
}

fn copy_recursive(src: &Path, dest: &Path) -> WorkspaceResult<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let to = dest.join(entry.file_name());
            copy_recursive(&entry.path(), &to)?;
        }
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest)?;
    }
    Ok(())
}
