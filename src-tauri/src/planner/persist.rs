use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tiamat_contracts::ProjectPlan;
use uuid::Uuid;

use crate::planner::render::{render_master_plan_markdown, sha256_hex, verify_markdown_projection};
use crate::planner::types::PlanArtifactHashes;
use crate::workspace::{
    create_control_checkpoint, CheckpointRecord, RunWorkspaceManifest, WorkspaceError,
    WorkspaceResult,
};

pub fn plan_json_path(control_root: &Path) -> PathBuf {
    control_root.join(".tiamat").join("plan.json")
}

pub fn master_plan_md_path(control_root: &Path) -> PathBuf {
    control_root.join(".tiamat").join("MASTER-PLAN.md")
}

/// Atomically write plan.json + MASTER-PLAN.md and verify Markdown projection.
pub fn write_plan_artifacts(
    control_root: &Path,
    plan: &ProjectPlan,
) -> WorkspaceResult<(PathBuf, PathBuf, PlanArtifactHashes)> {
    let tiamat_dir = control_root.join(".tiamat");
    fs::create_dir_all(&tiamat_dir)?;

    let json_path = plan_json_path(control_root);
    let md_path = master_plan_md_path(control_root);

    let json_text = serde_json::to_string_pretty(plan)
        .map_err(|e| WorkspaceError::Message(format!("serialize plan: {e}")))?;
    let markdown = render_master_plan_markdown(plan);
    verify_markdown_projection(plan, &markdown).map_err(WorkspaceError::Message)?;

    atomic_write(&json_path, json_text.as_bytes())?;
    atomic_write(&md_path, markdown.as_bytes())?;

    let written_md = fs::read_to_string(&md_path)?;
    verify_markdown_projection(plan, &written_md).map_err(WorkspaceError::Message)?;

    Ok((
        json_path,
        md_path,
        PlanArtifactHashes {
            plan_json_sha256: sha256_hex(json_text.as_bytes()),
            master_plan_md_sha256: sha256_hex(markdown.as_bytes()),
        },
    ))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> WorkspaceResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("plan"),
        Uuid::new_v4()
    ));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn checkpoint_control_plan(
    manifest: &mut RunWorkspaceManifest,
    message: &str,
) -> WorkspaceResult<CheckpointRecord> {
    create_control_checkpoint(manifest, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::render::render_master_plan_markdown;
    use crate::workspace::{
        configure_identity, git, ManagedProject, ManagedProjectKind, PromotionMetadata,
        PromotionStatus, RetentionPolicy, SourceFingerprint,
    };
    use tempfile::tempdir;
    use tiamat_contracts::{
        AcceptanceCriterion, ModelTier, PhasePlan, PhaseStatus, RollbackSpec, RollbackStrategy,
        TestExpected, TestKind, TestSpec,
    };

    #[test]
    fn atomic_write_and_checkpoint_control() {
        let dir = tempdir().unwrap();
        let control = dir.path().join("control");
        fs::create_dir_all(control.join(".tiamat")).unwrap();
        git(&control, &["init"]).unwrap();
        configure_identity(&control).unwrap();
        git(&control, &["add", "-A"]).unwrap();
        git(&control, &["commit", "--allow-empty", "-m", "baseline"]).unwrap();

        let plan = ProjectPlan {
            schema_version: 1,
            run_id: Uuid::nil(),
            title: "T".into(),
            summary: "S".into(),
            assumptions: vec![],
            risks: vec![],
            phases: vec![PhasePlan {
                phase_id: "P01".into(),
                title: "One".into(),
                objective: "Obj".into(),
                dependencies: vec![],
                project_ids: vec!["app".into()],
                read_roots: vec![".".into()],
                write_roots: vec![".".into()],
                model_tier: ModelTier::Composer,
                estimated_minutes: 5,
                acceptance_criteria: vec![AcceptanceCriterion {
                    criterion_id: "AC-1".into(),
                    description: "d".into(),
                    required_evidence_kinds: vec![TestKind::Unit],
                }],
                unit_tests: vec![TestSpec {
                    test_id: "UT-1".into(),
                    command: vec!["true".into()],
                    working_directory: ".".into(),
                    timeout_seconds: 10,
                    resource_locks: vec![],
                    expected: TestExpected {
                        exit_code: 0,
                        artifacts: vec![],
                    },
                    covers: vec!["AC-1".into()],
                    inapplicable_reason: None,
                }],
                integration_tests: vec![],
                e2e_tests: vec![],
                manual_checks: vec![],
                rollback: RollbackSpec {
                    checkpoint: "b".into(),
                    strategy: RollbackStrategy::Restore,
                },
                expected_artifacts: vec![],
                prompt: "Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json".into(),
                status: PhaseStatus::Draft,
                evidence: vec![],
            }],
            final_gates: vec![],
        };

        let (json_path, md_path, hashes) = write_plan_artifacts(&control, &plan).unwrap();
        assert!(json_path.exists());
        assert!(md_path.exists());
        assert!(!hashes.plan_json_sha256.is_empty());
        let md = fs::read_to_string(&md_path).unwrap();
        assert_eq!(md, render_master_plan_markdown(&plan));

        let mut manifest = RunWorkspaceManifest {
            schema_version: 1,
            run_id: Uuid::nil(),
            intake_id: Uuid::nil(),
            managed_run_root: dir.path().display().to_string(),
            control_root: control.display().to_string(),
            projects: vec![ManagedProject {
                project_id: "app".into(),
                source_root: ".".into(),
                managed_root: ".".into(),
                kind: ManagedProjectKind::NotesSnapshot,
                baseline_commit: None,
                baseline_branch: "main".into(),
                worktree_path: None,
                write_root: ".".into(),
                read_roots: vec![],
                dirty_overlay: None,
                source_fingerprint: SourceFingerprint {
                    path: ".".into(),
                    kind: "notes".into(),
                    head: None,
                    branch: None,
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
        let cp = checkpoint_control_plan(&mut manifest, "initial-plan").unwrap();
        assert_eq!(cp.project_id, "control");
        assert!(!cp.commit.is_empty());
    }
}
