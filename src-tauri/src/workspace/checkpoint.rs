use chrono::Utc;
use uuid::Uuid;

use crate::workspace::error::WorkspaceResult;
use crate::workspace::git::{git, git_text};
use crate::workspace::types::{CheckpointRecord, RunWorkspaceManifest};

pub fn create_checkpoint(
    manifest: &mut RunWorkspaceManifest,
    project_id: &str,
    message: &str,
) -> WorkspaceResult<CheckpointRecord> {
    let project = manifest
        .projects
        .iter()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| {
            crate::workspace::error::WorkspaceError::Message(format!(
                "unknown project for checkpoint: {project_id}"
            ))
        })?;
    let root = std::path::PathBuf::from(&project.managed_root);
    git(&root, &["add", "-A"])?;
    git(
        &root,
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("tiamat checkpoint: {message}"),
        ],
    )?;
    let commit = git_text(&root, &["rev-parse", "HEAD"])?;
    let branch = git_text(&root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|_| project.baseline_branch.clone());
    let parent = manifest
        .checkpoints
        .iter()
        .rev()
        .find(|c| c.project_id == project_id)
        .map(|c| c.checkpoint_id.clone());

    let record = CheckpointRecord {
        checkpoint_id: format!("cp-{}", Uuid::new_v4()),
        project_id: project_id.to_string(),
        commit,
        branch,
        message: message.to_string(),
        created_at_utc: Utc::now(),
        parent_checkpoint_id: parent,
    };

    // Update project baseline pointer to latest checkpoint.
    if let Some(project) = manifest
        .projects
        .iter_mut()
        .find(|p| p.project_id == project_id)
    {
        project.baseline_commit = Some(record.commit.clone());
    }
    manifest.checkpoints.push(record.clone());
    Ok(record)
}

pub fn rollback_to_checkpoint(
    manifest: &RunWorkspaceManifest,
    project_id: &str,
    checkpoint_id: &str,
) -> WorkspaceResult<()> {
    let checkpoint = manifest
        .checkpoints
        .iter()
        .find(|c| c.checkpoint_id == checkpoint_id && c.project_id == project_id)
        .ok_or_else(|| {
            crate::workspace::error::WorkspaceError::Message(format!(
                "checkpoint not found: {checkpoint_id}"
            ))
        })?;
    let project = manifest
        .projects
        .iter()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| {
            crate::workspace::error::WorkspaceError::Message(format!(
                "unknown project: {project_id}"
            ))
        })?;
    let root = std::path::PathBuf::from(&project.managed_root);
    git(&root, &["reset", "--hard", &checkpoint.commit])?;
    git(&root, &["clean", "-fd"])?;
    Ok(())
}

/// Commit the control repository (`.tiamat/*`) as a plan checkpoint.
pub fn create_control_checkpoint(
    manifest: &mut RunWorkspaceManifest,
    message: &str,
) -> WorkspaceResult<CheckpointRecord> {
    let control = std::path::PathBuf::from(&manifest.control_root);
    git(&control, &["add", "-A"])?;
    git(
        &control,
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("tiamat plan checkpoint: {message}"),
        ],
    )?;
    let commit = git_text(&control, &["rev-parse", "HEAD"])?;
    let branch = git_text(&control, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|_| "master".into());
    let parent = manifest
        .checkpoints
        .iter()
        .rev()
        .find(|c| c.project_id == "control")
        .map(|c| c.checkpoint_id.clone());
    let record = CheckpointRecord {
        checkpoint_id: format!("cp-{}", Uuid::new_v4()),
        project_id: "control".into(),
        commit,
        branch,
        message: message.to_string(),
        created_at_utc: Utc::now(),
        parent_checkpoint_id: parent,
    };
    manifest.checkpoints.push(record.clone());
    Ok(record)
}
