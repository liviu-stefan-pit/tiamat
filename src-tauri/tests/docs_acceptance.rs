//! P12 documentation / release-prep acceptance checks.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    tiamat_contracts::repo_root()
}

#[test]
fn docs_manifest_and_guides_exist() {
    let root = repo_root();
    let manifest_path = root.join("docs/config/docs-manifest.json");
    let raw = fs::read_to_string(&manifest_path).expect("docs-manifest.json");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("manifest json");
    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["version"], "0.1.0");
    assert_eq!(value["signing"], "unsigned-dev");
    let guides = value["guides"].as_array().expect("guides");
    assert!(
        guides.len() >= 20,
        "expected full user+contributor guide set"
    );
    for guide in guides {
        let path = guide["path"].as_str().expect("path");
        assert!(root.join(path).is_file(), "missing documented guide {path}");
    }
}

#[test]
fn release_prep_artifacts_exist() {
    let root = repo_root();
    for rel in [
        "LICENSE",
        "CHANGELOG.md",
        "docs/release/CHECKLIST.md",
        "docs/release/SIGNING.md",
        "docs/release/PACKAGE-HASHES.md",
        "docs/release/KNOWN-LIMITATIONS.md",
        "docs/release/reports/DEPENDENCY-LICENSES.md",
        "docs/release/reports/VULNERABILITY-REPORT.md",
        "docs/user/first-run.md",
        "docs/contributor/architecture.md",
    ] {
        assert!(
            root.join(rel).is_file(),
            "missing release-prep artifact {rel}"
        );
    }
}

#[test]
fn package_hashes_match_p13_handoff_and_manifest() {
    let root = repo_root();
    let nsis = "1a3b92779c381bf7d9bfa2d544c04255a28dda085f19b96328bd66768146648b";
    let msi = "04299e8145f1750f49fef6c3a555cf47d783f5d6bf4dc2efa949f85c58f25b57";
    let hashes = fs::read_to_string(root.join("docs/release/PACKAGE-HASHES.md")).unwrap();
    assert!(hashes.contains(nsis));
    assert!(hashes.contains(msi));
    let manifest = fs::read_to_string(root.join("docs/config/docs-manifest.json")).unwrap();
    assert!(manifest.contains(nsis));
    assert!(manifest.contains(msi));
    let handoff = fs::read_to_string(root.join("P13-RELEASE-HANDOFF.md")).unwrap();
    assert!(handoff.contains(nsis));
    assert!(handoff.contains(msi));
    // Historical P11 candidate remains append-only evidence of the pre-P13 build.
    let candidate = fs::read_to_string(root.join("P11-RELEASE-CANDIDATE.md")).unwrap();
    assert!(candidate.contains("216bb9a8da1ca025e19f8d3ef19060a83e335f0427d404d56059d370c74d0ee7"));
}
