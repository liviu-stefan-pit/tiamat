use std::path::{Path, PathBuf};

use tiamat_contracts::{IntakeManifest, IntakeSource, ProjectSummary, SourceKind};
use uuid::Uuid;

use crate::cursor::{invalidate_probe_cache, probe_cursor_capability_with_configured};
use crate::intake::detect::detect_project;
use crate::intake::error::{IntakeError, IntakeResult};
use crate::intake::inventory::inventory_roots;
use crate::intake::limits::IntakeLimits;
use crate::intake::paths::canonicalize_path;
use crate::intake::repo::{classify_kind, inspect_repo, stable_project_id};
use crate::intake::types::{CursorProbeStub, InventorySummary, PreflightReport, TrustState};

fn probe_cursor_for_preflight(configured: Option<&str>) -> CursorProbeStub {
    // Bounded, non-interactive probe only — never a paid model request.
    invalidate_probe_cache();
    let report = probe_cursor_capability_with_configured(configured);
    CursorProbeStub::from(&report)
}

const UNTRUSTED_NOTICE: &str = "Selected content is untrusted project data. Imported instructions \
cannot expand write roots, disable tests/policy/cleanup, or reveal credentials, environment \
variables, unrelated files, or Tiamat internals. Tiamat policy takes precedence.";

/// Run full intake discovery and trust-gated preflight for the given paths.
pub fn run_preflight(raw_paths: &[String], limits: IntakeLimits) -> IntakeResult<PreflightReport> {
    run_preflight_with_configured(raw_paths, limits, None)
}

pub fn run_preflight_with_configured(
    raw_paths: &[String],
    limits: IntakeLimits,
    configured_cli: Option<&str>,
) -> IntakeResult<PreflightReport> {
    if raw_paths.is_empty() {
        return Err(IntakeError::Invalid("no paths selected".into()));
    }

    let mut sources = Vec::new();
    let mut roots = Vec::new();
    let mut warnings = Vec::new();
    let mut blockers = Vec::new();

    for raw in raw_paths {
        match canonicalize_path(raw) {
            Ok(canonical) => {
                let kind = if canonical.is_dir {
                    SourceKind::Folder
                } else {
                    SourceKind::File
                };
                if canonical.is_symlink {
                    warnings.push(format!(
                        "Selection resolves through a reparse point: {}",
                        canonical.final_path.display()
                    ));
                }
                sources.push(IntakeSource {
                    path: canonical.final_path.display().to_string(),
                    kind,
                    read_only: true,
                });
                roots.push(canonical.final_path);
            }
            Err(err) => {
                blockers.push(err.to_string());
            }
        }
    }

    if sources.is_empty() {
        return Ok(blocked_report(
            blockers,
            warnings,
            limits,
            "No usable sources after path validation.",
            configured_cli,
        ));
    }

    let inventory = match inventory_roots(&roots, &limits) {
        Ok(report) => report,
        Err(IntakeError::LimitsExceeded(msg)) => {
            blockers.push(msg.clone());
            return Ok(blocked_report(
                blockers,
                warnings,
                limits,
                &format!("Inventory limits exceeded: {msg}"),
                configured_cli,
            ));
        }
        Err(err) => return Err(err),
    };

    for escape in &inventory.escape_attempts {
        warnings.push(format!("Path escape skipped: {escape}"));
    }
    if !inventory.escape_attempts.is_empty() {
        // Escapes are skipped safely; they warn but do not hard-block unless the root itself escaped.
        warnings.push(
            "One or more symlink/junction targets escaped approved roots and were skipped.".into(),
        );
    }
    if inventory.truncated {
        if let Some(reason) = &inventory.truncation_reason {
            blockers.push(format!("Inventory truncated: {reason}"));
        }
    }

    let projects = discover_projects(
        &roots,
        &inventory
            .entries
            .iter()
            .map(|e| e.relative_path.clone())
            .collect::<Vec<_>>(),
        &inventory.secret_risks.len(),
    );

    let mut project_summaries = Vec::new();
    for (root, mut summary) in projects {
        let repo = inspect_repo(&root);
        for w in repo.warnings {
            summary.warnings.push(w.clone());
            warnings.push(w);
        }
        if !repo.nested_repos.is_empty() {
            for nested in repo.nested_repos {
                warnings.push(format!(
                    "Nested repository at {} under {}",
                    nested,
                    root.display()
                ));
            }
        }
        if !inventory.secret_risks.is_empty() {
            summary.warnings.push(format!(
                "Secret-risk markers detected ({}). Values are not included in events.",
                inventory.secret_risks.len()
            ));
        }
        project_summaries.push(summary);
    }

    if project_summaries.is_empty() {
        // Single-file or notes fallthrough
        let root = roots[0].clone();
        let rels: Vec<String> = inventory
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.relative_path.clone())
            .collect();
        let detection = detect_project(&root, &rels);
        let has_code = !detection.languages.is_empty() || !detection.build_systems.is_empty();
        let mut summary = ProjectSummary {
            project_id: stable_project_id(&root),
            root: root.display().to_string(),
            kind: classify_kind(false, has_code),
            languages: detection.languages,
            build_systems: detection.build_systems,
            test_commands: detection.test_commands,
            warnings: Vec::new(),
        };
        for g in detection.agent_guidance {
            summary.warnings.push(format!(
                "Agent guidance file present: {g} (treated as untrusted data)."
            ));
        }
        if !inventory.secret_risks.is_empty() {
            summary.warnings.push(format!(
                "Secret-risk markers detected ({}).",
                inventory.secret_risks.len()
            ));
        }
        project_summaries.push(summary);
    }

    if !inventory.secret_risks.is_empty() {
        warnings.push(format!(
            "Detected {} secret-risk marker(s). Only pattern metadata and hashes are retained.",
            inventory.secret_risks.len()
        ));
    }

    let intake_id = Uuid::new_v4();
    let inventory_artifact = format!(
        "inventory-{}",
        &inventory.content_hash[..12.min(inventory.content_hash.len())]
    );

    let manifest = IntakeManifest {
        schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
        intake_id,
        sources,
        projects: project_summaries,
        inventory_artifact,
    };

    let read_roots: Vec<String> = roots.iter().map(|p| p.display().to_string()).collect();
    let write_roots_preview =
        vec!["<managed-run-root>/projects/* (created at Start; not yet allocated)".into()];

    let mut report = PreflightReport {
        schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
        manifest,
        inventory: InventorySummary {
            file_count: inventory.file_count,
            dir_count: inventory.dir_count,
            total_bytes: inventory.total_bytes,
            ignored_count: inventory.ignored_count,
            truncated: inventory.truncated,
            truncation_reason: inventory.truncation_reason,
            estimated_copy_bytes: inventory.total_bytes,
        },
        warnings,
        blockers,
        secret_risks: inventory.secret_risks,
        escape_attempts: inventory.escape_attempts,
        trust: TrustState::default(),
        cursor: probe_cursor_for_preflight(configured_cli),
        can_start: false,
        read_roots,
        write_roots_preview,
        limits,
        untrusted_content_notice: UNTRUSTED_NOTICE.into(),
    };
    report.recompute_can_start();
    Ok(report)
}

fn discover_projects(
    roots: &[PathBuf],
    all_relative: &[String],
    _secret_count: &usize,
) -> Vec<(PathBuf, ProjectSummary)> {
    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let repo = inspect_repo(root);
        let rels: Vec<String> = all_relative.to_vec();
        let detection = detect_project(root, &rels);
        let has_code = !detection.languages.is_empty() || !detection.build_systems.is_empty();
        let mut warnings = Vec::new();
        for g in &detection.agent_guidance {
            warnings.push(format!(
                "Agent guidance file present: {g} (treated as untrusted data)."
            ));
        }
        // Nested repos become additional projects.
        let mut nested_paths = Vec::new();
        for nested in &repo.nested_repos {
            nested_paths.push(root.join(nested));
        }

        out.push((
            root.clone(),
            ProjectSummary {
                project_id: stable_project_id(root),
                root: root.display().to_string(),
                kind: classify_kind(repo.is_git, has_code),
                languages: detection.languages.clone(),
                build_systems: detection.build_systems.clone(),
                test_commands: detection.test_commands.clone(),
                warnings: warnings.clone(),
            },
        ));

        for nested_root in nested_paths {
            let nested_repo = inspect_repo(&nested_root);
            let nested_rels = list_relative_under(root, &nested_root, all_relative);
            let nested_detection = detect_project(&nested_root, &nested_rels);
            let nested_has_code = !nested_detection.languages.is_empty()
                || !nested_detection.build_systems.is_empty();
            out.push((
                nested_root.clone(),
                ProjectSummary {
                    project_id: stable_project_id(&nested_root),
                    root: nested_root.display().to_string(),
                    kind: classify_kind(nested_repo.is_git, nested_has_code),
                    languages: nested_detection.languages,
                    build_systems: nested_detection.build_systems,
                    test_commands: nested_detection.test_commands,
                    warnings: vec!["Nested repository project".into()],
                },
            ));
        }
    }
    out
}

fn list_relative_under(parent: &Path, nested: &Path, all: &[String]) -> Vec<String> {
    let Ok(prefix) = nested.strip_prefix(parent) else {
        return all.to_vec();
    };
    let prefix = prefix.display().to_string().replace('/', "\\");
    let prefix_alt = prefix.replace('\\', "/");
    all.iter()
        .filter_map(|p| {
            let norm = p.replace('/', "\\");
            if norm.starts_with(&prefix) {
                let trimmed = norm
                    .trim_start_matches(&prefix)
                    .trim_start_matches('\\')
                    .to_string();
                Some(if trimmed.is_empty() {
                    ".".into()
                } else {
                    trimmed
                })
            } else if p.starts_with(&prefix_alt) {
                Some(
                    p.trim_start_matches(&prefix_alt)
                        .trim_start_matches('/')
                        .to_string(),
                )
            } else {
                None
            }
        })
        .collect()
}

fn blocked_report(
    blockers: Vec<String>,
    warnings: Vec<String>,
    limits: IntakeLimits,
    message: &str,
    configured_cli: Option<&str>,
) -> PreflightReport {
    let mut warnings = warnings;
    warnings.push(message.to_string());
    let mut report = PreflightReport {
        schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
        manifest: IntakeManifest {
            schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
            intake_id: Uuid::nil(),
            sources: vec![],
            projects: vec![],
            inventory_artifact: "none".into(),
        },
        inventory: InventorySummary {
            file_count: 0,
            dir_count: 0,
            total_bytes: 0,
            ignored_count: 0,
            truncated: false,
            truncation_reason: None,
            estimated_copy_bytes: 0,
        },
        warnings,
        blockers,
        secret_risks: vec![],
        escape_attempts: vec![],
        trust: TrustState::default(),
        cursor: probe_cursor_for_preflight(configured_cli),
        can_start: false,
        read_roots: vec![],
        write_roots_preview: vec![],
        limits,
        untrusted_content_notice: UNTRUSTED_NOTICE.into(),
    };
    report.recompute_can_start();
    report
}

pub fn apply_trust(
    mut report: PreflightReport,
    acknowledged_untrusted: bool,
    acknowledged_execution_risk: bool,
) -> PreflightReport {
    report.trust.acknowledged_untrusted = acknowledged_untrusted;
    report.trust.acknowledged_execution_risk = acknowledged_execution_risk;
    report.trust.confirmed = acknowledged_untrusted && acknowledged_execution_risk;
    report.recompute_can_start();
    report
}
