import { describe, expect, it } from "vitest";
import { canStartImplementation, type PreflightReport } from "./intake";

function baseReport(overrides: Partial<PreflightReport> = {}): PreflightReport {
  return {
    schemaVersion: 1,
    manifest: {
      schemaVersion: 1,
      intakeId: "11111111-1111-4111-8111-111111111111",
      sources: [{ path: "C:\\tmp\\demo", kind: "folder", readOnly: true }],
      projects: [
        {
          projectId: "demo",
          root: "C:\\tmp\\demo",
          kind: "folder",
          languages: ["typescript"],
          buildSystems: ["npm"],
          testCommands: ["npm test"],
          warnings: ["Secret-risk markers detected (1)."],
        },
      ],
      inventoryArtifact: "inventory-abc",
    },
    inventory: {
      fileCount: 2,
      dirCount: 1,
      totalBytes: 100,
      ignoredCount: 0,
      truncated: false,
      estimatedCopyBytes: 100,
    },
    warnings: ["Detected 1 secret-risk marker(s)."],
    blockers: [],
    secretRisks: [
      {
        relativePath: "config.env",
        patternId: "aws_access_key_id",
        matchHash: "abc",
        matchByteLen: 20,
      },
    ],
    escapeAttempts: [],
    trust: {
      confirmed: false,
      acknowledgedUntrusted: false,
      acknowledgedExecutionRisk: false,
    },
    cursor: {
      status: "available",
      message: "fake",
      version: "1.2.3",
      auth: "ready",
    },
    canStart: false,
    readRoots: ["C:\\tmp\\demo"],
    writeRootsPreview: ["<managed-run-root>/projects/*"],
    limits: {
      maxFiles: 100,
      maxTotalBytes: 1000,
      maxFileBytes: 100,
      maxSecretScanBytes: 100,
      maxDepth: 8,
    },
    untrustedContentNotice: "untrusted",
    ...overrides,
  };
}

describe("intake domain", () => {
  it("gates Start until canStart is true", () => {
    expect(canStartImplementation(null)).toBe(false);
    expect(canStartImplementation(baseReport())).toBe(false);
    expect(
      canStartImplementation(baseReport({ canStart: true, trust: {
        confirmed: true,
        acknowledgedUntrusted: true,
        acknowledgedExecutionRisk: true,
      }})),
    ).toBe(true);
  });

  it("never embeds fixture secret values in report helpers", () => {
    const report = baseReport();
    const serialized = JSON.stringify(report);
    expect(serialized).not.toContain("AKIAIOSFODNN7EXAMPLE");
    expect(serialized).not.toContain("fixture-secret-value");
  });
});
