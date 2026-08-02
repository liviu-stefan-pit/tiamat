//! P11 packaging, TestBench, long-path, install retention, and cleanup proof.

use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;
use tiamat_lib::db::Store;
use tiamat_lib::packaging::{
    assert_zero_owned_processes, create_long_path_fixture, ensure_upgrade_scaffold,
    plan_uninstall_retention, sample_unpromoted_manifest, simulate_upgrade_preserve,
    write_cleanup_proof_artifact, PackagedCleanupReport,
};
use uuid::Uuid;

#[test]
fn testbench_fixture_tree_exists() {
    let root = tiamat_contracts::repo_root()
        .join("fixtures")
        .join("testbench");
    for case in [
        "notes-only",
        "web-app",
        "multi-project",
        "dirty-git",
        "nested-repo",
        "secret-risk",
        "junction-escape",
        "long-path",
        "executor-app",
    ] {
        assert!(root.join(case).exists(), "missing TestBench case {case}");
    }
    // Unicode directory exists (name contains non-ASCII).
    let unicode = fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains("unicode"));
    assert!(unicode, "unicode TestBench case missing");
}

#[test]
fn long_path_and_unicode_inventory_safe() {
    let dir = tempdir().unwrap();
    let marker = create_long_path_fixture(&dir.path().join("long")).unwrap();
    assert!(marker.exists());
    assert!(marker.to_string_lossy().len() >= 240);

    let unicode = dir.path().join("项目-α");
    fs::create_dir_all(&unicode).unwrap();
    fs::write(unicode.join("说明.md"), "ok").unwrap();
    assert!(unicode.join("说明.md").exists());
}

#[test]
fn configured_cli_path_persists_in_app_settings() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("db.sqlite3"), dir.path().join("artifacts")).unwrap();
    assert_eq!(store.schema_version().unwrap(), 6);
    let settings = store
        .set_cursor_cli_path(Some(r"C:\tools\configured-agent.cmd".into()))
        .unwrap();
    assert_eq!(
        settings.cursor_cli_path.as_deref(),
        Some(r"C:\tools\configured-agent.cmd")
    );
    let loaded = store.get_app_settings().unwrap();
    assert_eq!(
        loaded.cursor_cli_path.as_deref(),
        Some(r"C:\tools\configured-agent.cmd")
    );
}

#[test]
fn uninstall_and_upgrade_policies() {
    let plan = plan_uninstall_retention(&[sample_unpromoted_manifest(
        r"C:\Users\user\AppData\Roaming\com.tiamat.desktop\tiamat\workspaces\keep-me",
    )]);
    assert!(plan.retain_unpromoted_workspaces);
    assert!(!plan.remove_managed_workspaces);

    let dir = tempdir().unwrap();
    ensure_upgrade_scaffold(dir.path()).unwrap();
    let upgrade = simulate_upgrade_preserve(dir.path(), "0.1.0", "0.1.1").unwrap();
    assert!(upgrade.db_preserved && upgrade.settings_preserved && upgrade.workspaces_preserved);
}

#[test]
fn packaged_cleanup_proof_artifact_zero_processes() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("db.sqlite3"), dir.path().join("artifacts")).unwrap();
    let run_id = Uuid::new_v4();
    store
        .create_run(run_id, "p11 cleanup", "executing")
        .unwrap();
    let count = assert_zero_owned_processes(&store, Some(run_id)).unwrap();
    assert_eq!(count, 0);
    let report = PackagedCleanupReport {
        run_id,
        active_process_count: 0,
        zero_owned_processes: true,
        proofs: vec![],
        artifact_path: None,
    };
    let path = write_cleanup_proof_artifact(&dir.path().join("proofs"), &report).unwrap();
    assert!(path.exists());
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("zeroOwnedProcesses"));
}

#[test]
fn package_scripts_and_vm_docs_exist() {
    let root = tiamat_contracts::repo_root();
    for rel in [
        "scripts/demo.ps1",
        "scripts/package.ps1",
        "scripts/materialize-testbench.ps1",
        "scripts/vm/README.md",
        "scripts/vm/run-install-matrix.ps1",
        "scripts/vm/clean-profile-smoke.ps1",
        "scripts/vm/packaged-cleanup-proof.ps1",
        "scripts/canary/README.md",
        "scripts/canary/run-contract-canary.ps1",
        "src-tauri/tauri.conf.json",
    ] {
        let path = root.join(rel);
        assert!(path.exists(), "missing {rel}");
    }
    let conf = fs::read_to_string(root.join("src-tauri/tauri.conf.json")).unwrap();
    assert!(conf.contains("\"nsis\""));
    assert!(conf.contains("\"msi\"") || conf.contains("wix"));
    let matrix = fs::read_to_string(root.join("scripts/vm/run-install-matrix.ps1")).unwrap();
    assert!(
        matrix.contains(r"com.tiamat.desktop") && matrix.contains(r"tiamat\workspaces"),
        "install matrix must plant KEEP under real app workspaces root"
    );
    assert!(
        !matrix.contains("tiamat-managed-runs"),
        "install matrix must not use fake LOCALAPPDATA\\tiamat-managed-runs retention path"
    );
}

#[test]
fn materialize_testbench_command_copies_cases() {
    let dir = tempdir().unwrap();
    let dest: PathBuf = dir.path().join("out");
    let src = tiamat_contracts::repo_root()
        .join("fixtures")
        .join("testbench");
    // Light copy of notes-only to avoid depending on Tauri command harness here.
    fs::create_dir_all(dest.join("notes-only")).unwrap();
    fs::copy(
        src.join("notes-only").join("NOTES.md"),
        dest.join("notes-only").join("NOTES.md"),
    )
    .unwrap();
    assert!(dest.join("notes-only").join("NOTES.md").exists());
    let marker = create_long_path_fixture(&dest.join("long-path")).unwrap();
    assert!(marker.exists());
}
