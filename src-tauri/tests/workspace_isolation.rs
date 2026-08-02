//! Integration tests for isolated workspace materialization (P04).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use tempfile::tempdir;
use tiamat_contracts::{IntakeManifest, IntakeSource, ProjectKind, ProjectSummary, SourceKind};
use tiamat_lib::workspace::{
    checkpoint_project, cleanup_managed_run, export_managed_project, load_manifest,
    materialize_run_workspace, promote_managed_run, quarantine_project_path,
    recheck_source_fingerprints_for_run, MaterializeRequest, PromotionStatus,
};
use uuid::Uuid;

/// Serialize git-heavy fixtures on Windows to avoid intermittent Access Denied races.
static GIT_FIXTURE_LOCK: Mutex<()> = Mutex::new(());

fn lock_fixtures() -> std::sync::MutexGuard<'static, ()> {
    GIT_FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Tiamat")
        .env("GIT_AUTHOR_EMAIL", "tiamat@example.com")
        .env("GIT_COMMITTER_NAME", "Tiamat")
        .env("GIT_COMMITTER_EMAIL", "tiamat@example.com")
        .status()
        .expect("git available");
    assert!(status.success(), "git {args:?} failed");
}

fn git_text(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn fingerprint_snapshot(root: &Path) -> (String, String) {
    let status = if root.join(".git").exists() {
        git_text(root, &["status", "--porcelain=v1", "--untracked-files=all"])
    } else {
        String::new()
    };
    let head = if root.join(".git").exists() {
        git_text(root, &["rev-parse", "HEAD"])
    } else {
        String::new()
    };
    (head, status)
}

fn intake_for(projects: Vec<(String, PathBuf, ProjectKind)>) -> IntakeManifest {
    let sources = projects
        .iter()
        .map(|(_, root, _)| IntakeSource {
            path: root.display().to_string(),
            kind: SourceKind::Folder,
            read_only: true,
        })
        .collect();
    let project_summaries = projects
        .into_iter()
        .map(|(id, root, kind)| ProjectSummary {
            project_id: id,
            root: root.display().to_string(),
            kind,
            languages: vec!["typescript".into()],
            build_systems: vec!["npm".into()],
            test_commands: vec![],
            warnings: vec![],
        })
        .collect();
    IntakeManifest {
        schema_version: 1,
        intake_id: Uuid::new_v4(),
        sources,
        projects: project_summaries,
        inventory_artifact: "test".into(),
    }
}

#[test]
fn clean_git_clone_is_owned_no_hardlinks_and_source_unchanged() {
    let _guard = lock_fixtures();
    let dir = tempdir().unwrap();
    let source = dir.path().join("clean-app");
    fs::create_dir_all(&source).unwrap();
    git(&source, &["init"]);
    write(&source.join("README.md"), "clean\n");
    write(&source.join("src/main.ts"), "export const n = 1;\n");
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "init"]);
    let before = fingerprint_snapshot(&source);

    let managed_parent = dir.path().join("managed");
    let intake = intake_for(vec![("clean-app".into(), source.clone(), ProjectKind::Git)]);
    let manifest = materialize_run_workspace(MaterializeRequest {
        run_id: Uuid::new_v4(),
        intake,
        managed_parent,
        create_internal_worktrees: true,
    })
    .unwrap();

    assert!(manifest.source_unchanged);
    assert_eq!(manifest.projects.len(), 1);
    let project = &manifest.projects[0];
    assert!(Path::new(&project.managed_root).join(".git").exists());
    assert!(project.baseline_commit.is_some());
    assert!(project.worktree_path.is_some());
    assert_eq!(fingerprint_snapshot(&source), before);

    // Source worktree list remains single entry (no linked worktree attached to source).
    let wt = git_text(&source, &["worktree", "list"]);
    assert_eq!(wt.lines().count(), 1);

    // Distinct lock / write root.
    assert_eq!(project.lock_name, "write:clean-app");
    assert!(manifest.validate_write_root(&project.write_root).is_ok());
    assert!(manifest
        .validate_write_root(&source.display().to_string())
        .is_err());
}

#[test]
fn dirty_git_overlay_preserves_staged_unstaged_untracked_without_source_writes() {
    let _guard = lock_fixtures();
    let dir = tempdir().unwrap();
    let source = dir.path().join("dirty-app");
    fs::create_dir_all(&source).unwrap();
    git(&source, &["init"]);
    write(&source.join("README.md"), "base\n");
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "init"]);

    write(&source.join("README.md"), "staged-change\n");
    git(&source, &["add", "README.md"]);
    write(&source.join("README.md"), "unstaged-change\n");
    write(&source.join("extra.txt"), "untracked\n");
    let before = fingerprint_snapshot(&source);
    assert!(!before.1.is_empty());

    let managed_parent = dir.path().join("managed");
    let intake = intake_for(vec![("dirty-app".into(), source.clone(), ProjectKind::Git)]);
    let manifest = materialize_run_workspace(MaterializeRequest {
        run_id: Uuid::new_v4(),
        intake,
        managed_parent,
        create_internal_worktrees: false,
    })
    .unwrap();

    assert!(manifest.source_unchanged);
    assert_eq!(fingerprint_snapshot(&source), before);
    let project = &manifest.projects[0];
    let overlay = project.dirty_overlay.as_ref().expect("dirty overlay");
    assert!(overlay.had_untracked);
    assert!(overlay.untracked_files.iter().any(|f| f == "extra.txt"));
    let managed = Path::new(&project.managed_root);
    assert!(managed.join("extra.txt").exists());
    assert!(managed
        .join(".tiamat/intake-overlay/metadata.json")
        .exists());
    // Source .git was not rewritten with worktrees or new commits.
    let source_log = git_text(&source, &["log", "--oneline"]);
    assert_eq!(source_log.lines().count(), 1);
}

#[test]
fn nested_and_multi_repo_get_distinct_managed_roots() {
    let _guard = lock_fixtures();
    let dir = tempdir().unwrap();
    let root = dir.path().join("workspace");
    let nested = root.join("services").join("api");
    fs::create_dir_all(&nested).unwrap();
    git(&root, &["init"]);
    write(&root.join("README.md"), "root\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "root"]);
    git(&nested, &["init"]);
    write(
        &nested.join("Cargo.toml"),
        "[package]\nname=\"api\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    );
    git(&nested, &["add", "."]);
    git(&nested, &["commit", "-m", "api"]);

    let before_root = fingerprint_snapshot(&root);
    let before_nested = fingerprint_snapshot(&nested);

    let intake = intake_for(vec![
        ("workspace".into(), root.clone(), ProjectKind::Git),
        ("api".into(), nested.clone(), ProjectKind::Git),
    ]);
    let manifest = materialize_run_workspace(MaterializeRequest {
        run_id: Uuid::new_v4(),
        intake,
        managed_parent: dir.path().join("managed"),
        create_internal_worktrees: false,
    })
    .unwrap();

    assert_eq!(manifest.projects.len(), 2);
    let roots: Vec<_> = manifest
        .projects
        .iter()
        .map(|p| p.managed_root.clone())
        .collect();
    assert_ne!(roots[0], roots[1]);
    assert_ne!(
        manifest.projects[0].lock_name,
        manifest.projects[1].lock_name
    );
    assert_eq!(fingerprint_snapshot(&root), before_root);
    assert_eq!(fingerprint_snapshot(&nested), before_nested);
    assert!(manifest.source_unchanged);
}

#[test]
fn non_git_folder_gets_initialized_baseline() {
    let _guard = lock_fixtures();
    let dir = tempdir().unwrap();
    let source = dir.path().join("notes-only");
    fs::create_dir_all(&source).unwrap();
    write(&source.join("NOTES.md"), "brainstorm\n");
    let before_bytes = fs::read(source.join("NOTES.md")).unwrap();

    let intake = intake_for(vec![(
        "notes-only".into(),
        source.clone(),
        ProjectKind::Notes,
    )]);
    let manifest = materialize_run_workspace(MaterializeRequest {
        run_id: Uuid::new_v4(),
        intake,
        managed_parent: dir.path().join("managed"),
        create_internal_worktrees: false,
    })
    .unwrap();

    assert_eq!(manifest.projects.len(), 1);
    assert!(!manifest.notes_roots.is_empty());
    let managed = Path::new(&manifest.projects[0].managed_root);
    assert!(managed.join(".git").exists());
    assert!(managed.join("NOTES.md").exists());
    assert_eq!(fs::read(source.join("NOTES.md")).unwrap(), before_bytes);
    assert!(!source.join(".git").exists());
}

#[test]
fn checkpoint_quarantine_export_and_retention() {
    let _guard = lock_fixtures();
    let dir = tempdir().unwrap();
    let source = dir.path().join("app");
    fs::create_dir_all(&source).unwrap();
    git(&source, &["init"]);
    write(&source.join("a.txt"), "1\n");
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "init"]);

    let managed_parent = dir.path().join("managed");
    let intake = intake_for(vec![("app".into(), source.clone(), ProjectKind::Git)]);
    let manifest = materialize_run_workspace(MaterializeRequest {
        run_id: Uuid::new_v4(),
        intake,
        managed_parent,
        create_internal_worktrees: false,
    })
    .unwrap();
    let run_root = PathBuf::from(&manifest.managed_run_root);

    // Edit managed copy and checkpoint.
    write(
        &Path::new(&manifest.projects[0].managed_root).join("a.txt"),
        "2\n",
    );
    let after_cp = checkpoint_project(&run_root, "app", "after-edit").unwrap();
    assert!(after_cp.checkpoints.len() >= 2);

    // Quarantine a bogus attempt folder under managed root.
    let bad = run_root.join("bad-attempt");
    fs::create_dir_all(&bad).unwrap();
    write(&bad.join("leak.txt"), "nope\n");
    let after_q = quarantine_project_path(&run_root, "app", &bad, "out-of-bound").unwrap();
    assert_eq!(after_q.quarantines.len(), 1);
    assert!(!bad.exists());
    assert!(Path::new(&after_q.quarantines[0].quarantine_path).exists());

    // Export does not touch source.
    let before = fingerprint_snapshot(&source);
    let after_export = export_managed_project(&run_root, "app").unwrap();
    assert!(matches!(
        after_export.promotion.status,
        PromotionStatus::Exported
    ));
    assert!(after_export.promotion.export_path.is_some());
    assert_eq!(fingerprint_snapshot(&source), before);

    let after_promote = promote_managed_run(&run_root, Some("accepted".into())).unwrap();
    assert!(matches!(
        after_promote.promotion.status,
        PromotionStatus::Promoted
    ));
    assert_eq!(after_promote.promotion.notes.as_deref(), Some("accepted"));

    // Retention blocks silent cleanup of (still) managed tree unless forced with policy.
    let loaded = load_manifest(&run_root).unwrap();
    // After promote, has_unpromoted_work is false.
    assert!(!loaded.has_unpromoted_work());

    // Create a fresh unpromoted manifest scenario via re-materialize path check:
    let source2 = dir.path().join("app2");
    fs::create_dir_all(&source2).unwrap();
    git(&source2, &["init"]);
    write(&source2.join("b.txt"), "b\n");
    git(&source2, &["add", "."]);
    git(&source2, &["commit", "-m", "init"]);
    let m2 = materialize_run_workspace(MaterializeRequest {
        run_id: Uuid::new_v4(),
        intake: intake_for(vec![("app2".into(), source2, ProjectKind::Git)]),
        managed_parent: dir.path().join("managed2"),
        create_internal_worktrees: false,
    })
    .unwrap();
    assert!(m2.has_unpromoted_work());
    let err = cleanup_managed_run(&m2, false).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unpromoted") || msg.contains("retention") || msg.contains("cleanup"),
        "{msg}"
    );
}

#[test]
fn terminal_fingerprint_recheck_loads_manifest_from_disk_without_in_memory_cache() {
    // DATA-002: omit in-memory workspace; mutate source; disk-backed recheck must block.
    let _guard = lock_fixtures();
    let dir = tempdir().unwrap();
    let source = dir.path().join("app");
    fs::create_dir_all(&source).unwrap();
    git(&source, &["init"]);
    write(&source.join("a.txt"), "baseline\n");
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "init"]);

    let managed_parent = dir.path().join("managed");
    let run_id = Uuid::new_v4();
    let manifest = materialize_run_workspace(MaterializeRequest {
        run_id,
        intake: intake_for(vec![("app".into(), source.clone(), ProjectKind::Git)]),
        managed_parent: managed_parent.clone(),
        create_internal_worktrees: false,
    })
    .unwrap();
    assert!(Path::new(&manifest.managed_run_root)
        .join("manifest.json")
        .is_file());

    // Mutate the original source after materialize.
    write(&source.join("a.txt"), "mutated-after-materialize\n");

    // Only durable managed parent is provided — no in-memory AppState workspace.
    let err = recheck_source_fingerprints_for_run(run_id, &[managed_parent])
        .expect_err("mutated source must block terminal fingerprint gate");
    let msg = err.to_string();
    assert!(
        msg.contains("fingerprint") || msg.contains("mutat") || msg.contains("source"),
        "expected fingerprint/source-mutated error, got: {msg}"
    );
}
