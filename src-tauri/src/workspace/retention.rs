use std::fs;
use std::path::Path;

use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::types::{PromotionStatus, RunWorkspaceManifest};

/// Refuse silent destruction of unpromoted managed work.
pub fn assert_can_cleanup(manifest: &RunWorkspaceManifest, force: bool) -> WorkspaceResult<()> {
    if manifest.retention.retain_unpromoted
        && manifest.has_unpromoted_work()
        && (!force || !manifest.retention.allow_destructive_cleanup)
    {
        return Err(WorkspaceError::UnpromotedWork(format!(
            "refusing cleanup of {} with promotion status {:?}",
            manifest.managed_run_root, manifest.promotion.status
        )));
    }
    if !force && !manifest.retention.allow_destructive_cleanup {
        return Err(WorkspaceError::RetentionBlocked(
            "destructive cleanup disabled by retention policy".into(),
        ));
    }
    Ok(())
}

pub fn cleanup_managed_run(manifest: &RunWorkspaceManifest, force: bool) -> WorkspaceResult<()> {
    assert_can_cleanup(manifest, force)?;
    let root = Path::new(&manifest.managed_run_root);
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    Ok(())
}

pub fn mark_exported(manifest: &mut RunWorkspaceManifest, export_path: &str) {
    manifest.promotion.status = PromotionStatus::Exported;
    manifest.promotion.export_path = Some(export_path.to_string());
    manifest.promotion.promoted_at_utc = Some(chrono::Utc::now());
}

pub fn mark_promoted(manifest: &mut RunWorkspaceManifest, notes: Option<String>) {
    manifest.promotion.status = PromotionStatus::Promoted;
    manifest.promotion.promoted_at_utc = Some(chrono::Utc::now());
    manifest.promotion.notes = notes;
}
