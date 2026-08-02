use std::path::Path;

use tiamat_contracts::TestKind;

use crate::verification::types::AdvisoryTestHint;

/// Advisory discovery only — architect-specified commands remain authoritative (MASTER-PLAN §15.2).
pub fn discover_advisory_tests(project_root: &Path) -> Vec<AdvisoryTestHint> {
    let mut hints = Vec::new();
    let package_json = project_root.join("package.json");
    if package_json.exists() {
        if let Ok(text) = std::fs::read_to_string(&package_json) {
            if text.contains("\"test\"") {
                hints.push(AdvisoryTestHint {
                    kind: TestKind::Unit,
                    command: vec!["npm".into(), "test".into()],
                    source: "package.json#scripts.test".into(),
                    note: "Advisory only; architect must specify exact expected commands.".into(),
                });
            }
            if text.contains("playwright") || text.contains("\"e2e\"") {
                hints.push(AdvisoryTestHint {
                    kind: TestKind::E2e,
                    command: vec!["npm".into(), "run".into(), "test:e2e".into()],
                    source: "package.json e2e hint".into(),
                    note: "Advisory only.".into(),
                });
            }
        }
    }
    if project_root.join("Cargo.toml").exists() {
        hints.push(AdvisoryTestHint {
            kind: TestKind::Unit,
            command: vec!["cargo".into(), "test".into()],
            source: "Cargo.toml".into(),
            note: "Advisory only.".into(),
        });
    }
    if project_root.join("tests").is_dir() || project_root.join("test").is_dir() {
        hints.push(AdvisoryTestHint {
            kind: TestKind::Integration,
            command: vec!["node".into(), "tests/integration.mjs".into()],
            source: "tests/ directory".into(),
            note: "Advisory only.".into(),
        });
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn discovers_npm_test_hint() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"test":"node test.mjs"}}"#,
        )
        .unwrap();
        let hints = discover_advisory_tests(dir.path());
        assert!(hints.iter().any(|h| h.kind == TestKind::Unit));
    }
}
