//! Allocate empty NonGit product roots for notes-only / brainstorm intakes.
//!
//! Isolated layout: `managed_run_root/projects/<slug>`.
//! Flat notes layout: the product write root is `managed_run_root` itself.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use tiamat_contracts::ProjectPlan;

use crate::workspace::checkpoint::create_checkpoint;
use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::git::{configure_identity, git, git_text};
use crate::workspace::roots::{is_within_managed, lock_name_for};
use crate::workspace::types::{
    ManagedProject, ManagedProjectKind, RunWorkspaceManifest, SourceFingerprint,
};

pub const DEFAULT_GREENFIELD_PROJECT_ID: &str = "app";

/// Flat notes-only layout: product + `.tiamat/` live at `managed_run_root` (control_root == managed).
pub fn is_flat_layout(workspace: &RunWorkspaceManifest) -> bool {
    normalize_path(&workspace.control_root) == normalize_path(&workspace.managed_run_root)
}

/// Safe project id / directory leaf under `projects/`.
pub fn is_safe_project_slug(project_id: &str) -> bool {
    let id = project_id.trim();
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && !id.contains("..")
}

pub fn is_writable_project_kind(kind: &ManagedProjectKind) -> bool {
    !matches!(kind, ManagedProjectKind::NotesSnapshot)
}

pub fn has_writable_project(workspace: &RunWorkspaceManifest) -> bool {
    workspace
        .projects
        .iter()
        .any(|p| is_writable_project_kind(&p.kind))
}

pub fn greenfield_project_path(workspace: &RunWorkspaceManifest, project_id: &str) -> PathBuf {
    let id = project_id.trim();
    if is_flat_layout(workspace) {
        // Flat notes-only: the single product root is the managed run root itself.
        return PathBuf::from(&workspace.managed_run_root);
    }
    Path::new(&workspace.managed_run_root)
        .join("projects")
        .join(id)
}

/// If `root` is the flat product root or `{managed}/projects/{slug}`, return the project id.
pub fn greenfield_slug_from_root(workspace: &RunWorkspaceManifest, root: &str) -> Option<String> {
    let trimmed = root.trim();
    if trimmed.is_empty() {
        return None;
    }
    let cand = normalize_path(trimmed);
    if is_flat_layout(workspace) {
        let managed = normalize_path(&workspace.managed_run_root);
        if cand == managed || cand.starts_with(&format!("{managed}/")) {
            return workspace
                .projects
                .iter()
                .find(|p| is_writable_project_kind(&p.kind))
                .map(|p| p.project_id.clone())
                .or_else(|| Some(DEFAULT_GREENFIELD_PROJECT_ID.to_string()));
        }
        return None;
    }
    let projects_root = normalize_path(
        &Path::new(&workspace.managed_run_root)
            .join("projects")
            .to_string_lossy(),
    );
    if cand == projects_root {
        return None;
    }
    let prefix = format!("{projects_root}/");
    let rest = cand.strip_prefix(&prefix)?;
    let slug = rest.split('/').next().unwrap_or("");
    if slug.is_empty() || !is_safe_project_slug(slug) {
        return None;
    }
    // Reject escapes that somehow passed normalize (shouldn't happen).
    if !is_within_managed(&projects_root, &cand) && cand != projects_root {
        return None;
    }
    Some(slug.to_string())
}

pub fn is_allocatable_greenfield_project_id(
    workspace: &RunWorkspaceManifest,
    project_id: &str,
) -> bool {
    if !is_safe_project_slug(project_id) {
        return false;
    }
    if is_flat_layout(workspace) {
        // Flat mode: only the existing root product id (or default) is allocatable.
        return workspace
            .projects
            .iter()
            .any(|p| p.project_id == project_id && is_writable_project_kind(&p.kind))
            || project_id == DEFAULT_GREENFIELD_PROJECT_ID
            || project_id == "implementation";
    }
    if workspace
        .projects
        .iter()
        .any(|p| p.project_id == project_id)
    {
        return is_writable_project_kind(
            &workspace
                .projects
                .iter()
                .find(|p| p.project_id == project_id)
                .unwrap()
                .kind,
        );
    }
    true
}

/// Init an empty NonGit project (isolated: `projects/<id>`; flat: managed root) and append it.
pub fn ensure_greenfield_project(
    workspace: &mut RunWorkspaceManifest,
    project_id: &str,
) -> WorkspaceResult<String> {
    let id = project_id.trim();
    if !is_safe_project_slug(id) {
        return Err(WorkspaceError::Message(format!(
            "unsafe greenfield project id: {id}"
        )));
    }
    if let Some(existing) = workspace.projects.iter().find(|p| p.project_id == id) {
        if is_writable_project_kind(&existing.kind) {
            return Ok(existing.write_root.clone());
        }
        return Err(WorkspaceError::Message(format!(
            "project id {id} exists as notes snapshot and is not writable"
        )));
    }

    if is_flat_layout(workspace) {
        // Flat workspaces already have the product root; map any requested id onto it
        // only when it matches the default / implementation naming — otherwise reject.
        if let Some(existing) = workspace
            .projects
            .iter()
            .find(|p| is_writable_project_kind(&p.kind))
        {
            if id == existing.project_id
                || id == DEFAULT_GREENFIELD_PROJECT_ID
                || id == "implementation"
            {
                return Ok(existing.write_root.clone());
            }
        }
        return Err(WorkspaceError::Message(format!(
            "flat notes workspace only allows writing to the product root (requested id {id})"
        )));
    }

    let dest = greenfield_project_path(workspace, id);
    if dest.exists() {
        // Directory may exist from a prior partial run; require it be empty of conflicting git or reuse.
        if dest.join(".git").is_dir() {
            let write_root = dest.display().to_string();
            let baseline = git_text(&dest, &["rev-parse", "HEAD"]).unwrap_or_default();
            let branch = git_text(&dest, &["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_else(|_| format!("tiamat/greenfield-{id}"));
            workspace.projects.push(ManagedProject {
                project_id: id.to_string(),
                source_root: format!("greenfield:{id}"),
                managed_root: write_root.clone(),
                kind: ManagedProjectKind::NonGitCopy,
                baseline_commit: if baseline.is_empty() {
                    None
                } else {
                    Some(baseline)
                },
                baseline_branch: branch,
                worktree_path: None,
                write_root: write_root.clone(),
                read_roots: vec![workspace.managed_run_root.clone(), write_root.clone()],
                dirty_overlay: None,
                source_fingerprint: synthetic_fingerprint(id),
                lock_name: lock_name_for(id),
            });
            return Ok(write_root);
        }
    }

    fs::create_dir_all(&dest)?;
    git(&dest, &["init"])?;
    configure_identity(&dest)?;
    let branch = format!("tiamat/greenfield-{id}");
    git(&dest, &["checkout", "-B", &branch])?;
    git(
        &dest,
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("tiamat greenfield baseline for {id}"),
        ],
    )?;
    let baseline_commit = git_text(&dest, &["rev-parse", "HEAD"])?;
    let write_root = dest.display().to_string();

    workspace.projects.push(ManagedProject {
        project_id: id.to_string(),
        source_root: format!("greenfield:{id}"),
        managed_root: write_root.clone(),
        kind: ManagedProjectKind::NonGitCopy,
        baseline_commit: Some(baseline_commit),
        baseline_branch: branch,
        worktree_path: None,
        write_root: write_root.clone(),
        read_roots: vec![workspace.managed_run_root.clone(), write_root.clone()],
        dirty_overlay: None,
        source_fingerprint: synthetic_fingerprint(id),
        lock_name: lock_name_for(id),
    });

    Ok(write_root)
}

/// When intake has only notes (no writable code projects), allocate `projects/app`
/// (or `implementation` if `app` is already a notes project id).
pub fn ensure_default_greenfield_if_needed(
    workspace: &mut RunWorkspaceManifest,
) -> WorkspaceResult<Option<String>> {
    if has_writable_project(workspace) {
        return Ok(None);
    }
    let id = if workspace
        .projects
        .iter()
        .any(|p| p.project_id == DEFAULT_GREENFIELD_PROJECT_ID)
    {
        "implementation"
    } else {
        DEFAULT_GREENFIELD_PROJECT_ID
    };
    let root = ensure_greenfield_project(workspace, id)?;
    let _ = create_checkpoint(workspace, id, "intake-baseline")?;
    Ok(Some(root))
}

/// Collect greenfield slugs referenced by the plan that are not yet writable projects.
pub fn collect_missing_greenfield_ids(
    plan: &ProjectPlan,
    workspace: &RunWorkspaceManifest,
) -> Vec<String> {
    let mut ids = Vec::new();
    for phase in &plan.phases {
        for pid in &phase.project_ids {
            if workspace
                .projects
                .iter()
                .any(|p| p.project_id == *pid && is_writable_project_kind(&p.kind))
            {
                continue;
            }
            if is_allocatable_greenfield_project_id(workspace, pid) {
                push_unique(&mut ids, pid.clone());
            }
        }
        for root in &phase.write_roots {
            if let Some(slug) = greenfield_slug_from_root(workspace, root) {
                if workspace
                    .projects
                    .iter()
                    .any(|p| p.project_id == slug && is_writable_project_kind(&p.kind))
                {
                    continue;
                }
                push_unique(&mut ids, slug);
            }
        }
    }
    ids
}

/// Materialize any plan-referenced greenfield projects and checkpoint them.
pub fn bootstrap_plan_greenfield_projects(
    workspace: &mut RunWorkspaceManifest,
    plan: &ProjectPlan,
) -> WorkspaceResult<Vec<String>> {
    let missing = collect_missing_greenfield_ids(plan, workspace);
    let mut created = Vec::new();
    for id in missing {
        ensure_greenfield_project(workspace, &id)?;
        let _ = create_checkpoint(workspace, &id, "greenfield-baseline")?;
        created.push(id);
    }
    Ok(created)
}

fn synthetic_fingerprint(project_id: &str) -> SourceFingerprint {
    SourceFingerprint {
        path: format!("greenfield:{project_id}"),
        kind: "greenfield".into(),
        head: None,
        branch: None,
        status_porcelain: String::new(),
        status_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        tree_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        captured_at_utc: Utc::now(),
    }
}

fn push_unique(out: &mut Vec<String>, value: String) {
    if !out.iter().any(|v| v == &value) {
        out.push(value);
    }
}

fn normalize_path(s: &str) -> String {
    s.replace('\\', "/").trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::types::{PromotionMetadata, PromotionStatus, RetentionPolicy};
    use tempfile::tempdir;
    use uuid::Uuid;

    fn empty_notes_workspace(managed: &Path) -> RunWorkspaceManifest {
        RunWorkspaceManifest {
            schema_version: 1,
            run_id: Uuid::nil(),
            intake_id: Uuid::nil(),
            managed_run_root: managed.display().to_string(),
            control_root: managed.join("control").display().to_string(),
            projects: vec![],
            notes_roots: vec![managed.join("notes").join("spec").display().to_string()],
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
            created_at_utc: Utc::now(),
            source_unchanged: true,
        }
    }

    #[test]
    fn safe_slug_rules() {
        assert!(is_safe_project_slug("app"));
        assert!(is_safe_project_slug("skill-rule-evaluation-engine"));
        assert!(!is_safe_project_slug("../escape"));
        assert!(!is_safe_project_slug("Bad Id"));
        assert!(!is_safe_project_slug(""));
    }

    #[test]
    fn allocates_default_app_when_notes_only() {
        let dir = tempdir().unwrap();
        let managed = dir.path().join("run-1");
        fs::create_dir_all(managed.join("projects")).unwrap();
        fs::create_dir_all(managed.join("notes").join("spec")).unwrap();
        let mut ws = empty_notes_workspace(&managed);
        let root = ensure_default_greenfield_if_needed(&mut ws).unwrap();
        assert!(root.is_some());
        assert!(has_writable_project(&ws));
        assert!(managed.join("projects").join("app").join(".git").is_dir());
    }

    #[test]
    fn flat_layout_maps_write_root_to_product_root() {
        let dir = tempdir().unwrap();
        let managed = dir.path().join("out");
        fs::create_dir_all(&managed).unwrap();
        let mut ws = empty_notes_workspace(&managed);
        ws.control_root = managed.display().to_string();
        assert!(is_flat_layout(&ws));
        assert_eq!(
            greenfield_project_path(&ws, "app"),
            PathBuf::from(&ws.managed_run_root)
        );
        assert_eq!(
            greenfield_slug_from_root(&ws, &managed.display().to_string()).as_deref(),
            Some("app")
        );
    }

    #[test]
    fn slug_from_write_root() {
        let dir = tempdir().unwrap();
        let managed = dir.path().join("run-1");
        fs::create_dir_all(managed.join("projects")).unwrap();
        let ws = empty_notes_workspace(&managed);
        let path = managed
            .join("projects")
            .join("skill-rule-evaluation-engine");
        assert_eq!(
            greenfield_slug_from_root(&ws, &path.display().to_string()).as_deref(),
            Some("skill-rule-evaluation-engine")
        );
    }

    #[test]
    fn bootstrap_creates_architect_invented_slug() {
        use tiamat_contracts::{
            ModelTier, PhasePlan, PhaseStatus, ProjectPlan, RollbackSpec, RollbackStrategy,
        };

        let dir = tempdir().unwrap();
        let managed = dir.path().join("run-1");
        fs::create_dir_all(managed.join("projects")).unwrap();
        fs::create_dir_all(managed.join("notes").join("spec")).unwrap();
        let mut ws = empty_notes_workspace(&managed);
        ensure_default_greenfield_if_needed(&mut ws).unwrap();

        let write_root = managed
            .join("projects")
            .join("skill-rule-evaluation-engine")
            .display()
            .to_string();
        let plan = ProjectPlan {
            schema_version: 1,
            run_id: Uuid::nil(),
            title: "Engine".into(),
            summary: "s".into(),
            assumptions: vec![],
            risks: vec![],
            phases: vec![PhasePlan {
                phase_id: "P01".into(),
                title: "Bootstrap".into(),
                objective: "Scaffold. Integration tests inapplicable. E2E tests inapplicable."
                    .into(),
                dependencies: vec![],
                project_ids: vec!["skill-rule-evaluation-engine".into()],
                read_roots: vec![managed.display().to_string()],
                write_roots: vec![write_root.clone()],
                model_tier: ModelTier::Composer,
                estimated_minutes: 10,
                acceptance_criteria: vec![],
                unit_tests: vec![],
                integration_tests: vec![],
                e2e_tests: vec![],
                manual_checks: vec![],
                rollback: RollbackSpec {
                    checkpoint: "intake-baseline".into(),
                    strategy: RollbackStrategy::Restore,
                },
                expected_artifacts: vec![],
                prompt: "Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json.".into(),
                status: PhaseStatus::Draft,
                evidence: vec![],
            }],
            final_gates: vec![],
        };

        let created = bootstrap_plan_greenfield_projects(&mut ws, &plan).unwrap();
        assert!(created
            .iter()
            .any(|id| id == "skill-rule-evaluation-engine"));
        assert!(Path::new(&write_root).join(".git").is_dir());
        assert!(ws
            .projects
            .iter()
            .any(|p| p.project_id == "skill-rule-evaluation-engine"
                && matches!(p.kind, ManagedProjectKind::NonGitCopy)));
    }
}
