//! Fake no-op orchestrator for P00 shell bootstrap.

#[derive(Debug, Clone)]
pub struct OrchestratorStatus {
    pub mode: String,
    pub active_runs: u32,
    pub message: String,
}

pub struct FakeOrchestrator;

impl FakeOrchestrator {
    pub const MODE: &'static str = "fake-no-op";

    pub fn status() -> OrchestratorStatus {
        OrchestratorStatus {
            mode: Self::MODE.to_string(),
            active_runs: 0,
            message: "Orchestration is disabled until scheduler phases land.".to_string(),
        }
    }

    pub fn start_run() -> Result<(), &'static str> {
        Err("orchestrator is fake/no-op in P00")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_orchestrator_reports_no_active_runs() {
        let status = FakeOrchestrator::status();
        assert_eq!(status.mode, "fake-no-op");
        assert_eq!(status.active_runs, 0);
    }

    #[test]
    fn fake_orchestrator_rejects_start() {
        assert_eq!(
            FakeOrchestrator::start_run(),
            Err("orchestrator is fake/no-op in P00")
        );
    }
}
