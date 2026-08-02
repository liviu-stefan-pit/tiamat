use serde::Serialize;
use tiamat_contracts::{compile_schema, schema_path, validate_json_str};

use crate::scheduler::FakeOrchestrator;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub schema_version: u32,
    pub orchestrator_mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractValidationResult {
    pub valid: bool,
    pub schema_name: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorStatus {
    pub mode: String,
    pub active_runs: u32,
    pub message: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Tiamat".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: tiamat_contracts::CURRENT_SCHEMA_VERSION,
        orchestrator_mode: FakeOrchestrator::MODE.to_string(),
    }
}

#[tauri::command]
pub fn validate_contract_json(schema_name: String, json_text: String) -> ContractValidationResult {
    let schema_file = match schema_name.as_str() {
        "intake-manifest" => schema_path("intake-manifest.schema.json"),
        "event-envelope" => schema_path("event-envelope.schema.json"),
        "project-plan" => schema_path("project-plan.schema.json"),
        _ => {
            return ContractValidationResult {
                valid: false,
                schema_name,
                error: Some("unsupported schema name".to_string()),
            };
        }
    };

    let result =
        compile_schema(&schema_file).and_then(|schema| validate_json_str(&schema, &json_text));

    match result {
        Ok(_) => ContractValidationResult {
            valid: true,
            schema_name,
            error: None,
        },
        Err(err) => ContractValidationResult {
            valid: false,
            schema_name,
            error: Some(err.to_string()),
        },
    }
}

#[tauri::command]
pub fn orchestrator_status() -> OrchestratorStatus {
    let status = FakeOrchestrator::status();
    OrchestratorStatus {
        mode: status.mode,
        active_runs: status.active_runs,
        message: status.message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_app_info_reports_fake_orchestrator() {
        let info = get_app_info();
        assert_eq!(info.name, "Tiamat");
        assert_eq!(info.orchestrator_mode, "fake-no-op");
        assert_eq!(info.schema_version, 1);
    }

    #[test]
    fn validate_contract_json_accepts_valid_intake_fixture() {
        let fixture = std::fs::read_to_string(
            tiamat_contracts::repo_root().join("fixtures/contracts/v1/intake-manifest.valid.json"),
        )
        .expect("fixture");
        let result = validate_contract_json("intake-manifest".to_string(), fixture);
        assert!(result.valid);
        assert!(result.error.is_none());
    }

    #[test]
    fn validate_contract_json_rejects_invalid_fixture() {
        let fixture = std::fs::read_to_string(
            tiamat_contracts::repo_root()
                .join("fixtures/contracts/v1/invalid/intake-wrong-schema-version.json"),
        )
        .expect("fixture");
        let result = validate_contract_json("intake-manifest".to_string(), fixture);
        assert!(!result.valid);
        assert!(result.error.is_some());
    }
}
