import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  ContractSchemaName,
  ContractValidationResult,
  OrchestratorStatus,
} from "../../domain/contracts";

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

export async function validateContractJson(
  schemaName: ContractSchemaName,
  jsonText: string,
): Promise<ContractValidationResult> {
  return invoke<ContractValidationResult>("validate_contract_json", {
    schemaName,
    jsonText,
  });
}

export async function getOrchestratorStatus(): Promise<OrchestratorStatus> {
  return invoke<OrchestratorStatus>("orchestrator_status");
}
