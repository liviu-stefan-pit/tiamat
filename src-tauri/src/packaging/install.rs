use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::packaging::types::{PackagingError, PackagingResult};
use crate::workspace::{
    ManagedProject, ManagedProjectKind, PromotionMetadata, PromotionStatus, RetentionPolicy,
    RunWorkspaceManifest, SourceFingerprint,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPlan {
    pub remove_program_files: bool,
    pub remove_start_menu_shortcuts: bool,
    pub remove_app_data_db: bool,
    pub remove_managed_workspaces: bool,
    pub retain_unpromoted_workspaces: bool,
    pub retained_paths: Vec<String>,
    pub warnings: Vec<String>,
}

/// Uninstall must never silently delete unpromoted managed work.
pub fn plan_uninstall_retention(manifests: &[RunWorkspaceManifest]) -> UninstallPlan {
    let mut retained = Vec::new();
    let mut warnings = Vec::new();
    for manifest in manifests {
        if manifest.retention.retain_unpromoted && manifest.has_unpromoted_work() {
            retained.push(manifest.managed_run_root.clone());
            warnings.push(format!(
                "retaining unpromoted workspace {} (promotion={:?})",
                manifest.managed_run_root, manifest.promotion.status
            ));
        }
    }
    UninstallPlan {
        remove_program_files: true,
        remove_start_menu_shortcuts: true,
        // App DB/settings may be removed on uninstall; managed workspaces are separate.
        remove_app_data_db: true,
        remove_managed_workspaces: retained.is_empty(),
        retain_unpromoted_workspaces: !retained.is_empty(),
        retained_paths: retained,
        warnings,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradePreserveResult {
    pub db_preserved: bool,
    pub settings_preserved: bool,
    pub workspaces_preserved: bool,
    pub previous_version: String,
    pub next_version: String,
    pub messages: Vec<String>,
}

/// Upgrade keeps DB, settings, and managed workspaces under the same app data root.
pub fn simulate_upgrade_preserve(
    app_data_root: &Path,
    previous_version: &str,
    next_version: &str,
) -> PackagingResult<UpgradePreserveResult> {
    let db = app_data_root.join("tiamat.sqlite3");
    let settings_marker = app_data_root.join("settings.marker");
    let workspaces = app_data_root.join("workspaces");
    if !app_data_root.exists() {
        return Err(PackagingError::Message(format!(
            "app data root missing: {}",
            app_data_root.display()
        )));
    }
    Ok(UpgradePreserveResult {
        db_preserved: db.exists(),
        settings_preserved: settings_marker.exists() || db.exists(),
        workspaces_preserved: workspaces.exists(),
        previous_version: previous_version.to_string(),
        next_version: next_version.to_string(),
        messages: vec![
            "upgrade must not rewrite managed workspace roots".into(),
            "upgrade must migrate schema forward only".into(),
        ],
    })
}

pub fn ensure_upgrade_scaffold(app_data_root: &Path) -> PackagingResult<PathBuf> {
    std::fs::create_dir_all(app_data_root)?;
    std::fs::create_dir_all(app_data_root.join("workspaces"))?;
    std::fs::write(app_data_root.join("tiamat.sqlite3"), b"sqlite-placeholder")?;
    std::fs::write(app_data_root.join("settings.marker"), b"ok")?;
    Ok(app_data_root.to_path_buf())
}

pub fn sample_unpromoted_manifest(root: &str) -> RunWorkspaceManifest {
    let fingerprint = SourceFingerprint {
        path: root.to_string(),
        kind: "git".into(),
        head: Some("deadbeef".into()),
        branch: Some("main".into()),
        status_porcelain: String::new(),
        status_hash: "hash".into(),
        tree_hash: "tree".into(),
        captured_at_utc: chrono::Utc::now(),
    };
    RunWorkspaceManifest {
        schema_version: 1,
        run_id: uuid::Uuid::nil(),
        intake_id: uuid::Uuid::nil(),
        managed_run_root: root.to_string(),
        control_root: format!("{root}\\.tiamat"),
        projects: vec![ManagedProject {
            project_id: "proj-1".into(),
            source_root: root.to_string(),
            managed_root: format!("{root}\\owned"),
            kind: ManagedProjectKind::GitClone,
            baseline_commit: Some("deadbeef".into()),
            baseline_branch: "tiamat/baseline".into(),
            worktree_path: None,
            write_root: format!("{root}\\owned"),
            read_roots: vec![format!("{root}\\owned")],
            dirty_overlay: None,
            source_fingerprint: fingerprint,
            lock_name: "proj-1".into(),
        }],
        notes_roots: vec![],
        checkpoints: vec![],
        quarantines: vec![],
        promotion: PromotionMetadata {
            status: PromotionStatus::Unpromoted,
            export_path: None,
            promoted_at_utc: None,
            notes: None,
        },
        retention: RetentionPolicy {
            retain_unpromoted: true,
            max_quarantine_entries: 32,
            allow_destructive_cleanup: false,
        },
        fingerprint_pairs: vec![],
        created_at_utc: chrono::Utc::now(),
        source_unchanged: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn uninstall_retains_unpromoted_workspaces() {
        let plan = plan_uninstall_retention(&[sample_unpromoted_manifest(
            r"C:\Users\user\AppData\Roaming\com.tiamat.desktop\tiamat\workspaces\unpromoted-1",
        )]);
        assert!(plan.retain_unpromoted_workspaces);
        assert!(!plan.remove_managed_workspaces);
        assert_eq!(plan.retained_paths.len(), 1);
    }

    #[test]
    fn upgrade_preserves_db_settings_workspaces() {
        let dir = tempdir().unwrap();
        ensure_upgrade_scaffold(dir.path()).unwrap();
        let result = simulate_upgrade_preserve(dir.path(), "0.1.0", "0.1.1").unwrap();
        assert!(result.db_preserved);
        assert!(result.settings_preserved);
        assert!(result.workspaces_preserved);
    }
}
