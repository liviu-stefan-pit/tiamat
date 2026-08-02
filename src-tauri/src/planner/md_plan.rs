//! Extract architect MASTER-PLAN.md and compile structured Markdown → ProjectPlan.

use tiamat_contracts::{
    AcceptanceCriterion, FinalGate, ManualCheck, ModelTier, PhasePlan, PhaseStatus, ProjectPlan,
    RollbackSpec, RollbackStrategy, TestExpected, TestKind, TestSpec,
};
use uuid::Uuid;

/// Extract the markdown body from architect assistant text.
/// Prefers fenced ```markdown / ```md; otherwise accepts raw MD with phase markers.
/// Never treats stream control / session frames as the plan (caller should pass
/// assembled assistant text only).
pub fn extract_master_plan_markdown(assistant_text: &str) -> Result<String, String> {
    let text = assistant_text.trim();
    if text.is_empty() {
        return Err(
            "architect stream had no assistant markdown (ignored session/control frames)".into(),
        );
    }

    if let Some(body) = extract_fenced_markdown(text) {
        let body = body.trim().to_string();
        ensure_phase_markers(&body)?;
        return Ok(body);
    }

    // Whole-assistant markdown (common in plan mode).
    if looks_like_structured_master_plan(text) {
        return Ok(text.to_string());
    }

    // Last-resort: any ``` fence whose body looks like a master plan.
    if let Some(body) = extract_any_markdownish_fence(text) {
        ensure_phase_markers(&body)?;
        return Ok(body);
    }

    Err(
        "architect assistant text missing required MASTER-PLAN.md phase markers \
         (`## Phase:`). Do not emit ProjectPlan JSON; return structured Markdown."
            .into(),
    )
}

fn extract_fenced_markdown(text: &str) -> Option<String> {
    for lang in ["markdown", "md"] {
        let open = format!("```{lang}");
        if let Some(start) = find_ci(text, &open) {
            let after = start + open.len();
            let rest = &text[after..];
            let rest = rest.strip_prefix('\r').unwrap_or(rest);
            let rest = rest.strip_prefix('\n').unwrap_or(rest);
            if let Some(end) = rest.find("```") {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

fn extract_any_markdownish_fence(text: &str) -> Option<String> {
    let mut search = text;
    while let Some(idx) = search.find("```") {
        let after_ticks = &search[idx + 3..];
        // Skip language tag on same line.
        let body_start = if let Some(nl) = after_ticks.find('\n') {
            let meta = after_ticks[..nl].trim().to_ascii_lowercase();
            if meta == "json" || meta == "ts" || meta == "js" || meta == "rust" {
                search = &after_ticks[nl + 1..];
                continue;
            }
            nl + 1
        } else {
            0
        };
        let body_and_rest = &after_ticks[body_start..];
        if let Some(end) = body_and_rest.find("```") {
            let body = body_and_rest[..end].trim();
            if looks_like_structured_master_plan(body) {
                return Some(body.to_string());
            }
            search = &body_and_rest[end + 3..];
        } else {
            break;
        }
    }
    None
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

fn looks_like_structured_master_plan(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("## phase:") || lower.contains("## phase ")
}

fn ensure_phase_markers(body: &str) -> Result<(), String> {
    if looks_like_structured_master_plan(body) {
        Ok(())
    } else {
        Err(
            "extracted markdown missing required phase markers (`## Phase: <id> — <title>`)"
                .into(),
        )
    }
}

/// Compile structured MASTER-PLAN.md into a ProjectPlan. `run_id` is always taken
/// from the active run (Markdown must not invent a different runId).
pub fn compile_master_plan_markdown(
    markdown: &str,
    run_id: Uuid,
) -> Result<ProjectPlan, Vec<String>> {
    let mut issues = Vec::new();
    let md = markdown.trim();
    if md.is_empty() {
        return Err(vec!["MASTER-PLAN.md is empty".into()]);
    }

    let title = parse_title(md).unwrap_or_else(|| {
        issues.push("missing top-level `# Title`".into());
        String::new()
    });
    let summary = section_body(md, "Summary").unwrap_or_default();
    if summary.trim().is_empty() {
        issues.push("missing ## Summary".into());
    }
    let assumptions = bullet_list(section_body(md, "Assumptions").as_deref().unwrap_or(""));
    let risks = bullet_list(section_body(md, "Risks").as_deref().unwrap_or(""));

    let phase_blocks = split_phase_blocks(md);
    if phase_blocks.is_empty() {
        issues.push(
            "no `## Phase:` sections found; structured phase outline is required".into(),
        );
    }

    let mut phases = Vec::new();
    for block in &phase_blocks {
        match compile_phase(block) {
            Ok(phase) => phases.push(phase),
            Err(errs) => issues.extend(errs),
        }
    }

    let final_gates = match compile_final_gates(md) {
        Ok(gates) => gates,
        Err(errs) => {
            issues.extend(errs);
            vec![]
        }
    };

    if !issues.is_empty() {
        return Err(issues);
    }

    Ok(ProjectPlan {
        schema_version: 1,
        run_id,
        title,
        summary: summary.trim().to_string(),
        assumptions,
        risks,
        phases,
        final_gates,
    })
}

fn parse_title(md: &str) -> Option<String> {
    for line in md.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("# ") {
            if !rest.starts_with('#') {
                return Some(rest.trim().to_string());
            }
        }
    }
    None
}

fn section_body(md: &str, heading: &str) -> Option<String> {
    let needle = format!("## {heading}");
    let lower = md.to_ascii_lowercase();
    let needle_l = needle.to_ascii_lowercase();
    let start = lower.find(&needle_l)?;
    let after_heading = &md[start..];
    let after_nl = after_heading.find('\n').map(|i| i + 1).unwrap_or(after_heading.len());
    let body_start = start + after_nl;
    let rest = &md[body_start..];
    let end = rest
        .find("\n## ")
        .or_else(|| rest.find("\n# "))
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn bullet_list(body: &str) -> Vec<String> {
    body.lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with("- ") || l.starts_with("* "))
        .map(|l| l[2..].trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn split_phase_blocks(md: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let lower = lines[i].trim().to_ascii_lowercase();
        if lower.starts_with("## phase:") || lower.starts_with("## phase ") {
            let start = i;
            let mut end = start + 1;
            while end < lines.len() {
                let l = lines[end].trim().to_ascii_lowercase();
                if l.starts_with("## phase:")
                    || l.starts_with("## phase ")
                    || l.starts_with("## final gate")
                {
                    break;
                }
                end += 1;
            }
            blocks.push(lines[start..end].join("\n"));
            i = end;
            continue;
        }
        i += 1;
    }
    blocks
}

fn compile_phase(block: &str) -> Result<PhasePlan, Vec<String>> {
    let mut issues = Vec::new();
    let header = block.lines().next().unwrap_or("").trim();
    let (hdr_id, hdr_title) = parse_phase_header(header);

    let phase_id = field(block, "phaseId")
        .or(hdr_id)
        .unwrap_or_else(|| {
            issues.push("phase missing phaseId".into());
            String::new()
        });
    let title = field(block, "title").or(hdr_title).unwrap_or_else(|| {
        // Prefer header title; else empty.
        String::new()
    });
    if title.trim().is_empty() {
        issues.push(format!("phase {phase_id}: missing title"));
    }

    let objective = field(block, "objective").unwrap_or_default();
    if objective.trim().is_empty() {
        issues.push(format!("phase {phase_id}: missing objective"));
    }

    let dependencies = list_field(block, "dependencies");
    let project_ids = list_field(block, "projectIds");
    let read_roots = path_list_field(block, "readRoots");
    let write_roots = path_list_field(block, "writeRoots");
    let model_tier = parse_model_tier(
        field(block, "modelTier")
            .as_deref()
            .unwrap_or("composer"),
    )
    .unwrap_or_else(|e| {
        issues.push(format!("phase {phase_id}: {e}"));
        ModelTier::Composer
    });
    let estimated_minutes = field(block, "estimatedMinutes")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(30);

    let rollback_checkpoint = field(block, "rollbackCheckpoint")
        .or_else(|| field(block, "rollback"))
        .unwrap_or_else(|| "intake-baseline".into());
    let rollback_strategy = parse_rollback_strategy(
        field(block, "rollbackStrategy")
            .as_deref()
            .unwrap_or("restore"),
    )
    .unwrap_or(RollbackStrategy::Restore);

    let expected_artifacts = list_field(block, "expectedArtifacts");

    let acceptance_criteria = parse_acceptance_criteria(block, &phase_id, &mut issues);
    let unit_tests = parse_tests(block, "Unit tests", &phase_id, &mut issues);
    let integration_tests = parse_tests(block, "Integration tests", &phase_id, &mut issues);
    let e2e_tests = {
        let mut tmp = Vec::new();
        for heading in ["E2E tests", "E2e tests", "End-to-end tests"] {
            tmp = parse_tests(block, heading, &phase_id, &mut issues);
            if !tmp.is_empty() || subsection(block, heading).is_some() {
                break;
            }
        }
        tmp
    };
    let manual_checks = parse_manual_checks(block);

    if !issues.is_empty() {
        return Err(issues);
    }

    let mut phase = PhasePlan {
        phase_id: phase_id.clone(),
        title,
        objective: objective.clone(),
        dependencies,
        project_ids,
        read_roots,
        write_roots,
        model_tier,
        estimated_minutes,
        acceptance_criteria,
        unit_tests,
        integration_tests,
        e2e_tests,
        manual_checks,
        rollback: RollbackSpec {
            checkpoint: rollback_checkpoint,
            strategy: rollback_strategy,
        },
        expected_artifacts,
        prompt: String::new(),
        status: PhaseStatus::Draft,
        evidence: vec![],
    };
    phase.prompt = synthesize_phase_prompt(&phase);
    Ok(phase)
}

fn parse_phase_header(header: &str) -> (Option<String>, Option<String>) {
    // ## Phase: P01 — Title   or  ## Phase P01 — Title
    let trimmed = header.trim().trim_start_matches('#').trim();
    let lower = trimmed.to_ascii_lowercase();
    let rest = if let Some(r) = lower.strip_prefix("phase:") {
        let offset = trimmed.len() - r.len();
        &trimmed[offset..]
    } else if let Some(r) = lower.strip_prefix("phase ") {
        let offset = trimmed.len() - r.len();
        &trimmed[offset..]
    } else {
        return (None, None);
    };
    let rest = rest.trim();
    if let Some((id, title)) = rest.split_once("—") {
        (
            Some(id.trim().to_string()),
            Some(title.trim().to_string()),
        )
    } else if let Some((id, title)) = rest.split_once(" - ") {
        (
            Some(id.trim().to_string()),
            Some(title.trim().to_string()),
        )
    } else if !rest.is_empty() {
        (Some(rest.to_string()), None)
    } else {
        (None, None)
    }
}

fn field(block: &str, key: &str) -> Option<String> {
    let key_l = key.to_ascii_lowercase();
    for line in block.lines() {
        let t = line.trim();
        let t = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).unwrap_or(t);
        // **phaseId**: value  |  **phaseId:** value  | phaseId: value
        let cleaned = t.replace("**", "");
        let cleaned = cleaned.trim();
        if let Some((k, v)) = cleaned.split_once(':') {
            if k.trim().eq_ignore_ascii_case(&key_l) {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn list_field(block: &str, key: &str) -> Vec<String> {
    let raw = match field(block, key) {
        Some(v) => v,
        None => return vec![],
    };
    let lower = raw.to_ascii_lowercase();
    if lower == "none" || lower == "(none)" || lower == "n/a" {
        return vec![];
    }
    raw.split(|c| c == ',' || c == '|')
        .map(|s| s.trim().trim_matches('`').to_string())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
        .collect()
}

fn path_list_field(block: &str, key: &str) -> Vec<String> {
    list_field(block, key)
}

fn parse_model_tier(raw: &str) -> Result<ModelTier, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "composer" => Ok(ModelTier::Composer),
        "grok-low" | "groklow" | "low" => Ok(ModelTier::GrokLow),
        "grok-medium" | "grokmedium" | "medium" => Ok(ModelTier::GrokMedium),
        "grok-high" | "grokhigh" | "high" => Ok(ModelTier::GrokHigh),
        other => Err(format!("unknown modelTier '{other}'")),
    }
}

fn parse_rollback_strategy(raw: &str) -> Result<RollbackStrategy, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "restore" => Ok(RollbackStrategy::Restore),
        "quarantine" => Ok(RollbackStrategy::Quarantine),
        other => Err(format!("unknown rollbackStrategy '{other}'")),
    }
}

fn subsection(block: &str, heading: &str) -> Option<String> {
    let needle = format!("### {heading}");
    let lower = block.to_ascii_lowercase();
    let needle_l = needle.to_ascii_lowercase();
    let start = lower.find(&needle_l)?;
    let after = &block[start..];
    let after_nl = after.find('\n').map(|i| i + 1).unwrap_or(after.len());
    let body_start = start + after_nl;
    let rest = &block[body_start..];
    let end = rest
        .find("\n### ")
        .or_else(|| rest.find("\n## "))
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn parse_acceptance_criteria(
    block: &str,
    phase_id: &str,
    issues: &mut Vec<String>,
) -> Vec<AcceptanceCriterion> {
    let body = subsection(block, "Acceptance criteria")
        .or_else(|| subsection(block, "Acceptance Criteria"))
        .unwrap_or_default();
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if !(t.starts_with("- ") || t.starts_with("* ")) {
            continue;
        }
        let t = t[2..].trim();
        if t.eq_ignore_ascii_case("(none)") || t.eq_ignore_ascii_case("none") {
            continue;
        }
        // `AC-..` — description — evidence: unit, integration
        let (id, rest) = split_backtick_id(t);
        let parts: Vec<&str> = rest.split("—").map(|s| s.trim()).collect();
        let description = parts.first().copied().unwrap_or("").trim().to_string();
        let evidence_raw = parts
            .iter()
            .find(|p| p.to_ascii_lowercase().starts_with("evidence:"))
            .map(|p| p.split_once(':').map(|(_, v)| v.trim()).unwrap_or(""))
            .unwrap_or("unit");
        let kinds = parse_evidence_kinds(evidence_raw);
        let criterion_id = id.unwrap_or_else(|| format!("AC-{phase_id}-{:02}", out.len() + 1));
        if description.is_empty() {
            issues.push(format!(
                "phase {phase_id}: acceptance criterion {criterion_id} missing description"
            ));
        }
        out.push(AcceptanceCriterion {
            criterion_id,
            description,
            required_evidence_kinds: kinds,
        });
    }
    out
}

fn parse_tests(
    block: &str,
    heading: &str,
    phase_id: &str,
    issues: &mut Vec<String>,
) -> Vec<TestSpec> {
    let body = subsection(block, heading).unwrap_or_default();
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if !(t.starts_with("- ") || t.starts_with("* ")) {
            continue;
        }
        let t = t[2..].trim();
        // (none) — reason: ...
        if t.to_ascii_lowercase().starts_with("(none)")
            || t.to_ascii_lowercase().starts_with("none ")
            || t.eq_ignore_ascii_case("none")
        {
            continue;
        }
        let (id, rest) = split_backtick_id(t);
        let test_id = id.unwrap_or_else(|| format!("T-{phase_id}-{:02}", out.len() + 1));

        // command: `a` `b` — cwd: `.` — timeout: 120 — covers: AC-1
        let command = extract_command_tokens(rest);
        let cwd = extract_labeled(rest, "cwd").unwrap_or_else(|| ".".into());
        let timeout = extract_labeled(rest, "timeout")
            .and_then(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
            .unwrap_or(120);
        let covers = extract_labeled(rest, "covers")
            .map(|s| {
                s.split(|c| c == ',' || c == ' ')
                    .map(|x| x.trim().trim_matches('`').to_string())
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let inapplicable = extract_labeled(rest, "reason").or_else(|| {
            if rest.to_ascii_lowercase().contains("inapplicable") {
                Some(rest.to_string())
            } else {
                None
            }
        });

        if command.is_empty() && inapplicable.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true)
        {
            issues.push(format!(
                "phase {phase_id}: test {test_id} needs command: `…` or reason:"
            ));
        }

        out.push(TestSpec {
            test_id,
            command,
            working_directory: cwd,
            timeout_seconds: timeout,
            resource_locks: vec![],
            expected: TestExpected {
                exit_code: 0,
                artifacts: vec![],
            },
            covers,
            inapplicable_reason: inapplicable.filter(|s| !s.trim().is_empty()),
        });
    }
    out
}

fn parse_manual_checks(block: &str) -> Vec<ManualCheck> {
    let body = subsection(block, "Manual checks")
        .or_else(|| subsection(block, "Manual Checks"))
        .unwrap_or_default();
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if !(t.starts_with("- ") || t.starts_with("* ")) {
            continue;
        }
        let t = t[2..].trim();
        if t.eq_ignore_ascii_case("(none)") || t.eq_ignore_ascii_case("none") {
            continue;
        }
        let blocking = t.to_ascii_lowercase().contains("blocking: true");
        let description = t
            .split("—")
            .next()
            .unwrap_or(t)
            .trim()
            .to_string();
        out.push(ManualCheck {
            description,
            blocking,
        });
    }
    out
}

fn compile_final_gates(md: &str) -> Result<Vec<FinalGate>, Vec<String>> {
    let body = section_body(md, "Final gates")
        .or_else(|| section_body(md, "Final Gates"))
        .unwrap_or_default();
    let mut issues = Vec::new();
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if !(t.starts_with("- ") || t.starts_with("* ")) {
            continue;
        }
        let t = t[2..].trim();
        if t.eq_ignore_ascii_case("(none)") || t.eq_ignore_ascii_case("none") {
            continue;
        }
        let (id, rest) = split_backtick_id(t);
        let gate_id = id.unwrap_or_else(|| format!("FG-{:02}", out.len() + 1));
        let parts: Vec<&str> = rest.split("—").map(|s| s.trim()).collect();
        let description = parts
            .iter()
            .find(|p| {
                let l = p.to_ascii_lowercase();
                !l.starts_with("deps:") && !l.starts_with("evidence:")
            })
            .copied()
            .unwrap_or("")
            .to_string();
        let deps = parts
            .iter()
            .find(|p| p.to_ascii_lowercase().starts_with("deps:"))
            .map(|p| {
                p.split_once(':')
                    .map(|(_, v)| v)
                    .unwrap_or("")
                    .split(|c| c == ',' || c == ' ')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let evidence = parts
            .iter()
            .find(|p| p.to_ascii_lowercase().starts_with("evidence:"))
            .map(|p| p.split_once(':').map(|(_, v)| v.trim()).unwrap_or("review"))
            .unwrap_or("review");
        if description.is_empty() {
            issues.push(format!("final gate {gate_id}: missing description"));
        }
        out.push(FinalGate {
            gate_id,
            description,
            dependencies: deps,
            required_evidence_kinds: parse_evidence_kinds(evidence),
        });
    }
    if !issues.is_empty() {
        Err(issues)
    } else {
        Ok(out)
    }
}

fn split_backtick_id(text: &str) -> (Option<String>, &str) {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('`') {
        if let Some(end) = rest.find('`') {
            let id = rest[..end].trim().to_string();
            let after = rest[end + 1..].trim();
            let after = after
                .strip_prefix('—')
                .or_else(|| after.strip_prefix('-'))
                .unwrap_or(after)
                .trim();
            return (Some(id), after);
        }
    }
    (None, text)
}

fn extract_labeled(text: &str, label: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let needle = format!("{}:", label.to_ascii_lowercase());
    let idx = lower.find(&needle)?;
    let after = &text[idx + needle.len()..];
    // Take until next em-dash separator.
    let chunk = after.split("—").next().unwrap_or(after).trim();
    Some(chunk.trim().to_string())
}

fn extract_command_tokens(text: &str) -> Vec<String> {
    let Some(cmd_part) = extract_labeled(text, "command") else {
        return vec![];
    };
    // Prefer backtick tokens: `npm` `test`
    let mut tokens = Vec::new();
    let mut rest = cmd_part.as_str();
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('`') {
            tokens.push(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    if !tokens.is_empty() {
        return tokens;
    }
    // Fallback: whitespace-split
    cmd_part
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_evidence_kinds(raw: &str) -> Vec<TestKind> {
    raw.split(|c| c == ',' || c == '|' || c == ' ')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .filter_map(|s| match s.as_str() {
            "unit" => Some(TestKind::Unit),
            "integration" => Some(TestKind::Integration),
            "e2e" | "end-to-end" => Some(TestKind::E2e),
            "manual" => Some(TestKind::Manual),
            "diff" => Some(TestKind::Diff),
            "review" => Some(TestKind::Review),
            "artifact" => Some(TestKind::Artifact),
            "cleanup" => Some(TestKind::Cleanup),
            _ => None,
        })
        .collect()
}

/// Template used so the architect does not spend tokens on copy-paste prompts.
pub fn synthesize_phase_prompt(phase: &PhasePlan) -> String {
    format!(
        "Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json. Inspect git status and prior \
         evidence. Implement only {id} ({title}). Objective: {objective}. Preserve unrelated \
         work. Add/run appropriate unit, integration, and E2E tests as specified for this \
         phase. Return a schema-valid immutable phase-result payload. The orchestrator alone \
         updates SQLite and both plan artifacts transactionally. Do not declare success \
         without command output and artifacts.",
        id = phase.phase_id,
        title = phase.title,
        objective = phase.objective.trim(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiamat_contracts::compile_schema;
    use tiamat_contracts::schema_path;
    use tiamat_contracts::validate_json_str;

    const SAMPLE_MD: &str = r#"# Rough-spec notes tool

## Summary
Turn brainstorm notes into a small testable notes list app.

## Assumptions
- Desktop-first MVP
- No cloud sync in v1

## Risks
- Ambiguous scope in brainstorm notes

## Phase: P01 — Notes list vertical slice

Deep design notes go here and are preserved in MASTER-PLAN.md.

- **phaseId**: P01
- **objective**: Render a notes list from fixture data. Integration tests inapplicable for notes-only MVP shell; e2e tests inapplicable until a UI host exists.
- **dependencies**: none
- **projectIds**: notes-app
- **readRoots**: /managed/notes-app
- **writeRoots**: /managed/notes-app
- **modelTier**: composer
- **estimatedMinutes**: 10
- **rollbackCheckpoint**: intake-baseline
- **rollbackStrategy**: restore
- **expectedArtifacts**: src/notes.ts

### Acceptance criteria
- `AC-P01-01` — Notes list unit test passes against fixture data — evidence: unit

### Unit tests
- `UT-P01-01` — command: `npm` `test` — cwd: `.` — timeout: 120 — covers: AC-P01-01

### Integration tests
- (none) — reason: notes-only MVP

### E2E tests
- (none) — reason: no UI host yet

### Manual checks
- (none)

## Final gates
- `FG-01` — Independent architecture review — deps: P01 — evidence: review
"#;

    #[test]
    fn extracts_fenced_markdown_and_ignores_noise() {
        let assistant = format!(
            "Thinking...\n\n```markdown\n{SAMPLE_MD}\n```\n\nDone."
        );
        let md = extract_master_plan_markdown(&assistant).unwrap();
        assert!(md.contains("## Phase: P01"));
        assert!(md.starts_with("# Rough-spec"));
    }

    #[test]
    fn stream_fixture_with_session_noise_still_extracts_md() {
        // Simulates assembled assistant_text after stream parse (control frames already ignored).
        let assistant = format!(
            "I will inspect notes.\n\n```md\n{SAMPLE_MD}\n```"
        );
        let md = extract_master_plan_markdown(&assistant).unwrap();
        let plan = compile_master_plan_markdown(&md, Uuid::nil()).unwrap();
        assert_eq!(plan.phases.len(), 1);
        assert_eq!(plan.phases[0].phase_id, "P01");
        assert!(plan.phases[0].prompt.contains(".tiamat/MASTER-PLAN.md"));
        assert!(plan.phases[0].prompt.contains(".tiamat/plan.json"));
    }

    #[test]
    fn compile_sample_validates_against_schema() {
        let run_id = Uuid::parse_str("d4e5f6a7-b8c9-4012-d345-6789abcdef01").unwrap();
        let plan = compile_master_plan_markdown(SAMPLE_MD, run_id).unwrap();
        assert_eq!(plan.run_id, run_id);
        assert_eq!(plan.title, "Rough-spec notes tool");
        assert_eq!(plan.phases[0].unit_tests[0].command, vec!["npm", "test"]);
        assert_eq!(plan.final_gates[0].gate_id, "FG-01");

        let json = serde_json::to_string_pretty(&plan).unwrap();
        let schema = compile_schema(&schema_path("project-plan.schema.json")).unwrap();
        validate_json_str(&schema, &json).expect("compiled plan must schema-validate");
    }

    #[test]
    fn fixture_sample_md_compiles() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fixtures/planner/sample-master-plan.md");
        let md = std::fs::read_to_string(path).unwrap();
        let run_id = Uuid::parse_str("d4e5f6a7-b8c9-4012-d345-6789abcdef01").unwrap();
        let plan = compile_master_plan_markdown(&md, run_id).unwrap();
        assert_eq!(plan.phases[0].phase_id, "P01");
        assert!(plan.phases[0].prompt.contains("Implement only P01"));
    }

    #[test]
    fn stream_ndjson_fixture_extracts_markdown() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fixtures/planner/stream-architect-md.ndjson");
        let ndjson = std::fs::read_to_string(path).unwrap();
        let parsed = crate::cursor::parse_stream_json(&ndjson, "", &[]);
        assert!(parsed.chat_id.as_deref() == Some("chat-fixture"));
        // Control frames must not become the plan body.
        assert!(!parsed.assistant_text.contains("session_id"));
        let md = extract_master_plan_markdown(&parsed.assistant_text).unwrap();
        assert!(md.contains("## Phase: P01"));
        let plan = compile_master_plan_markdown(&md, Uuid::nil()).unwrap();
        assert_eq!(plan.title, "Fixture plan");
    }
}
