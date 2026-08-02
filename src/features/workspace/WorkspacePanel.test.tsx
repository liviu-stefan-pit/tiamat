import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RunWorkspaceManifest } from "../../domain/workspace";
import { WorkspacePanel } from "./WorkspacePanel";

const manifest: RunWorkspaceManifest = {
  schemaVersion: 1,
  runId: "11111111-1111-4111-8111-111111111111",
  intakeId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
  managedRunRoot: "C:\\managed\\run-1",
  controlRoot: "C:\\managed\\run-1\\control",
  projects: [
    {
      projectId: "demo",
      sourceRoot: "C:\\fixture\\demo",
      managedRoot: "C:\\managed\\run-1\\projects\\demo",
      kind: "gitClone",
      baselineBranch: "tiamat/intake-demo",
      writeRoot: "C:\\managed\\run-1\\projects\\demo",
      readRoots: ["C:\\managed\\run-1"],
      sourceFingerprint: {
        path: "C:\\fixture\\demo",
        kind: "git",
        statusPorcelain: "",
        statusHash: "0",
        treeHash: "0",
        capturedAtUtc: "2026-08-02T09:00:00Z",
      },
      lockName: "write:demo",
    },
  ],
  notesRoots: [],
  checkpoints: [],
  quarantines: [],
  promotion: { status: "unpromoted" },
  retention: {
    retainUnpromoted: true,
    maxQuarantineEntries: 32,
    allowDestructiveCleanup: false,
  },
  fingerprintPairs: [],
  createdAtUtc: "2026-08-02T09:00:00Z",
  sourceUnchanged: true,
};

describe("WorkspacePanel", () => {
  it("shows source unchanged and managed roots", () => {
    render(<WorkspacePanel manifest={manifest} />);
    expect(screen.getByTestId("workspace-source-unchanged")).toHaveTextContent(
      "unchanged",
    );
    expect(screen.getByTestId("workspace-managed-root")).toHaveTextContent(
      "C:\\managed\\run-1",
    );
    expect(screen.getByTestId("workspace-project")).toHaveTextContent("demo");
  });

  it("exposes export and promote actions", async () => {
    const onExport = vi.fn();
    const onPromote = vi.fn();
    const { userEvent } = await import("@testing-library/user-event");
    const user = userEvent.setup();
    render(
      <WorkspacePanel
        manifest={manifest}
        onExportProject={onExport}
        onPromote={onPromote}
      />,
    );
    expect(screen.getByTestId("workspace-actions")).toBeInTheDocument();
    await user.click(screen.getByTestId("workspace-export"));
    expect(onExport).toHaveBeenCalledWith("demo");
    await user.click(screen.getByTestId("workspace-promote"));
    expect(onPromote).toHaveBeenCalled();
  });
});
