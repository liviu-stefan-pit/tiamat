use std::collections::{HashMap, HashSet, VecDeque};

use tiamat_contracts::{compile_schema, schema_path, validate_json_str, PhasePlan, ProjectPlan};
use uuid::Uuid;

use crate::planner::types::PlanValidationIssue;
use crate::workspace::RunWorkspaceManifest;

pub fn validate_plan_json(
    json_text: &str,
    expected_run_id: Uuid,
    workspace: &RunWorkspaceManifest,
) -> Result<ProjectPlan, Vec<PlanValidationIssue>> {
    let mut issues = Vec::new();

    let schema_file = schema_path("project-plan.schema.json");
    match compile_schema(&schema_file).and_then(|schema| validate_json_str(&schema, json_text)) {
        Ok(_) => {}
        Err(err) => {
            issues.push(PlanValidationIssue {
                code: "schema".into(),
                message: err.to_string(),
                phase_id: None,
            });
            return Err(issues);
        }
    }

    let plan: ProjectPlan = match serde_json::from_str(json_text) {
        Ok(plan) => plan,
        Err(err) => {
            issues.push(PlanValidationIssue {
                code: "deserialize".into(),
                message: err.to_string(),
                phase_id: None,
            });
            return Err(issues);
        }
    };

    if let Err(err) = plan.validate_schema_version() {
        issues.push(PlanValidationIssue {
            code: "schema_version".into(),
            message: err.to_string(),
            phase_id: None,
        });
    }

    if plan.run_id != expected_run_id {
        issues.push(PlanValidationIssue {
            code: "run_id_mismatch".into(),
            message: format!(
                "plan.runId {} does not match run {}",
                plan.run_id, expected_run_id
            ),
            phase_id: None,
        });
    }

    if plan.title.trim().is_empty() {
        issues.push(PlanValidationIssue {
            code: "empty_title".into(),
            message: "title must be nonempty".into(),
            phase_id: None,
        });
    }

    if plan.phases.is_empty() {
        issues.push(PlanValidationIssue {
            code: "no_phases".into(),
            message: "plan must include at least one phase".into(),
            phase_id: None,
        });
    }

    let project_ids: HashSet<String> = workspace
        .projects
        .iter()
        .map(|p| p.project_id.clone())
        .collect();
    let mut phase_ids = HashSet::new();

    for phase in &plan.phases {
        if !phase_ids.insert(phase.phase_id.clone()) {
            issues.push(PlanValidationIssue {
                code: "duplicate_phase_id".into(),
                message: format!("duplicate phaseId {}", phase.phase_id),
                phase_id: Some(phase.phase_id.clone()),
            });
        }
        issues.extend(validate_phase(phase, workspace, &project_ids));
    }

    issues.extend(validate_dag(&plan));
    issues.extend(validate_final_gates(&plan));

    if issues.is_empty() {
        Ok(plan)
    } else {
        Err(issues)
    }
}

fn validate_phase(
    phase: &PhasePlan,
    workspace: &RunWorkspaceManifest,
    project_ids: &HashSet<String>,
) -> Vec<PlanValidationIssue> {
    let mut issues = Vec::new();
    let pid = Some(phase.phase_id.clone());

    if phase.title.trim().is_empty() || phase.objective.trim().is_empty() {
        issues.push(PlanValidationIssue {
            code: "phase_incomplete".into(),
            message: "title and objective must be nonempty".into(),
            phase_id: pid.clone(),
        });
    }

    if phase.acceptance_criteria.is_empty() {
        issues.push(PlanValidationIssue {
            code: "missing_acceptance".into(),
            message: "phase requires at least one acceptance criterion".into(),
            phase_id: pid.clone(),
        });
    } else {
        for ac in &phase.acceptance_criteria {
            if ac.description.trim().is_empty() || ac.criterion_id.trim().is_empty() {
                issues.push(PlanValidationIssue {
                    code: "vague_acceptance".into(),
                    message: "acceptance criteria must have stable ids and descriptions".into(),
                    phase_id: pid.clone(),
                });
            }
        }
    }

    if phase.prompt.trim().is_empty() {
        issues.push(PlanValidationIssue {
            code: "empty_prompt".into(),
            message: "phase prompt must be nonempty".into(),
            phase_id: pid.clone(),
        });
    } else {
        let lower = phase.prompt.to_ascii_lowercase();
        if !lower.contains(".tiamat/master-plan.md") || !lower.contains(".tiamat/plan.json") {
            issues.push(PlanValidationIssue {
                code: "prompt_missing_plan_refs".into(),
                message:
                    "phase prompt must require reading .tiamat/MASTER-PLAN.md and .tiamat/plan.json"
                        .into(),
                phase_id: pid.clone(),
            });
        }
        if lower.contains("todo?") || lower.contains("placeholder") {
            issues.push(PlanValidationIssue {
                code: "prompt_placeholder".into(),
                message: "phase prompt must not leave TODO/placeholder instructions".into(),
                phase_id: pid.clone(),
            });
        }
    }

    if phase.project_ids.is_empty() {
        issues.push(PlanValidationIssue {
            code: "missing_project_ids".into(),
            message: "phase requires projectIds".into(),
            phase_id: pid.clone(),
        });
    }
    for project_id in &phase.project_ids {
        if !project_ids.is_empty() && !project_ids.contains(project_id) {
            // Notes-only runs may use notes project ids present only in intake.
            // Allow if it matches a notes root leaf name or managed project.
            let notes_ok = workspace
                .notes_roots
                .iter()
                .any(|n| n.replace('\\', "/").ends_with(project_id));
            if !notes_ok {
                issues.push(PlanValidationIssue {
                    code: "unknown_project_id".into(),
                    message: format!("projectId '{project_id}' is not in the managed workspace"),
                    phase_id: pid.clone(),
                });
            }
        }
    }

    if phase.write_roots.is_empty() {
        issues.push(PlanValidationIssue {
            code: "missing_write_roots".into(),
            message: "phase requires exclusive writeRoots".into(),
            phase_id: pid.clone(),
        });
    }
    for root in &phase.write_roots {
        if let Err(err) = validate_root_against_workspace(workspace, root, true) {
            issues.push(PlanValidationIssue {
                code: "invalid_write_root".into(),
                message: err,
                phase_id: pid.clone(),
            });
        }
    }
    for root in &phase.read_roots {
        if let Err(err) = validate_root_against_workspace(workspace, root, false) {
            issues.push(PlanValidationIssue {
                code: "invalid_read_root".into(),
                message: err,
                phase_id: pid.clone(),
            });
        }
    }

    let ac_ids: HashSet<_> = phase
        .acceptance_criteria
        .iter()
        .map(|a| a.criterion_id.clone())
        .collect();
    let all_tests = phase
        .unit_tests
        .iter()
        .chain(phase.integration_tests.iter())
        .chain(phase.e2e_tests.iter());
    let mut any_test = false;
    for test in all_tests {
        any_test = true;
        if test.command.is_empty()
            && test
                .inapplicable_reason
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        {
            issues.push(PlanValidationIssue {
                code: "invalid_test".into(),
                message: format!(
                    "test {} needs a command or nonempty inapplicableReason",
                    test.test_id
                ),
                phase_id: pid.clone(),
            });
        }
        for cover in &test.covers {
            if !ac_ids.contains(cover) {
                issues.push(PlanValidationIssue {
                    code: "test_covers_unknown".into(),
                    message: format!("test {} covers unknown criterion {cover}", test.test_id),
                    phase_id: pid.clone(),
                });
            }
        }
    }

    let empty_layers = [
        ("unit", phase.unit_tests.is_empty()),
        ("integration", phase.integration_tests.is_empty()),
        ("e2e", phase.e2e_tests.is_empty()),
    ];
    for (layer, empty) in empty_layers {
        if !empty {
            continue;
        }
        let blob = format!(
            "{} {} {}",
            phase.objective, phase.prompt, phase.rollback.checkpoint
        )
        .to_ascii_lowercase();
        let explained = blob.contains(&format!("{layer} inapplicable"))
            || blob.contains(&format!("{layer} tests inapplicable"))
            || blob.contains("tests inapplicable")
            || blob.contains("inapplicable");
        if !any_test && !explained {
            issues.push(PlanValidationIssue {
                code: "empty_tests_unexplained".into(),
                message: format!(
                    "{layer} tests empty without inapplicability explanation or nearest evidence"
                ),
                phase_id: pid.clone(),
            });
        } else if any_test && !explained {
            issues.push(PlanValidationIssue {
                code: "empty_layer_unexplained".into(),
                message: format!("{layer} tests empty; state why that layer is inapplicable"),
                phase_id: pid.clone(),
            });
        }
    }

    issues
}

fn validate_root_against_workspace(
    workspace: &RunWorkspaceManifest,
    root: &str,
    write: bool,
) -> Result<(), String> {
    let trimmed = root.trim();
    if trimmed.is_empty() {
        return Err("root path is empty".into());
    }
    // Allow managed-relative markers used in fixtures ("." / project id).
    if trimmed == "." {
        if workspace.projects.len() == 1 || !workspace.notes_roots.is_empty() {
            return Ok(());
        }
        return Err("'.' write/read root is only valid for single-project or notes runs".into());
    }

    if write {
        workspace.validate_write_root(trimmed).or_else(|_| {
            // Also accept managed_root equality / project-relative leaf names.
            if workspace.projects.iter().any(|p| {
                p.project_id == trimmed || p.write_root == trimmed || p.managed_root == trimmed
            }) {
                Ok(())
            } else if workspace.notes_roots.iter().any(|n| n == trimmed) {
                Err("notes roots are read-only; cannot be writeRoots".into())
            } else {
                Err(format!("write root not approved: {trimmed}"))
            }
        })
    } else {
        workspace.validate_read_root(trimmed).or_else(|_| {
            if workspace
                .projects
                .iter()
                .any(|p| p.project_id == trimmed || p.managed_root == trimmed)
                || workspace.notes_roots.iter().any(|n| n == trimmed)
                || trimmed == workspace.managed_run_root
                || trimmed == workspace.control_root
            {
                Ok(())
            } else {
                Err(format!("read root not approved: {trimmed}"))
            }
        })
    }
}

fn validate_dag(plan: &ProjectPlan) -> Vec<PlanValidationIssue> {
    let mut issues = Vec::new();
    let ids: HashSet<_> = plan.phases.iter().map(|p| p.phase_id.clone()).collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut indegree: HashMap<String, usize> = HashMap::new();

    for phase in &plan.phases {
        indegree.entry(phase.phase_id.clone()).or_insert(0);
        for dep in &phase.dependencies {
            if !ids.contains(dep) {
                issues.push(PlanValidationIssue {
                    code: "unknown_dependency".into(),
                    message: format!("phase {} depends on unknown {dep}", phase.phase_id),
                    phase_id: Some(phase.phase_id.clone()),
                });
                continue;
            }
            adj.entry(dep.clone())
                .or_default()
                .push(phase.phase_id.clone());
            *indegree.entry(phase.phase_id.clone()).or_insert(0) += 1;
            indegree.entry(dep.clone()).or_insert(0);
        }
    }

    let mut queue: VecDeque<String> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut seen = 0usize;
    while let Some(node) = queue.pop_front() {
        seen += 1;
        if let Some(next) = adj.get(&node) {
            for child in next {
                if let Some(deg) = indegree.get_mut(child) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(child.clone());
                    }
                }
            }
        }
    }
    if seen != plan.phases.len() && !plan.phases.is_empty() {
        issues.push(PlanValidationIssue {
            code: "cyclic_dependencies".into(),
            message: "phase dependency graph contains a cycle".into(),
            phase_id: None,
        });
    }
    issues
}

fn validate_final_gates(plan: &ProjectPlan) -> Vec<PlanValidationIssue> {
    let mut issues = Vec::new();
    let ids: HashSet<_> = plan.phases.iter().map(|p| p.phase_id.clone()).collect();
    let mut gate_ids = HashSet::new();
    for gate in &plan.final_gates {
        if !gate_ids.insert(gate.gate_id.clone()) {
            issues.push(PlanValidationIssue {
                code: "duplicate_gate_id".into(),
                message: format!("duplicate gateId {}", gate.gate_id),
                phase_id: None,
            });
        }
        if gate.description.trim().is_empty() {
            issues.push(PlanValidationIssue {
                code: "empty_gate".into(),
                message: format!("gate {} needs a description", gate.gate_id),
                phase_id: None,
            });
        }
        for dep in &gate.dependencies {
            if !ids.contains(dep) {
                issues.push(PlanValidationIssue {
                    code: "gate_unknown_dependency".into(),
                    message: format!("gate {} depends on unknown phase {dep}", gate.gate_id),
                    phase_id: None,
                });
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{
        ManagedProject, ManagedProjectKind, PromotionMetadata, PromotionStatus, RetentionPolicy,
        SourceFingerprint,
    };

    fn workspace_single(write_root: &str) -> RunWorkspaceManifest {
        RunWorkspaceManifest {
            schema_version: 1,
            run_id: Uuid::parse_str("d4e5f6a7-b8c9-4012-d345-6789abcdef01").unwrap(),
            intake_id: Uuid::nil(),
            managed_run_root: r"C:\managed\run".into(),
            control_root: r"C:\managed\run\control".into(),
            projects: vec![ManagedProject {
                project_id: "notes-app".into(),
                source_root: r"C:\src\notes".into(),
                managed_root: write_root.into(),
                kind: ManagedProjectKind::NotesSnapshot,
                baseline_commit: Some("abc".into()),
                baseline_branch: "main".into(),
                worktree_path: None,
                write_root: write_root.into(),
                read_roots: vec![r"C:\managed\run".into()],
                dirty_overlay: None,
                source_fingerprint: SourceFingerprint {
                    path: r"C:\src\notes".into(),
                    kind: "notes".into(),
                    head: None,
                    branch: None,
                    status_porcelain: String::new(),
                    status_hash: "0".into(),
                    tree_hash: "0".into(),
                    captured_at_utc: chrono::Utc::now(),
                },
                lock_name: "write:notes-app".into(),
            }],
            notes_roots: vec![],
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
            created_at_utc: chrono::Utc::now(),
            source_unchanged: true,
        }
    }

    fn valid_plan_json(write_root: &str) -> String {
        format!(
            r#"{{
  "schemaVersion": 1,
  "runId": "d4e5f6a7-b8c9-4012-d345-6789abcdef01",
  "title": "Notes tool",
  "summary": "Build a small notes tool from rough specs",
  "assumptions": ["Desktop-first"],
  "risks": ["Ambiguous scope"],
  "phases": [{{
    "phaseId": "P01",
    "title": "Core slice",
    "objective": "Ship a minimal notes list. Integration tests inapplicable for notes-only MVP; e2e tests inapplicable until UI shell exists.",
    "dependencies": [],
    "projectIds": ["notes-app"],
    "readRoots": ["{write_root}"],
    "writeRoots": ["{write_root}"],
    "modelTier": "composer",
    "estimatedMinutes": 10,
    "acceptanceCriteria": [{{
      "criterionId": "AC-P01-01",
      "description": "Notes list renders from fixture data",
      "requiredEvidenceKinds": ["unit"]
    }}],
    "unitTests": [{{
      "testId": "UT-P01-01",
      "command": ["npm", "test"],
      "workingDirectory": ".",
      "timeoutSeconds": 120,
      "resourceLocks": [],
      "expected": {{"exitCode": 0, "artifacts": []}},
      "covers": ["AC-P01-01"]
    }}],
    "integrationTests": [],
    "e2eTests": [],
    "manualChecks": [],
    "rollback": {{"checkpoint": "intake-baseline", "strategy": "restore"}},
    "expectedArtifacts": ["src/notes.ts"],
    "prompt": "Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json. Inspect git status and prior evidence. Implement only P01. Add/run unit tests. Return a schema-valid phase-result payload.",
    "status": "draft",
    "evidence": []
  }}],
  "finalGates": [{{
    "gateId": "FG-01",
    "description": "Independent review",
    "dependencies": ["P01"],
    "requiredEvidenceKinds": ["review"]
  }}]
}}"#,
            write_root = write_root.replace('\\', "\\\\")
        )
    }

    #[test]
    fn accepts_valid_plan() {
        let root = r"C:\managed\run\projects\notes-app";
        let ws = workspace_single(root);
        let plan = validate_plan_json(
            &valid_plan_json(root),
            Uuid::parse_str("d4e5f6a7-b8c9-4012-d345-6789abcdef01").unwrap(),
            &ws,
        )
        .expect("valid");
        assert_eq!(plan.phases.len(), 1);
    }

    #[test]
    fn rejects_cycle() {
        let root = r"C:\managed\run\projects\notes-app";
        let ws = workspace_single(root);
        let mut json = valid_plan_json(root);
        json = json.replace(r#""dependencies": []"#, r#""dependencies": ["P01"]"#);
        let err = validate_plan_json(
            &json,
            Uuid::parse_str("d4e5f6a7-b8c9-4012-d345-6789abcdef01").unwrap(),
            &ws,
        )
        .unwrap_err();
        assert!(err.iter().any(|i| i.code == "cyclic_dependencies"));
    }

    #[test]
    fn rejects_invalid_write_root() {
        let root = r"C:\managed\run\projects\notes-app";
        let ws = workspace_single(root);
        let json = valid_plan_json(r"C:\source\escape");
        let err = validate_plan_json(
            &json,
            Uuid::parse_str("d4e5f6a7-b8c9-4012-d345-6789abcdef01").unwrap(),
            &ws,
        )
        .unwrap_err();
        assert!(err.iter().any(|i| i.code == "invalid_write_root"));
    }
}
