use std::collections::{HashMap, HashSet, VecDeque};

use tiamat_contracts::ProjectPlan;

use crate::scheduler::error::{SchedulerError, SchedulerResult};
use crate::scheduler::types::{PhaseRecord, PhaseRuntimeStatus};

/// Validate DAG acyclicity and dependency references before a scheduling epoch.
pub fn validate_dag(plan: &ProjectPlan) -> SchedulerResult<()> {
    let ids: HashSet<&str> = plan.phases.iter().map(|p| p.phase_id.as_str()).collect();
    for phase in &plan.phases {
        for dep in &phase.dependencies {
            if !ids.contains(dep.as_str()) {
                return Err(SchedulerError::Plan(format!(
                    "phase {} depends on unknown {}",
                    phase.phase_id, dep
                )));
            }
        }
    }
    if let Some(cycle) = find_cycle(plan) {
        return Err(SchedulerError::Plan(format!("DAG cycle detected: {cycle}")));
    }
    Ok(())
}

fn find_cycle(plan: &ProjectPlan) -> Option<String> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for phase in &plan.phases {
        adj.entry(phase.phase_id.as_str()).or_default();
        for dep in &phase.dependencies {
            adj.entry(dep.as_str())
                .or_default()
                .push(phase.phase_id.as_str());
        }
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut stack = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        adj: &HashMap<&'a str, Vec<&'a str>>,
        visiting: &mut HashSet<&'a str>,
        visited: &mut HashSet<&'a str>,
        stack: &mut Vec<&'a str>,
    ) -> Option<String> {
        if visited.contains(node) {
            return None;
        }
        if !visiting.insert(node) {
            let start = stack.iter().position(|n| *n == node).unwrap_or(0);
            let mut cycle: Vec<&str> = stack[start..].to_vec();
            cycle.push(node);
            return Some(cycle.join(" -> "));
        }
        stack.push(node);
        if let Some(nexts) = adj.get(node) {
            for next in nexts {
                if let Some(cycle) = dfs(next, adj, visiting, visited, stack) {
                    return Some(cycle);
                }
            }
        }
        stack.pop();
        visiting.remove(node);
        visited.insert(node);
        None
    }

    for phase in &plan.phases {
        if let Some(cycle) = dfs(
            phase.phase_id.as_str(),
            &adj,
            &mut visiting,
            &mut visited,
            &mut stack,
        ) {
            return Some(cycle);
        }
    }
    None
}

/// Longest path length from this phase to a leaf (critical-path fairness key).
pub fn compute_critical_path_lengths(phases: &[PhaseRecord]) -> HashMap<String, u32> {
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut indegree: HashMap<&str, u32> = HashMap::new();
    for phase in phases {
        indegree.entry(phase.phase_id.as_str()).or_insert(0);
        dependents.entry(phase.phase_id.as_str()).or_default();
        for dep in &phase.dependencies {
            dependents
                .entry(dep.as_str())
                .or_default()
                .push(phase.phase_id.as_str());
            *indegree.entry(phase.phase_id.as_str()).or_insert(0) += 1;
            indegree.entry(dep.as_str()).or_insert(0);
        }
    }

    // Bottom-up DP via reverse topo: longest remaining chain including self.
    let mut memo: HashMap<String, u32> = HashMap::new();
    let ids: Vec<String> = phases.iter().map(|p| p.phase_id.clone()).collect();

    fn longest(
        id: &str,
        dependents: &HashMap<&str, Vec<&str>>,
        memo: &mut HashMap<String, u32>,
    ) -> u32 {
        if let Some(v) = memo.get(id) {
            return *v;
        }
        let mut best = 1u32;
        if let Some(nexts) = dependents.get(id) {
            for next in nexts {
                best = best.max(1 + longest(next, dependents, memo));
            }
        }
        memo.insert(id.to_string(), best);
        best
    }

    for id in &ids {
        longest(id, &dependents, &mut memo);
    }
    // Silence unused indegree warning by consuming topo count.
    let _ = indegree;
    memo
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    Blocked { reason: String },
    Waiting,
    Terminal,
}

/// A phase is ready only when all dependencies passed (or were safely skipped).
pub fn evaluate_readiness(phase: &PhaseRecord, by_id: &HashMap<String, PhaseRecord>) -> Readiness {
    if phase.status.is_terminal() || phase.status == PhaseRuntimeStatus::NeedsReview {
        return Readiness::Terminal;
    }
    if phase.status.is_active() {
        return Readiness::Waiting;
    }

    let mut waiting = false;
    for dep_id in &phase.dependencies {
        let Some(dep) = by_id.get(dep_id) else {
            return Readiness::Blocked {
                reason: format!("missing dependency {dep_id}"),
            };
        };
        if dep.status.counts_as_success_dep() {
            continue;
        }
        if matches!(
            dep.status,
            PhaseRuntimeStatus::Failed | PhaseRuntimeStatus::Cancelled
        ) {
            return Readiness::Blocked {
                reason: format!("dependency {dep_id} is {}", dep.status.as_str()),
            };
        }
        waiting = true;
    }

    if waiting {
        Readiness::Waiting
    } else {
        Readiness::Ready
    }
}

/// Fairness: oldest ready first, then higher critical-path length.
pub fn sort_ready_phases(mut ready: Vec<PhaseRecord>) -> Vec<PhaseRecord> {
    ready.sort_by(|a, b| {
        let ready_cmp = a
            .ready_at_utc
            .as_deref()
            .unwrap_or("")
            .cmp(b.ready_at_utc.as_deref().unwrap_or(""));
        if ready_cmp != std::cmp::Ordering::Equal {
            return ready_cmp;
        }
        b.critical_path_length
            .cmp(&a.critical_path_length)
            .then_with(|| a.phase_id.cmp(&b.phase_id))
    });
    ready
}

/// Stable sorted lock acquisition order for write roots + named resources.
pub fn sorted_lock_names(write_roots: &[String], resource_locks: &[String]) -> Vec<String> {
    use crate::scheduler::types::{resource_lock_name, write_lock_name};
    let mut names: Vec<String> = write_roots.iter().map(|r| write_lock_name(r)).collect();
    names.extend(resource_locks.iter().map(|r| resource_lock_name(r)));
    names.sort();
    names.dedup();
    names
}

/// Collect resource locks declared on a phase's tests (from plan projection).
#[allow(dead_code)]
pub fn collect_plan_resource_locks(plan: &ProjectPlan, phase_id: &str) -> Vec<String> {
    let Some(phase) = plan.phases.iter().find(|p| p.phase_id == phase_id) else {
        return Vec::new();
    };
    let mut locks = HashSet::new();
    for test in phase
        .unit_tests
        .iter()
        .chain(phase.integration_tests.iter())
        .chain(phase.e2e_tests.iter())
    {
        for lock in &test.resource_locks {
            locks.insert(lock.clone());
        }
    }
    let mut out: Vec<String> = locks.into_iter().collect();
    out.sort();
    out
}

#[allow(dead_code)]
pub fn topo_phase_ids(phases: &[PhaseRecord]) -> Vec<String> {
    let mut indegree: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for phase in phases {
        indegree.entry(phase.phase_id.as_str()).or_insert(0);
        for dep in &phase.dependencies {
            adj.entry(dep.as_str())
                .or_default()
                .push(phase.phase_id.as_str());
            *indegree.entry(phase.phase_id.as_str()).or_insert(0) += 1;
            indegree.entry(dep.as_str()).or_insert(0);
        }
    }
    let mut queue: VecDeque<&str> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(k, _)| *k)
        .collect();
    let mut order = Vec::new();
    while let Some(id) = queue.pop_front() {
        order.push(id.to_string());
        if let Some(nexts) = adj.get(id) {
            for next in nexts {
                if let Some(d) = indegree.get_mut(next) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        queue.push_back(next);
                    }
                }
            }
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiamat_contracts::ModelTier;
    use uuid::Uuid;

    fn phase(id: &str, deps: &[&str], cpl: u32, ready: Option<&str>) -> PhaseRecord {
        PhaseRecord {
            run_id: Uuid::nil(),
            phase_id: id.into(),
            title: id.into(),
            status: PhaseRuntimeStatus::Draft,
            project_ids: vec!["p".into()],
            write_roots: vec![format!("C:\\w\\{id}")],
            resource_locks: vec![],
            dependencies: deps.iter().map(|s| (*s).to_string()).collect(),
            model_tier: ModelTier::Composer,
            estimated_minutes: 10,
            critical_path_length: cpl,
            ready_at_utc: ready.map(str::to_string),
            queued_at_utc: None,
            attempt_count: 0,
            last_failure_kind: None,
        }
    }

    #[test]
    fn readiness_requires_passed_deps() {
        let mut by_id = HashMap::new();
        let mut a = phase("A", &[], 1, None);
        a.status = PhaseRuntimeStatus::Passed;
        let b = phase("B", &["A"], 1, None);
        by_id.insert("A".into(), a);
        assert_eq!(evaluate_readiness(&b, &by_id), Readiness::Ready);

        by_id.get_mut("A").unwrap().status = PhaseRuntimeStatus::Failed;
        assert!(matches!(
            evaluate_readiness(&b, &by_id),
            Readiness::Blocked { .. }
        ));
    }

    #[test]
    fn fairness_oldest_then_critical_path() {
        let ready = vec![
            phase("B", &[], 1, Some("2026-08-02T10:00:02Z")),
            phase("A", &[], 5, Some("2026-08-02T10:00:01Z")),
            phase("C", &[], 9, Some("2026-08-02T10:00:01Z")),
        ];
        let sorted = sort_ready_phases(ready);
        assert_eq!(
            sorted
                .iter()
                .map(|p| p.phase_id.as_str())
                .collect::<Vec<_>>(),
            vec!["C", "A", "B"]
        );
    }

    #[test]
    fn lock_names_are_sorted_and_deduped() {
        let names = sorted_lock_names(
            &[
                "C:\\Repo\\B".into(),
                "C:\\Repo\\A".into(),
                "C:\\Repo\\A\\".into(),
            ],
            &["port:3000".into(), "browser".into()],
        );
        assert_eq!(
            names,
            vec![
                "resource:browser".to_string(),
                "resource:port:3000".to_string(),
                "write:c:\\repo\\a".to_string(),
                "write:c:\\repo\\b".to_string(),
            ]
        );
    }
}
