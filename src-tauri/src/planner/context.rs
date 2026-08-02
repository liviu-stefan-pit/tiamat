use serde_json::json;
use tiamat_contracts::IntakeManifest;

use crate::intake::PreflightReport;
use crate::workspace::RunWorkspaceManifest;

/// Soft cap for packaged intake context (characters).
pub const CONTEXT_CHAR_BUDGET: usize = 48_000;
const PER_FILE_CHAR_BUDGET: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedArchitectContext {
    pub text: String,
    pub omitted: Vec<String>,
    pub coverage: Vec<String>,
}

/// Package bounded intake + workspace facts for the architect prompt.
pub fn package_architect_context(
    preflight: &PreflightReport,
    workspace: &RunWorkspaceManifest,
) -> BoundedArchitectContext {
    let mut omitted = Vec::new();
    let mut coverage = Vec::new();
    let mut sections = Vec::new();

    sections.push(format!(
        "## Run\n- runId: {}\n- managedRunRoot: {}\n- controlRoot: {}\n",
        workspace.run_id, workspace.managed_run_root, workspace.control_root
    ));
    coverage.push("run-workspace".into());

    sections.push(format!(
        "## Intake inventory summary\n- files: {}\n- dirs: {}\n- bytes: {}\n- truncated: {}\n- estimatedCopyBytes: {}\n",
        preflight.inventory.file_count,
        preflight.inventory.dir_count,
        preflight.inventory.total_bytes,
        preflight.inventory.truncated,
        preflight.inventory.estimated_copy_bytes
    ));
    coverage.push("inventory-summary".into());

    sections.push(format!(
        "## Intake manifest (JSON)\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&bounded_manifest(&preflight.manifest))
            .unwrap_or_else(|_| "{}".into())
    ));
    coverage.push("intake-manifest".into());

    if !preflight.warnings.is_empty() {
        sections.push(format!(
            "## Warnings\n{}\n",
            preflight
                .warnings
                .iter()
                .map(|w| format!("- {w}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
        coverage.push("warnings".into());
    }

    if !preflight.secret_risks.is_empty() {
        sections.push(format!(
            "## Secret-risk markers (hashes only; values withheld)\n{}\n",
            preflight
                .secret_risks
                .iter()
                .map(|s| format!(
                    "- {} pattern={} hash={} bytes={}",
                    s.relative_path, s.pattern_id, s.match_hash, s.match_byte_len
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
        coverage.push("secret-risk-metadata".into());
    }

    sections.push("## Managed projects\n".into());
    for project in &workspace.projects {
        sections.push(format!(
            "- projectId={} kind={:?} managedRoot={} writeRoot={} baseline={:?}\n",
            project.project_id,
            project.kind,
            project.managed_root,
            project.write_root,
            project.baseline_commit
        ));
        coverage.push(format!("project:{}", project.project_id));
    }

    if !workspace.notes_roots.is_empty() {
        sections.push(format!(
            "## Notes roots (read-only)\n{}\n",
            workspace
                .notes_roots
                .iter()
                .map(|n| format!("- {n}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
        coverage.push("notes-roots".into());
    }

    // Sample a few small text files from notes/managed roots for context.
    for notes_root in &workspace.notes_roots {
        sample_directory(notes_root, &mut sections, &mut coverage, &mut omitted);
    }
    for project in &workspace.projects {
        sample_directory(
            &project.managed_root,
            &mut sections,
            &mut coverage,
            &mut omitted,
        );
    }

    sections.push(format!(
        "## Approved roots\n- readRoots: {:?}\n- writeRootsPreview: {:?}\n",
        preflight.read_roots, preflight.write_roots_preview
    ));

    sections.push(
        "## Output schema\nReturn a ProjectPlan JSON object with schemaVersion=1 matching \
         Tiamat's project-plan schema. Phases must use approved managed write/read roots. \
         Do not implement product code.\n"
            .into(),
    );

    let mut text = sections.join("\n");
    if text.len() > CONTEXT_CHAR_BUDGET {
        omitted.push(format!(
            "context truncated from {} to {} chars",
            text.len(),
            CONTEXT_CHAR_BUDGET
        ));
        text.truncate(CONTEXT_CHAR_BUDGET);
        text.push_str("\n\n[CONTEXT TRUNCATED]\n");
    }

    if !omitted.is_empty() {
        text.push_str("\n## Explicit omissions\n");
        for item in &omitted {
            text.push_str(&format!("- {item}\n"));
        }
    }

    BoundedArchitectContext {
        text,
        omitted,
        coverage,
    }
}

fn bounded_manifest(manifest: &IntakeManifest) -> serde_json::Value {
    json!({
        "schemaVersion": manifest.schema_version,
        "intakeId": manifest.intake_id,
        "sources": manifest.sources.iter().map(|s| json!({
            "path": s.path,
            "kind": s.kind,
            "readOnly": s.read_only,
        })).collect::<Vec<_>>(),
        "projects": manifest.projects.iter().map(|p| json!({
            "projectId": p.project_id,
            "root": p.root,
            "kind": p.kind,
            "languages": p.languages,
            "buildSystems": p.build_systems,
            "testCommands": p.test_commands,
            "warnings": p.warnings,
        })).collect::<Vec<_>>(),
        "inventoryArtifact": manifest.inventory_artifact,
    })
}

fn sample_directory(
    root: &str,
    sections: &mut Vec<String>,
    coverage: &mut Vec<String>,
    omitted: &mut Vec<String>,
) {
    let path = std::path::Path::new(root);
    if !path.exists() {
        omitted.push(format!("missing sample root: {root}"));
        return;
    }
    let walker = walkdir_limited(path, 3, 12);
    for file in walker {
        let rel = file
            .strip_prefix(path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| file.display().to_string());
        let Ok(bytes) = std::fs::read(&file) else {
            omitted.push(format!("unreadable: {rel}"));
            continue;
        };
        if bytes.len() > PER_FILE_CHAR_BUDGET {
            omitted.push(format!(
                "oversized sample omitted: {rel} ({} bytes)",
                bytes.len()
            ));
            continue;
        }
        if looks_binary(&bytes) {
            omitted.push(format!("binary omitted: {rel}"));
            continue;
        }
        let content = String::from_utf8_lossy(&bytes);
        sections.push(format!(
            "### File sample `{root}/{rel}`\n```\n{content}\n```\n"
        ));
        coverage.push(format!("file:{root}/{rel}"));
    }
}

fn walkdir_limited(
    root: &std::path::Path,
    max_depth: usize,
    max_files: usize,
) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if out.len() >= max_files {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if out.len() >= max_files {
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".git" || name == "node_modules" || name == "target" {
                continue;
            }
            if path.is_dir() {
                if depth < max_depth {
                    stack.push((path, depth + 1));
                }
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    out
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(512).any(|b| *b == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn packages_manifest_and_records_omissions_for_binary() {
        let dir = tempdir().unwrap();
        let notes = dir.path().join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::write(notes.join("NOTES.md"), "rough idea").unwrap();
        std::fs::write(notes.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();

        let manifest: IntakeManifest = serde_json::from_value(json!({
            "schemaVersion": 1,
            "intakeId": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
            "sources": [{"path": notes.display().to_string(), "kind": "folder", "readOnly": true}],
            "projects": [{
                "projectId": "notes",
                "root": notes.display().to_string(),
                "kind": "notes",
                "languages": [],
                "buildSystems": [],
                "testCommands": [],
                "warnings": []
            }],
            "inventoryArtifact": "inv"
        }))
        .unwrap();

        let preflight = PreflightReport {
            schema_version: 1,
            manifest,
            inventory: crate::intake::InventorySummary {
                file_count: 2,
                dir_count: 1,
                total_bytes: 20,
                ignored_count: 0,
                truncated: false,
                truncation_reason: None,
                estimated_copy_bytes: 20,
            },
            warnings: vec!["demo warning".into()],
            blockers: vec![],
            secret_risks: vec![],
            escape_attempts: vec![],
            trust: crate::intake::TrustState {
                confirmed: true,
                acknowledged_untrusted: true,
                acknowledged_execution_risk: true,
            },
            cursor: crate::intake::CursorProbeStub::default(),
            can_start: true,
            read_roots: vec![notes.display().to_string()],
            write_roots_preview: vec![],
            limits: crate::intake::IntakeLimits::default(),
            untrusted_content_notice: "untrusted".into(),
        };

        let workspace = RunWorkspaceManifest {
            schema_version: 1,
            run_id: uuid::Uuid::nil(),
            intake_id: uuid::Uuid::nil(),
            managed_run_root: dir.path().display().to_string(),
            control_root: dir.path().join("control").display().to_string(),
            projects: vec![],
            notes_roots: vec![notes.display().to_string()],
            checkpoints: vec![],
            quarantines: vec![],
            promotion: crate::workspace::PromotionMetadata {
                status: crate::workspace::PromotionStatus::Unpromoted,
                export_path: None,
                promoted_at_utc: None,
                notes: None,
            },
            retention: crate::workspace::RetentionPolicy::default(),
            fingerprint_pairs: vec![],
            created_at_utc: chrono::Utc::now(),
            source_unchanged: true,
        };

        let packaged = package_architect_context(&preflight, &workspace);
        assert!(packaged.text.contains("rough idea"));
        assert!(packaged.omitted.iter().any(|o| o.contains("binary")));
        assert!(packaged.coverage.iter().any(|c| c.contains("NOTES.md")));
        assert!(packaged.text.len() <= CONTEXT_CHAR_BUDGET + 200);
    }
}
