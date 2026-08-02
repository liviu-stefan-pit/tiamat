import { describe, expect, it } from "vitest";
import {
  checkpointAfterAllGates,
  type PhaseExecutionOutcome,
} from "./executor";

describe("executor domain", () => {
  it("requires all three gates before checkpoint success", () => {
    const base: PhaseExecutionOutcome = {
      ok: true,
      runId: "11111111-1111-4111-8111-111111111111",
      phaseId: "P01",
      terminalStatus: "passed",
      evidence: [],
      layers: [
        {
          kind: "unit",
          required: true,
          executed: 1,
          passed: 1,
          failed: 0,
          skipped: 0,
          inapplicable: false,
        },
        {
          kind: "integration",
          required: true,
          executed: 1,
          passed: 1,
          failed: 0,
          skipped: 0,
          inapplicable: false,
        },
        {
          kind: "e2e",
          required: true,
          executed: 1,
          passed: 1,
          failed: 0,
          skipped: 0,
          inapplicable: false,
        },
      ],
      changedFiles: ["src/feature.ts"],
      boundaryOk: true,
      projectCheckpoint: {
        checkpointId: "cp-1",
        commit: "abc",
        message: "phase P01 passed gates",
      },
      planProjected: true,
      message: "ok",
      evidenceNotes: [],
    };
    expect(checkpointAfterAllGates(base)).toBe(true);
    expect(
      checkpointAfterAllGates({
        ...base,
        layers: base.layers.map((l, i) =>
          i === 0 ? { ...l, failed: 1, passed: 0 } : l,
        ),
        ok: false,
        projectCheckpoint: undefined,
      }),
    ).toBe(false);
  });
});
