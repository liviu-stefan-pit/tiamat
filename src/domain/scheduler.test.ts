import { describe, expect, it } from "vitest";
import { projectGraphFromScheduler, type SchedulerSnapshot } from "./scheduler";

describe("scheduler graph projection", () => {
  it("surfaces parallel blocked paused and escalated statuses", () => {
    const snap: SchedulerSnapshot = {
      runId: "11111111-1111-4111-8111-111111111111",
      mode: "dag-scheduler",
      paused: true,
      epoch: 3,
      maxConcurrent: 2,
      activeAttempts: 1,
      phases: [
        {
          phaseId: "P01",
          title: "A",
          status: "running",
          modelTier: "composer",
          selectedModel: "composer-2.5",
          attemptCount: 1,
          writeRoots: ["C:\\a"],
        },
        {
          phaseId: "P02",
          title: "B",
          status: "running",
          modelTier: "composer",
          selectedModel: "composer-2.5",
          attemptCount: 1,
          writeRoots: ["C:\\b"],
        },
        {
          phaseId: "P03",
          title: "C",
          status: "blocked",
          modelTier: "grok-low",
          attemptCount: 0,
          writeRoots: ["C:\\a"],
        },
        {
          phaseId: "P04",
          title: "D",
          status: "ready",
          modelTier: "composer",
          selectedModel: "cursor-grok-4.5-low",
          selectionReason: "escalated after timeout",
          attemptCount: 2,
          writeRoots: ["C:\\c"],
        },
      ],
      attempts: [],
      heldLocks: ["write:c:\\a", "write:c:\\b"],
    };

    const graph = projectGraphFromScheduler("Scheduler demo", snap.runId, snap);
    expect(graph.nodes.map((n) => n.status)).toEqual([
      "running",
      "running",
      "blocked",
      "ready",
    ]);
    expect(snap.paused).toBe(true);
    expect(graph.nodes[3]?.objective).toContain("escalated");
  });
});
