//! Isolated workspace manager: owned clones, dirty overlays, checkpoints, quarantine, retention.

mod checkpoint;
mod clone;
mod copy;
mod dirty;
mod error;
mod fingerprint;
mod git;
mod greenfield;
mod manager;
mod promote;
mod quarantine;
mod retention;
pub mod roots;
mod types;

pub use error::{WorkspaceError, WorkspaceResult};
pub use greenfield::{
    bootstrap_plan_greenfield_projects, collect_missing_greenfield_ids,
    ensure_default_greenfield_if_needed, ensure_greenfield_project, greenfield_project_path,
    greenfield_slug_from_root, has_writable_project, is_allocatable_greenfield_project_id,
    is_flat_layout, is_safe_project_slug, is_writable_project_kind, DEFAULT_GREENFIELD_PROJECT_ID,
};
pub use manager::{
    checkpoint_project, export_managed_project, export_managed_project_to, find_managed_run_root,
    load_manifest, materialize_run_workspace, promote_managed_run, quarantine_project_path,
    recheck_source_fingerprints, recheck_source_fingerprints_for_run, write_manifest,
    MaterializeRequest,
};
pub use promote::export_project;
pub use quarantine::quarantine_path;
pub use retention::{assert_can_cleanup, cleanup_managed_run, mark_exported, mark_promoted};
pub use roots::{
    is_within_managed, lock_name_for, validate_read_roots, validate_relative_within,
    validate_write_roots,
};
pub use types::*;

// Re-exported for planner control-repo checkpoints (P05) and executor recovery (P08).
pub use checkpoint::{create_checkpoint, create_control_checkpoint, rollback_to_checkpoint};
pub use git::{configure_identity, git, git_text};

pub const MODULE: &str = "workspace";

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn manifest_write_root_validation() {
        let manifest = RunWorkspaceManifest {
            schema_version: 1,
            run_id: uuid::Uuid::nil(),
            intake_id: uuid::Uuid::nil(),
            managed_run_root: r"C:\managed\run-1".into(),
            control_root: r"C:\managed\run-1\control".into(),
            projects: vec![ManagedProject {
                project_id: "app".into(),
                source_root: r"C:\src\app".into(),
                managed_root: r"C:\managed\run-1\projects\app".into(),
                kind: ManagedProjectKind::GitClone,
                baseline_commit: Some("abc".into()),
                baseline_branch: "tiamat/intake-app".into(),
                worktree_path: None,
                write_root: r"C:\managed\run-1\projects\app".into(),
                read_roots: vec![r"C:\managed\run-1".into()],
                dirty_overlay: None,
                source_fingerprint: SourceFingerprint {
                    path: r"C:\src\app".into(),
                    kind: "git".into(),
                    head: Some("abc".into()),
                    branch: Some("main".into()),
                    status_porcelain: String::new(),
                    status_hash: "0".into(),
                    tree_hash: "0".into(),
                    captured_at_utc: chrono::Utc::now(),
                },
                lock_name: "write:app".into(),
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
            retention: RetentionPolicy::default(),
            fingerprint_pairs: vec![],
            created_at_utc: chrono::Utc::now(),
            source_unchanged: true,
        };

        assert!(manifest
            .validate_write_root(r"C:\managed\run-1\projects\app\src")
            .is_ok());
        assert!(manifest.validate_write_root(r"C:\src\app").is_err());
        assert!(manifest
            .validate_read_root(r"C:\managed\run-1\control")
            .is_ok());
        assert!(manifest.has_unpromoted_work());
    }

    #[test]
    fn lock_names_are_stable() {
        assert_eq!(lock_name_for("api"), "write:api");
        assert!(is_within_managed(
            r"C:\managed\a",
            Path::new(r"C:\managed\a\b").to_string_lossy().as_ref()
        ));
    }
}
