import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { EventEnvelope } from "../../domain/contracts";
import { ActivityLog } from "./ActivityLog";

function makeEvents(count: number): EventEnvelope[] {
  return Array.from({ length: count }, (_, index) => ({
    schemaVersion: 1,
    eventId: `e-${index + 1}`,
    sequence: index + 1,
    runId: "r1",
    phaseId: index % 2 === 0 ? "P01" : "P02",
    type: index % 2 === 0 ? "phase.started" : "attempt.started",
    level: index % 5 === 0 ? ("warning" as const) : ("info" as const),
    timestampUtc: new Date(Date.UTC(2026, 7, 2, 9, 0, index)).toISOString(),
    message:
      index === 0
        ? "short"
        : `long message ${"y".repeat(200)} #${index + 1}`,
    payload: {},
  }));
}

describe("ActivityLog", () => {
  it("virtualizes a large event list and supports export", async () => {
    render(<ActivityLog events={makeEvents(500)} statusLine="running" />);

    expect(screen.getByTestId("activity-log")).toBeInTheDocument();
    expect(screen.getByTestId("run-status-line")).toHaveTextContent("running");
    expect(screen.getByTestId("log-count").textContent).toBe("500");
    const rendered = screen.getAllByTestId("log-event").length;
    expect(rendered).toBeLessThan(120);
    expect(screen.getByTestId("log-export")).toBeInTheDocument();
  });

  it("filters by level", async () => {
    const user = userEvent.setup();
    render(<ActivityLog events={makeEvents(20)} />);
    await user.selectOptions(screen.getByTestId("log-level-filter"), "warning");
    const rows = screen.getAllByTestId("log-event");
    expect(rows.length).toBeGreaterThan(0);
    for (const row of rows) {
      expect(row.className).toMatch(/level-warning/);
    }
  });
});
