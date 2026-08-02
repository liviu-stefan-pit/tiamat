import { describe, expect, it } from "vitest";
import { buildCompletionSummary } from "./reports";
import type { ProjectPlan } from "./contracts";
import type { SchedulerSnapshot } from "./scheduler";

const plan: ProjectPlan = {
  schemaVersion: 1,
  runId: "r1",
  title: "Demo plan",
  summary: "sum",
  assumptions: [],
  risks: [],
  phases: [
    {
      phaseId: "P01",
      title: "A",
      objective: "o",
      dependencies: [],
      projectIds: ["p"],
      readRoots: [],
      writeRoots: ["w"],
      modelTier: "composer",
      estimatedMinutes: 5,
      acceptanceCriteria: [],
      unitTests: [],
      integrationTests: [],
      e2eTests: [],
      manualChecks: [],
      rollback: { checkpoint: "c", strategy: "restore" },
      expectedArtifacts: [],
      prompt: "p",
      status: "passed",
      evidence: [],
    },
  ],
  finalGates: [],
};

const scheduler: SchedulerSnapshot = {
  runId: "r1",
  mode: "dag-scheduler",
  paused: false,
  epoch: 1,
  maxConcurrent: 2,
  activeAttempts: 0,
  phases: [
    {
      phaseId: "P01",
      title: "A",
      status: "passed",
      modelTier: "composer",
      attemptCount: 1,
      writeRoots: ["w"],
    },
  ],
  attempts: [
    {
      attemptId: "a1",
      phaseId: "P01",
      attemptNumber: 1,
      status: "completed",
      selectedModel: "composer-2.5",
      selectionReason: "tier",
      terminalResult: "succeeded",
    },
  ],
  heldLocks: [],
};

describe("buildCompletionSummary", () => {
  it("tallies phases and requires cleanup for complete", () => {
    const summary = buildCompletionSummary({
      runId: "r1",
      runStatus: "completed",
      plan,
      scheduler,
      executor: null,
      workspace: null,
      events: [
        {
          schemaVersion: 1,
          eventId: "e1",
          sequence: 1,
          runId: "r1",
          type: "cleanup.succeeded",
          level: "info",
          timestampUtc: "2026-08-02T09:00:00Z",
          message: "Cleanup proof: zero active Job processes",
          payload: {},
        },
      ],
      processRegistryEmpty: true,
    });
    expect(summary.counts.completed).toBe(1);
    expect(summary.cleanupConfirmed).toBe(true);
    expect(summary.complete).toBe(true);
    expect(summary.promotionInstructions.length).toBeGreaterThan(0);
  });
});
