//! Systematic fault injection at side-effect boundaries (test/dev only).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::recovery::error::{RecoveryError, RecoveryResult};
use crate::recovery::types::SideEffectKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultPoint {
    BeforePlanWrite,
    AfterPlanWrite,
    BeforeDbCommit,
    AfterDbCommit,
    BeforeProcessSpawn,
    AfterProcessSpawn,
    BeforeProcessExit,
    BeforeTestLaunch,
    AfterTestLaunch,
    BeforeGitCheckpoint,
    AfterGitCheckpoint,
}

impl FaultPoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BeforePlanWrite => "before_plan_write",
            Self::AfterPlanWrite => "after_plan_write",
            Self::BeforeDbCommit => "before_db_commit",
            Self::AfterDbCommit => "after_db_commit",
            Self::BeforeProcessSpawn => "before_process_spawn",
            Self::AfterProcessSpawn => "after_process_spawn",
            Self::BeforeProcessExit => "before_process_exit",
            Self::BeforeTestLaunch => "before_test_launch",
            Self::AfterTestLaunch => "after_test_launch",
            Self::BeforeGitCheckpoint => "before_git_checkpoint",
            Self::AfterGitCheckpoint => "after_git_checkpoint",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "before_plan_write" => Self::BeforePlanWrite,
            "after_plan_write" => Self::AfterPlanWrite,
            "before_db_commit" => Self::BeforeDbCommit,
            "after_db_commit" => Self::AfterDbCommit,
            "before_process_spawn" => Self::BeforeProcessSpawn,
            "after_process_spawn" => Self::AfterProcessSpawn,
            "before_process_exit" => Self::BeforeProcessExit,
            "before_test_launch" => Self::BeforeTestLaunch,
            "after_test_launch" => Self::AfterTestLaunch,
            "before_git_checkpoint" => Self::BeforeGitCheckpoint,
            "after_git_checkpoint" => Self::AfterGitCheckpoint,
            _ => return None,
        })
    }

    pub fn for_kind_before(kind: SideEffectKind) -> Self {
        match kind {
            SideEffectKind::PlanWrite => Self::BeforePlanWrite,
            SideEffectKind::DbCommit => Self::BeforeDbCommit,
            SideEffectKind::ProcessSpawn => Self::BeforeProcessSpawn,
            SideEffectKind::ProcessExit => Self::BeforeProcessExit,
            SideEffectKind::TestLaunch => Self::BeforeTestLaunch,
            SideEffectKind::GitCheckpoint => Self::BeforeGitCheckpoint,
        }
    }

    pub fn for_kind_after(kind: SideEffectKind) -> Option<Self> {
        Some(match kind {
            SideEffectKind::PlanWrite => Self::AfterPlanWrite,
            SideEffectKind::DbCommit => Self::AfterDbCommit,
            SideEffectKind::ProcessSpawn => Self::AfterProcessSpawn,
            SideEffectKind::ProcessExit => return None,
            SideEffectKind::TestLaunch => Self::AfterTestLaunch,
            SideEffectKind::GitCheckpoint => Self::AfterGitCheckpoint,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultAction {
    /// Return an error simulating a crash mid-operation.
    Crash,
    /// Skip performing the side effect (leave ledger non-terminal).
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FaultRule {
    pub point: FaultPoint,
    pub action: FaultAction,
    /// When true, the fault fires once then clears.
    pub once: bool,
}

#[derive(Debug, Default)]
struct FaultState {
    rules: HashMap<FaultPoint, FaultRule>,
    hit_counts: HashMap<FaultPoint, u32>,
}

fn global_faults() -> &'static Mutex<FaultState> {
    static FAULTS: OnceLock<Mutex<FaultState>> = OnceLock::new();
    FAULTS.get_or_init(|| Mutex::new(FaultState::default()))
}

pub fn clear_faults() {
    if let Ok(mut state) = global_faults().lock() {
        state.rules.clear();
        state.hit_counts.clear();
    }
}

pub fn set_fault(rule: FaultRule) {
    if let Ok(mut state) = global_faults().lock() {
        state.rules.insert(rule.point, rule);
    }
}

pub fn set_faults(rules: Vec<FaultRule>) {
    clear_faults();
    for rule in rules {
        set_fault(rule);
    }
}

pub fn list_faults() -> Vec<FaultRule> {
    global_faults()
        .lock()
        .map(|s| s.rules.values().cloned().collect())
        .unwrap_or_default()
}

pub fn hit_count(point: FaultPoint) -> u32 {
    global_faults()
        .lock()
        .map(|s| *s.hit_counts.get(&point).unwrap_or(&0))
        .unwrap_or(0)
}

/// Check and consume a fault at `point`. Returns Ok(None) if no fault,
/// Ok(Some(Skip)) if the caller should skip the side effect,
/// Err(FaultInjected) for Crash.
pub fn check_fault(point: FaultPoint) -> RecoveryResult<Option<FaultAction>> {
    let mut state = global_faults()
        .lock()
        .map_err(|_| RecoveryError::Validation("fault injector lock poisoned".into()))?;
    let Some(rule) = state.rules.get(&point).cloned() else {
        return Ok(None);
    };
    *state.hit_counts.entry(point).or_insert(0) += 1;
    if rule.once {
        state.rules.remove(&point);
    }
    match rule.action {
        FaultAction::Crash => Err(RecoveryError::FaultInjected(format!(
            "fault injected at {}",
            point.as_str()
        ))),
        FaultAction::Skip => Ok(Some(FaultAction::Skip)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_fault_fires_once_then_clears() {
        clear_faults();
        set_fault(FaultRule {
            point: FaultPoint::BeforePlanWrite,
            action: FaultAction::Crash,
            once: true,
        });
        assert!(check_fault(FaultPoint::BeforePlanWrite).is_err());
        assert!(check_fault(FaultPoint::BeforePlanWrite).unwrap().is_none());
        assert_eq!(hit_count(FaultPoint::BeforePlanWrite), 1);
        clear_faults();
    }

    #[test]
    fn skip_fault_returns_skip() {
        clear_faults();
        set_fault(FaultRule {
            point: FaultPoint::BeforeGitCheckpoint,
            action: FaultAction::Skip,
            once: false,
        });
        assert_eq!(
            check_fault(FaultPoint::BeforeGitCheckpoint).unwrap(),
            Some(FaultAction::Skip)
        );
        assert_eq!(
            check_fault(FaultPoint::BeforeGitCheckpoint).unwrap(),
            Some(FaultAction::Skip)
        );
        clear_faults();
    }
}
