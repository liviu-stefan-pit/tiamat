use std::fs;
use std::path::{Path, PathBuf};

use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::retention::mark_exported;
use crate::workspace::types::RunWorkspaceManifest;

/// Export an isolated copy of a managed project. Never writes back to the source root.
pub fn export_project(
    manifest: &mut RunWorkspaceManifest,
    project_id: &str,
    export_dir: &Path,
) -> WorkspaceResult<PathBuf> {
    let project = manifest
        .projects
        .iter()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| WorkspaceError::Message(format!("unknown project: {project_id}")))?;

    let source_managed = PathBuf::from(&project.managed_root);
    if !source_managed.exists() {
        return Err(WorkspaceError::NotFound(source_managed));
    }

    fs::create_dir_all(export_dir)?;
    let dest = export_dir.join(project_id);
    if dest.exists() {
        return Err(WorkspaceError::Message(format!(
            "export destination exists: {}",
            dest.display()
        )));
    }
    copy_tree(&source_managed, &dest)?;

    // Write promotion metadata alongside the export; do not touch source.
    let meta = serde_json::json!({
        "schemaVersion": 1,
        "runId": manifest.run_id,
        "projectId": project_id,
        "managedRoot": project.managed_root,
        "sourceRoot": project.source_root,
        "baselineCommit": project.baseline_commit,
        "exportedAtUtc": chrono::Utc::now().to_rfc3339(),
        "note": "Isolated export only. Source repository was not modified.",
    });
    fs::write(
        dest.join("tiamat-export.json"),
        serde_json::to_vec_pretty(&meta)?,
    )?;

    mark_exported(manifest, &dest.display().to_string());
    Ok(dest)
}

fn copy_tree(src: &Path, dest: &Path) -> WorkspaceResult<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let name = entry.file_name();
            // Skip nested worktrees metadata noise if present as plain dirs we own.
            copy_tree(&entry.path(), &dest.join(name))?;
        }
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest)?;
    }
    Ok(())
}
