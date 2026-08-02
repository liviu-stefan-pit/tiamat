//! Promote / export Tauri commands (DOCS-001).
//! Kept separate from `commands.rs` to reduce merge conflict with terminal-gate edits.

use std::path::PathBuf;

use tauri::{AppHandle, State};
use tiamat_contracts::EventLevel;

use crate::app::commands::{emit_workspace_event, AppState};
use crate::workspace::RunWorkspaceManifest;

#[tauri::command]
pub fn export_workspace_project(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    export_dir: Option<String>,
) -> Result<RunWorkspaceManifest, String> {
    let current = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no workspace manifest".to_string())?;
    let root = PathBuf::from(&current.managed_run_root);

    let dest = match export_dir.filter(|s| !s.trim().is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => rfd::FileDialog::new()
            .set_title("Export managed project")
            .pick_folder()
            .unwrap_or_else(|| root.join("exports")),
    };

    let default_exports = root.join("exports");
    let manifest = if dest == default_exports {
        crate::workspace::export_managed_project(&root, &project_id).map_err(|e| e.to_string())?
    } else {
        crate::workspace::export_managed_project_to(&root, &project_id, &dest)
            .map_err(|e| e.to_string())?
    };

    emit_workspace_event(
        &app,
        &state,
        "workspace.exported",
        EventLevel::Info,
        format!("Exported project '{project_id}'"),
        serde_json::json!({
            "runId": manifest.run_id,
            "projectId": project_id,
            "exportPath": manifest.promotion.export_path,
            "promotionStatus": format!("{:?}", manifest.promotion.status).to_lowercase(),
        }),
    )?;

    *state.last_workspace.lock().map_err(|e| e.to_string())? = Some(manifest.clone());
    Ok(manifest)
}

#[tauri::command]
pub fn promote_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    notes: Option<String>,
) -> Result<RunWorkspaceManifest, String> {
    let current = state
        .last_workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no workspace manifest".to_string())?;
    let root = PathBuf::from(&current.managed_run_root);
    let manifest =
        crate::workspace::promote_managed_run(&root, notes).map_err(|e| e.to_string())?;

    emit_workspace_event(
        &app,
        &state,
        "workspace.promoted",
        EventLevel::Info,
        "Marked managed workspace as promoted".into(),
        serde_json::json!({
            "runId": manifest.run_id,
            "promotionStatus": format!("{:?}", manifest.promotion.status).to_lowercase(),
            "notes": manifest.promotion.notes,
        }),
    )?;

    *state.last_workspace.lock().map_err(|e| e.to_string())? = Some(manifest.clone());
    Ok(manifest)
}
