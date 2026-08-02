use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use tiamat_contracts::{IntakeManifest, ProjectKind};
use uuid::Uuid;

use crate::workspace::checkpoint::create_checkpoint;
use crate::workspace::clone::materialize_git_project;
use crate::workspace::copy::materialize_non_git_project;
use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::fingerprint::{
    capture_fingerprint, ensure_source_unchanged, write_fingerprint_file,
};
use crate::workspace::git::{configure_identity, git};
use crate::workspace::promote::export_project;
use crate::workspace::quarantine::quarantine_path;
use crate::workspace::roots::lock_name_for;
use crate::workspace::types::{
    FingerprintPair, ManagedProject, PromotionMetadata, PromotionStatus, RetentionPolicy,
    RunWorkspaceManifest,
};

pub struct MaterializeRequest {
    pub run_id: Uuid,
    pub intake: IntakeManifest,
    pub managed_parent: PathBuf,
    pub create_internal_worktrees: bool,
}

pub fn materialize_run_workspace(req: MaterializeRequest) -> WorkspaceResult<RunWorkspaceManifest> {
    let managed_run_root = req.managed_parent.join(format!("run-{}", req.run_id));
    if managed_run_root.exists() {
        return Err(WorkspaceError::Message(format!(
            "managed run root already exists: {}",
            managed_run_root.display()
        )));
    }

    // Output may equal the intake folder (common for notes → build-in-place).
    // Copying skips managed `run-*` dirs so we never nest the workspace into itself.

    fs::create_dir_all(managed_run_root.join("projects"))?;
    fs::create_dir_all(managed_run_root.join("notes"))?;
    fs::create_dir_all(managed_run_root.join("quarantine"))?;
    fs::create_dir_all(managed_run_root.join("exports"))?;
    fs::create_dir_all(managed_run_root.join("fingerprints"))?;
    fs::create_dir_all(managed_run_root.join("control"))?;

    // Dedicated control repository for .tiamat/* (plan files land in P05).
    let control_root = managed_run_root.join("control");
    git(&control_root, &["init"])?;
    configure_identity(&control_root)?;
    fs::create_dir_all(control_root.join(".tiamat"))?;
    fs::write(
        control_root.join(".gitignore"),
        "/../projects/\n/../notes/\n/../quarantine/\n/../exports/\n",
    )?;
    fs::write(
        control_root.join(".tiamat").join("README.md"),
        "Tiamat run-control repository. Plan artifacts are written by the orchestrator.\n",
    )?;
    git(&control_root, &["add", "-A"])?;
    git(
        &control_root,
        &["commit", "--allow-empty", "-m", "tiamat control baseline"],
    )?;

    let approved_roots: Vec<PathBuf> = req
        .intake
        .projects
        .iter()
        .map(|p| PathBuf::from(&p.root))
        .collect();

    let mut projects = Vec::new();
    let mut notes_roots = Vec::new();
    let mut fingerprint_pairs = Vec::new();
    let mut source_unchanged = true;

    for project in &req.intake.projects {
        let source = PathBuf::from(&project.root);
        if !source.exists() {
            return Err(WorkspaceError::NotFound(source));
        }

        let before = capture_fingerprint(&source, project_kind_label(&project.kind))?;
        write_fingerprint_file(
            &managed_run_root
                .join("fingerprints")
                .join(format!("{}-before.json", project.project_id)),
            &before,
        )?;

        let dest = match project.kind {
            ProjectKind::Notes => managed_run_root.join("notes").join(&project.project_id),
            _ => managed_run_root.join("projects").join(&project.project_id),
        };

        let managed = match project.kind {
            ProjectKind::Git => {
                let result = materialize_git_project(
                    &source,
                    &dest,
                    &project.project_id,
                    req.create_internal_worktrees,
                )?;
                let write_root = result
                    .worktree_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| result.managed_root.display().to_string());
                ManagedProject {
                    project_id: project.project_id.clone(),
                    source_root: project.root.clone(),
                    managed_root: result.managed_root.display().to_string(),
                    kind: result.kind,
                    baseline_commit: Some(result.baseline_commit),
                    baseline_branch: result.baseline_branch,
                    worktree_path: result.worktree_path.map(|p| p.display().to_string()),
                    write_root,
                    read_roots: vec![
                        managed_run_root.display().to_string(),
                        dest.display().to_string(),
                    ],
                    dirty_overlay: result.dirty_overlay,
                    source_fingerprint: before.clone(),
                    lock_name: lock_name_for(&project.project_id),
                }
            }
            ProjectKind::Folder => {
                let result = materialize_non_git_project(
                    &source,
                    &dest,
                    &project.project_id,
                    &approved_roots,
                    false,
                )?;
                ManagedProject {
                    project_id: project.project_id.clone(),
                    source_root: project.root.clone(),
                    managed_root: result.managed_root.display().to_string(),
                    kind: result.kind,
                    baseline_commit: Some(result.baseline_commit),
                    baseline_branch: result.baseline_branch,
                    worktree_path: None,
                    write_root: dest.display().to_string(),
                    read_roots: vec![
                        managed_run_root.display().to_string(),
                        dest.display().to_string(),
                    ],
                    dirty_overlay: None,
                    source_fingerprint: before.clone(),
                    lock_name: lock_name_for(&project.project_id),
                }
            }
            ProjectKind::Notes => {
                let result = materialize_non_git_project(
                    &source,
                    &dest,
                    &project.project_id,
                    &approved_roots,
                    true,
                )?;
                notes_roots.push(dest.display().to_string());
                // Notes are read-only snapshots; write_root still points at managed copy for metadata,
                // but validation later can treat notes as non-writable for agents.
                ManagedProject {
                    project_id: project.project_id.clone(),
                    source_root: project.root.clone(),
                    managed_root: result.managed_root.display().to_string(),
                    kind: result.kind,
                    baseline_commit: Some(result.baseline_commit),
                    baseline_branch: result.baseline_branch,
                    worktree_path: None,
                    write_root: dest.display().to_string(),
                    read_roots: vec![
                        managed_run_root.display().to_string(),
                        dest.display().to_string(),
                    ],
                    dirty_overlay: None,
                    source_fingerprint: before.clone(),
                    lock_name: lock_name_for(&project.project_id),
                }
            }
        };

        let after = capture_fingerprint(&source, project_kind_label(&project.kind))?;
        write_fingerprint_file(
            &managed_run_root
                .join("fingerprints")
                .join(format!("{}-after.json", project.project_id)),
            &after,
        )?;
        ensure_source_unchanged(&before, &after)?;
        let unchanged = true;
        source_unchanged &= unchanged;
        fingerprint_pairs.push(FingerprintPair {
            before,
            after,
            unchanged,
        });

        projects.push(managed);
    }

    let mut manifest = RunWorkspaceManifest {
        schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
        run_id: req.run_id,
        intake_id: req.intake.intake_id,
        managed_run_root: managed_run_root.display().to_string(),
        control_root: control_root.display().to_string(),
        projects,
        notes_roots,
        checkpoints: Vec::new(),
        quarantines: Vec::new(),
        promotion: PromotionMetadata {
            status: PromotionStatus::Unpromoted,
            export_path: None,
            promoted_at_utc: None,
            notes: None,
        },
        retention: RetentionPolicy::default(),
        fingerprint_pairs,
        created_at_utc: Utc::now(),
        source_unchanged,
    };

    // Initial checkpoints from intake baselines.
    let project_ids: Vec<String> = manifest
        .projects
        .iter()
        .map(|p| p.project_id.clone())
        .collect();
    for project_id in project_ids {
        let _ = create_checkpoint(&mut manifest, &project_id, "intake-baseline")?;
    }

    write_manifest(&manifest)?;
    Ok(manifest)
}

pub fn write_manifest(manifest: &RunWorkspaceManifest) -> WorkspaceResult<()> {
    let path = Path::new(&manifest.managed_run_root).join("manifest.json");
    let json = serde_json::to_vec_pretty(manifest)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn load_manifest(managed_run_root: &Path) -> WorkspaceResult<RunWorkspaceManifest> {
    let path = managed_run_root.join("manifest.json");
    let bytes = fs::read(&path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn checkpoint_project(
    managed_run_root: &Path,
    project_id: &str,
    message: &str,
) -> WorkspaceResult<RunWorkspaceManifest> {
    let mut manifest = load_manifest(managed_run_root)?;
    create_checkpoint(&mut manifest, project_id, message)?;
    write_manifest(&manifest)?;
    Ok(manifest)
}

pub fn quarantine_project_path(
    managed_run_root: &Path,
    project_id: &str,
    path: &Path,
    reason: &str,
) -> WorkspaceResult<RunWorkspaceManifest> {
    let mut manifest = load_manifest(managed_run_root)?;
    quarantine_path(&mut manifest, project_id, path, reason, None)?;
    write_manifest(&manifest)?;
    Ok(manifest)
}

pub fn export_managed_project(
    managed_run_root: &Path,
    project_id: &str,
) -> WorkspaceResult<RunWorkspaceManifest> {
    let mut manifest = load_manifest(managed_run_root)?;
    let export_dir = PathBuf::from(&manifest.managed_run_root).join("exports");
    export_project(&mut manifest, project_id, &export_dir)?;
    write_manifest(&manifest)?;
    Ok(manifest)
}

/// Export a managed project to an explicit destination directory (caller-chosen).
pub fn export_managed_project_to(
    managed_run_root: &Path,
    project_id: &str,
    export_dir: &Path,
) -> WorkspaceResult<RunWorkspaceManifest> {
    let mut manifest = load_manifest(managed_run_root)?;
    export_project(&mut manifest, project_id, export_dir)?;
    write_manifest(&manifest)?;
    Ok(manifest)
}

/// Re-capture source fingerprints and block when any source mutated since materialize (DATA-002).
pub fn recheck_source_fingerprints(manifest: &mut RunWorkspaceManifest) -> WorkspaceResult<()> {
    for project in &manifest.projects {
        let baseline = &project.source_fingerprint;
        let current = capture_fingerprint(Path::new(&baseline.path), &baseline.kind)?;
        ensure_source_unchanged(baseline, &current)?;
    }
    for pair in &manifest.fingerprint_pairs {
        let current = capture_fingerprint(Path::new(&pair.before.path), &pair.before.kind)?;
        ensure_source_unchanged(&pair.before, &current)?;
    }
    manifest.source_unchanged = true;
    Ok(())
}

/// Locate durable managed run root for `run_id` under known parents / explicit roots.
/// Candidates may be managed parents (`…/workspaces`) or the run root itself (`…/run-{id}`).
pub fn find_managed_run_root(run_id: Uuid, candidates: &[PathBuf]) -> Option<PathBuf> {
    let run_dir = format!("run-{run_id}");
    for candidate in candidates {
        if candidate.join("manifest.json").is_file() {
            if let Ok(manifest) = load_manifest(candidate) {
                if manifest.run_id == run_id {
                    return Some(candidate.clone());
                }
            }
        }
        let nested = candidate.join(&run_dir);
        if nested.join("manifest.json").is_file() {
            if let Ok(manifest) = load_manifest(&nested) {
                if manifest.run_id == run_id {
                    return Some(nested);
                }
            } else {
                return Some(nested);
            }
        }
    }
    None
}

/// DATA-002: load workspace manifest from disk for the run and re-check source fingerprints.
/// Returns `Ok(None)` when no durable manifest exists under the search roots (nothing to gate).
/// On mutation, persists `source_unchanged=false` and returns `SourceMutated`.
pub fn recheck_source_fingerprints_for_run(
    run_id: Uuid,
    search_roots: &[PathBuf],
) -> WorkspaceResult<Option<RunWorkspaceManifest>> {
    let Some(root) = find_managed_run_root(run_id, search_roots) else {
        return Ok(None);
    };
    let mut manifest = load_manifest(&root)?;
    if let Err(e) = recheck_source_fingerprints(&mut manifest) {
        manifest.source_unchanged = false;
        let _ = write_manifest(&manifest);
        return Err(e);
    }
    let _ = write_manifest(&manifest);
    Ok(Some(manifest))
}

/// Record that the user accepted managed output for external merge/use.
pub fn promote_managed_run(
    managed_run_root: &Path,
    notes: Option<String>,
) -> WorkspaceResult<RunWorkspaceManifest> {
    let mut manifest = load_manifest(managed_run_root)?;
    // DATA-002: block promote when source inputs mutated since materialize.
    if let Err(e) = recheck_source_fingerprints(&mut manifest) {
        manifest.source_unchanged = false;
        let _ = write_manifest(&manifest);
        return Err(e);
    }
    crate::workspace::mark_promoted(&mut manifest, notes);
    write_manifest(&manifest)?;
    Ok(manifest)
}

fn project_kind_label(kind: &ProjectKind) -> &'static str {
    match kind {
        ProjectKind::Git => "git",
        ProjectKind::Folder => "folder",
        ProjectKind::Notes => "notes",
    }
}
