import { describe, expect, it, vi, beforeEach } from "vitest";
import * as tauriCore from "@tauri-apps/api/core";
import {
  getAppInfo,
  getOrchestratorStatus,
  validateContractJson,
} from "./commands";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("tauri commands", () => {
  beforeEach(() => {
    vi.mocked(tauriCore.invoke).mockReset();
  });

  it("getAppInfo invokes backend command", async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValue({
      name: "Tiamat",
      version: "0.1.0",
      schemaVersion: 1,
      orchestratorMode: "fake-no-op",
    });

    const info = await getAppInfo();
    expect(tauriCore.invoke).toHaveBeenCalledWith("get_app_info");
    expect(info.orchestratorMode).toBe("fake-no-op");
  });

  it("validateContractJson passes schema and payload", async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValue({
      valid: true,
      schemaName: "intake-manifest",
    });

    const result = await validateContractJson("intake-manifest", "{}");
    expect(tauriCore.invoke).toHaveBeenCalledWith("validate_contract_json", {
      schemaName: "intake-manifest",
      jsonText: "{}",
    });
    expect(result.valid).toBe(true);
  });

  it("getOrchestratorStatus invokes backend command", async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValue({
      mode: "fake-no-op",
      activeRuns: 0,
      message: "disabled",
    });

    const status = await getOrchestratorStatus();
    expect(tauriCore.invoke).toHaveBeenCalledWith("orchestrator_status");
    expect(status.activeRuns).toBe(0);
  });
});
