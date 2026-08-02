import { describe, expect, it } from "vitest";
import type { EventEnvelope } from "./contracts";
import {
  DEFAULT_EVENT_FILTER,
  eventCategory,
  exportFilteredEventsJson,
  filterEvents,
  truncateMessage,
} from "./events";

const events: EventEnvelope[] = [
  {
    schemaVersion: 1,
    eventId: "1",
    sequence: 1,
    runId: "r",
    type: "run.created",
    level: "info",
    timestampUtc: "2026-08-02T09:00:00Z",
    message: "Run created",
    payload: {},
  },
  {
    schemaVersion: 1,
    eventId: "2",
    sequence: 2,
    runId: "r",
    phaseId: "P01",
    attemptId: "a1",
    type: "phase.started",
    level: "warning",
    timestampUtc: "2026-08-02T09:00:01Z",
    message: "Phase started",
    payload: {},
  },
  {
    schemaVersion: 1,
    eventId: "3",
    sequence: 3,
    runId: "r",
    type: "system.info",
    level: "error",
    timestampUtc: "2026-08-02T09:00:02Z",
    message: "Boom",
    payload: {},
  },
  {
    schemaVersion: 1,
    eventId: "4",
    sequence: 4,
    runId: "r",
    type: "test.unit.passed",
    level: "info",
    timestampUtc: "2026-08-02T09:00:03Z",
    message: "unit ok",
    payload: {},
  },
];

describe("filterEvents", () => {
  it("filters by level and type prefix", () => {
    const filtered = filterEvents(events, {
      ...DEFAULT_EVENT_FILTER,
      level: "warning",
      typePrefix: "phase",
    });
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.eventId).toBe("2");
  });

  it("filters by free-text query", () => {
    const filtered = filterEvents(events, {
      ...DEFAULT_EVENT_FILTER,
      query: "boom",
    });
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.type).toBe("system.info");
  });

  it("filters by category and phase", () => {
    expect(eventCategory(events[3]!)).toBe("test");
    const filtered = filterEvents(events, {
      ...DEFAULT_EVENT_FILTER,
      category: "phase",
      phaseId: "P01",
    });
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.eventId).toBe("2");
  });

  it("truncates long messages and exports filtered json", () => {
    const long = "x".repeat(200);
    expect(truncateMessage(long).truncated).toBe(true);
    const json = exportFilteredEventsJson(events, {
      ...DEFAULT_EVENT_FILTER,
      category: "test",
    });
    expect(json).toContain("test.unit.passed");
    expect(json).not.toContain("phase.started");
  });
});
