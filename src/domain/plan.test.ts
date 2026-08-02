import { describe, expect, it } from "vitest";
import type { ProjectPlan } from "./contracts";
import { projectGraphFromPlan } from "./plan";

const samplePlan: ProjectPlan = {
  schemaVersion: 1,
  runId: "d4e5f6a7-b8c9-4012-d345-6789abcdef01",
  title: "Rough-spec notes tool",
  summary: "Build notes list",
  assumptions: [],
  risks: [],
  phases: [
    {
      phaseId: "P01",
      title: "Core",
      objective: "Ship list",
      dependencies: [],
      projectIds: ["notes"],
      readRoots: ["."],
      writeRoots: ["."],
      modelTier: "composer",
      estimatedMinutes: 10,
      acceptanceCriteria: [
        {
          criterionId: "AC-P01-01",
          description: "works",
          requiredEvidenceKinds: ["unit"],
        },
      ],
      unitTests: [],
      integrationTests: [],
      e2eTests: [],
      manualChecks: [],
      rollback: { checkpoint: "base", strategy: "restore" },
      expectedArtifacts: [],
      prompt: "Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json",
      status: "draft",
      evidence: [],
    },
    {
      phaseId: "P02",
      title: "Follow-up",
      objective: "Next",
      dependencies: ["P01"],
      projectIds: ["notes"],
      readRoots: ["."],
      writeRoots: ["."],
      modelTier: "grok-low",
      estimatedMinutes: 10,
      acceptanceCriteria: [
        {
          criterionId: "AC-P02-01",
          description: "works",
          requiredEvidenceKinds: ["unit"],
        },
      ],
      unitTests: [],
      integrationTests: [],
      e2eTests: [],
      manualChecks: [],
      rollback: { checkpoint: "base", strategy: "restore" },
      expectedArtifacts: [],
      prompt: "Read .tiamat/MASTER-PLAN.md and .tiamat/plan.json",
      status: "ready",
      evidence: [],
    },
  ],
  finalGates: [],
};

describe("plan graph projection", () => {
  it("projects nodes and dependency edges", () => {
    const graph = projectGraphFromPlan(samplePlan);
    expect(graph.nodes.map((n) => n.phaseId)).toEqual(["P01", "P02"]);
    expect(graph.edges).toEqual([{ from: "P01", to: "P02" }]);
    expect(graph.title).toBe("Rough-spec notes tool");
  });
});
