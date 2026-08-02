import { describe, expect, it } from "vitest";
import {
  summarizeWorkspace,
  type RunWorkspaceManifest,
} from "./workspace";

const sample: RunWorkspaceManifest = {
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
      baselineCommit: "abc",
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
  fingerprintPairs: [
    {
      before: {
        path: "C:\\fixture\\demo",
        kind: "git",
        statusPorcelain: "",
        statusHash: "0",
        treeHash: "0",
        capturedAtUtc: "2026-08-02T09:00:00Z",
      },
      after: {
        path: "C:\\fixture\\demo",
        kind: "git",
        statusPorcelain: "",
        statusHash: "0",
        treeHash: "0",
        capturedAtUtc: "2026-08-02T09:00:01Z",
      },
      unchanged: true,
    },
  ],
  createdAtUtc: "2026-08-02T09:00:00Z",
  sourceUnchanged: true,
};

describe("workspace domain", () => {
  it("summarizes isolated output status", () => {
    expect(summarizeWorkspace(sample)).toContain("source unchanged");
    expect(summarizeWorkspace(sample)).toContain("promotion=unpromoted");
  });
});
