import { describe, expect, it, vi, beforeEach } from "vitest";
import * as tauriCore from "@tauri-apps/api/core";
import {
  confirmIntakeTrust,
  ensureDemoRun,
  exportWorkspaceProject,
  getAppInfo,
  getOrchestratorStatus,
  materializeWorkspace,
  previewCursorCommand,
  probeCursorCapability,
  promoteWorkspace,
  replayEvents,
  runIntakePreflight,
  validateContractJson,
} from "./commands";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("./runtime", () => ({
  isTauriRuntime: () => true,
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
      orchestratorMode: "dag-scheduler",
      storeSchemaVersion: 3,
    });

    const info = await getAppInfo();
    expect(tauriCore.invoke).toHaveBeenCalledWith("get_app_info");
    expect(info.orchestratorMode).toBe("dag-scheduler");
    expect(info.storeSchemaVersion).toBe(3);
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
      mode: "dag-scheduler",
      activeRuns: 0,
      message: "Durable DAG scheduler ready.",
    });

    const status = await getOrchestratorStatus();
    expect(tauriCore.invoke).toHaveBeenCalledWith("orchestrator_status");
    expect(status.activeRuns).toBe(0);
  });

  it("ensureDemoRun and replayEvents use typed bridge commands", async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValueOnce({
      run: {
        runId: "11111111-1111-4111-8111-111111111111",
        status: "executing",
        title: "demo",
        createdAtUtc: "2026-08-02T09:00:00Z",
        updatedAtUtc: "2026-08-02T09:00:00Z",
        metadata: {},
      },
      events: [],
      artifacts: [],
    });
    vi.mocked(tauriCore.invoke).mockResolvedValueOnce([
      {
        schemaVersion: 1,
        eventId: "e1",
        sequence: 1,
        runId: "11111111-1111-4111-8111-111111111111",
        type: "run.created",
        level: "info",
        timestampUtc: "2026-08-02T09:00:00Z",
        message: "Run created",
        payload: {},
      },
    ]);

    await ensureDemoRun();
    const events = await replayEvents(
      "11111111-1111-4111-8111-111111111111",
      0,
    );
    expect(tauriCore.invoke).toHaveBeenNthCalledWith(1, "ensure_demo_run");
    expect(tauriCore.invoke).toHaveBeenNthCalledWith(2, "replay_events", {
      runId: "11111111-1111-4111-8111-111111111111",
      afterSequence: 0,
    });
    expect(events).toHaveLength(1);
  });

  it("runIntakePreflight and confirmIntakeTrust pass typed args", async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValueOnce({
      canStart: false,
      manifest: { intakeId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee" },
    });
    vi.mocked(tauriCore.invoke).mockResolvedValueOnce({
      canStart: true,
      trust: { confirmed: true },
    });

    await runIntakePreflight(["C:\\tmp\\demo"]);
    await confirmIntakeTrust(
      "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
      true,
      true,
    );

    expect(tauriCore.invoke).toHaveBeenNthCalledWith(1, "run_intake_preflight", {
      paths: ["C:\\tmp\\demo"],
    });
    expect(tauriCore.invoke).toHaveBeenNthCalledWith(2, "confirm_intake_trust", {
      intakeId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
      acknowledgedUntrusted: true,
      acknowledgedExecutionRisk: true,
    });
  });

  it("probeCursorCapability and previewCursorCommand use typed bridge commands", async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValueOnce({
      status: "available",
      version: "1.2.3",
      features: { streamJson: true },
    });
    vi.mocked(tauriCore.invoke).mockResolvedValueOnce({
      argv: ["agent", "--print"],
      spawned: false,
      stdinPreview: "redacted",
    });

    await probeCursorCapability();
    await previewCursorCommand({
      workspace: "C:\\managed",
      prompt: "hello",
      apiKey: "sekrit",
    });

    expect(tauriCore.invoke).toHaveBeenNthCalledWith(
      1,
      "probe_cursor_capability",
    );
    expect(tauriCore.invoke).toHaveBeenNthCalledWith(
      2,
      "preview_cursor_command",
      {
        args: {
          workspace: "C:\\managed",
          prompt: "hello",
          model: undefined,
          resumeChatId: undefined,
          force: undefined,
          trust: undefined,
          planMode: undefined,
          apiKey: "sekrit",
          timeoutMs: undefined,
        },
      },
    );
  });

  it("materializeWorkspace passes run id and worktree flag", async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValue({
      schemaVersion: 1,
      runId: "11111111-1111-4111-8111-111111111111",
      sourceUnchanged: true,
      projects: [],
    });
    await materializeWorkspace("11111111-1111-4111-8111-111111111111", false);
    expect(tauriCore.invoke).toHaveBeenCalledWith("materialize_workspace", {
      runId: "11111111-1111-4111-8111-111111111111",
      createInternalWorktrees: false,
    });
  });

  it("exportWorkspaceProject and promoteWorkspace invoke backend", async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValue({
      schemaVersion: 1,
      promotion: { status: "exported" },
      projects: [],
    });
    await exportWorkspaceProject("demo", "C:\\exports");
    expect(tauriCore.invoke).toHaveBeenCalledWith("export_workspace_project", {
      projectId: "demo",
      exportDir: "C:\\exports",
    });
    vi.mocked(tauriCore.invoke).mockResolvedValue({
      schemaVersion: 1,
      promotion: { status: "promoted" },
      projects: [],
    });
    await promoteWorkspace("ok");
    expect(tauriCore.invoke).toHaveBeenCalledWith("promote_workspace", {
      notes: "ok",
    });
  });
});
