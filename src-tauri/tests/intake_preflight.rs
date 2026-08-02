//! Intake integration fixtures: git, nested repos, unicode, junctions, secrets.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use tiamat_lib::intake::{apply_trust, assert_no_secret_leak, run_preflight, IntakeLimits};

const FIXTURE_SECRET: &str = "AKIAIOSFODNN7EXAMPLE";
const FIXTURE_ASSIGNMENT: &str = "api_key=\"fixture-secret-value-do-not-leak\"";

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

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn preflight_detects_git_language_and_tests() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("sample-app");
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init"]);
    write(
        &root.join("package.json"),
        r#"{"name":"sample","scripts":{"test":"vitest run"}}"#,
    );
    write(&root.join("tsconfig.json"), "{}");
    write(&root.join("src/main.ts"), " console.log('hi');\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "init"]);

    let report = run_preflight(
        &[root.to_string_lossy().to_string()],
        IntakeLimits::default(),
    )
    .unwrap();
    assert!(report.blockers.is_empty());
    assert_eq!(report.manifest.projects.len(), 1);
    let project = &report.manifest.projects[0];
    assert_eq!(project.kind, tiamat_contracts::ProjectKind::Git);
    assert!(project.languages.iter().any(|l| l == "typescript"));
    assert!(project.build_systems.iter().any(|b| b == "npm"));
    assert!(!report.can_start);

    let trusted = apply_trust(report, true, true);
    assert!(trusted.trust.confirmed);
    assert!(trusted.can_start);
}

#[test]
fn nested_repo_surfaces_as_separate_project() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("workspace");
    let nested = root.join("services").join("api");
    fs::create_dir_all(&nested).unwrap();
    git(&root, &["init"]);
    write(&root.join("README.md"), "workspace\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "root"]);

    git(&nested, &["init"]);
    write(
        &nested.join("Cargo.toml"),
        "[package]\nname=\"api\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    );
    write(&nested.join("src/lib.rs"), "pub fn x() {}\n");
    git(&nested, &["add", "."]);
    git(&nested, &["commit", "-m", "nested"]);

    let report = run_preflight(
        &[root.to_string_lossy().to_string()],
        IntakeLimits::default(),
    )
    .unwrap();
    assert!(report.manifest.projects.len() >= 2);
    assert!(report
        .warnings
        .iter()
        .any(|w| w.to_lowercase().contains("nested")));
}

#[test]
fn unicode_paths_are_inventoried() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("项目-α");
    fs::create_dir_all(root.join("docs")).unwrap();
    write(&root.join("docs").join("说明.md"), "# notes\n");

    let report = run_preflight(
        &[root.to_string_lossy().to_string()],
        IntakeLimits::default(),
    )
    .unwrap();
    assert!(report.inventory.file_count >= 1);
    assert!(report.blockers.is_empty());
}

#[test]
fn secret_fixture_never_enters_serialized_events() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("secret-risk");
    fs::create_dir_all(&root).unwrap();
    write(
        &root.join("config.env"),
        &format!("AWS_ACCESS_KEY_ID={FIXTURE_SECRET}\n{FIXTURE_ASSIGNMENT}\n"),
    );
    write(&root.join("notes.md"), "plain notes\n");

    let report = run_preflight(
        &[root.to_string_lossy().to_string()],
        IntakeLimits::default(),
    )
    .unwrap();
    assert!(!report.secret_risks.is_empty());
    let json = serde_json::to_string(&report).unwrap();
    assert_no_secret_leak(&json, &[FIXTURE_SECRET, "fixture-secret-value-do-not-leak"]).unwrap();
    assert!(report
        .warnings
        .iter()
        .any(|w| w.to_lowercase().contains("secret")));
}

#[test]
fn over_limit_inputs_fail_safely() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("too-many");
    fs::create_dir_all(&root).unwrap();
    for i in 0..6 {
        write(&root.join(format!("f{i}.txt")), "hello\n");
    }
    let report = run_preflight(
        &[root.to_string_lossy().to_string()],
        IntakeLimits::for_tests_small(),
    )
    .unwrap();
    assert!(!report.blockers.is_empty());
    assert!(!report.can_start);
}

#[test]
fn symlink_or_junction_escape_is_skipped() {
    let dir = tempdir().unwrap();
    let approved = dir.path().join("approved");
    let outside = dir.path().join("outside");
    fs::create_dir_all(&approved).unwrap();
    fs::create_dir_all(&outside).unwrap();
    write(&outside.join("secret.txt"), "should-not-inventory\n");
    write(&approved.join("ok.txt"), "ok\n");

    let link = approved.join("escape-link");
    #[cfg(windows)]
    {
        let ok = Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &link.to_string_lossy(),
                &outside.to_string_lossy(),
            ])
            .status()
            .expect("mklink");
        if !ok.success() {
            // Fallback to std symlink file if junctions require elevation unexpectedly.
            std::os::windows::fs::symlink_dir(&outside, &link).expect("symlink_dir");
        }
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(&outside, &link).unwrap();
    }

    let report = run_preflight(
        &[approved.to_string_lossy().to_string()],
        IntakeLimits::default(),
    )
    .unwrap();
    assert!(
        !report.escape_attempts.is_empty()
            || report
                .warnings
                .iter()
                .any(|w| w.to_lowercase().contains("escape"))
    );
    let serialized = serde_json::to_string(&report).unwrap();
    // Outside file content must not be treated as an inventory file entry path under outside root.
    assert!(!serialized.contains("should-not-inventory") || !report.escape_attempts.is_empty());
    let has_outside_file = report
        .manifest
        .projects
        .iter()
        .any(|p| p.root.contains("outside"));
    // The approved root project should remain; escaped content skipped.
    assert!(!has_outside_file || !report.escape_attempts.is_empty());
}

#[test]
fn single_file_intake_works() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("brainstorm.md");
    write(&file, "# idea\n");
    let report = run_preflight(
        &[file.to_string_lossy().to_string()],
        IntakeLimits::default(),
    )
    .unwrap();
    assert_eq!(report.manifest.sources.len(), 1);
    assert_eq!(
        report.manifest.sources[0].kind,
        tiamat_contracts::SourceKind::File
    );
    assert!(!report.can_start);
}

#[test]
fn ignore_rules_skip_node_modules() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(root.join("node_modules").join("pkg")).unwrap();
    write(&root.join("index.js"), "module.exports=1\n");
    write(
        &root.join("node_modules").join("pkg").join("index.js"),
        "SECRET\n",
    );
    let report = run_preflight(
        &[root.to_string_lossy().to_string()],
        IntakeLimits::default(),
    )
    .unwrap();
    assert_eq!(report.inventory.file_count, 1);
    assert!(report.inventory.ignored_count >= 1);
}
