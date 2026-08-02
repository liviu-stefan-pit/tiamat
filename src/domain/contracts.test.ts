import { describe, expect, it } from "vitest";
import intakeFixture from "../../fixtures/contracts/v1/intake-manifest.valid.json";
import eventFixture from "../../fixtures/contracts/v1/event-envelope.valid.json";
import planFixture from "../../fixtures/contracts/v1/project-plan.valid.json";
import {
  assertSchemaVersion,
  CURRENT_SCHEMA_VERSION,
  type EventEnvelope,
  type IntakeManifest,
  type ProjectPlan,
} from "./contracts";

describe("domain contracts", () => {
  it("parses intake manifest fixture with current schema version", () => {
    const manifest = intakeFixture as IntakeManifest;
    assertSchemaVersion(manifest.schemaVersion);
    expect(manifest.projects[0].projectId).toBe("sample-app");
  });

  it("parses event envelope fixture", () => {
    const envelope = eventFixture as EventEnvelope;
    assertSchemaVersion(envelope.schemaVersion);
    expect(envelope.type).toBe("run.created");
  });

  it("parses project plan fixture", () => {
    const plan = planFixture as ProjectPlan;
    assertSchemaVersion(plan.schemaVersion);
    expect(plan.phases[0].phaseId).toBe("P00");
  });

  it("rejects incompatible schema versions", () => {
    expect(() => assertSchemaVersion(99)).toThrow(/incompatible schema version/);
    expect(() => assertSchemaVersion(CURRENT_SCHEMA_VERSION)).not.toThrow();
  });
});
