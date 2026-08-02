import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { EventEnvelope } from "../../domain/contracts";
import { DEFAULT_EVENT_FILTER } from "../../domain/events";
import { ActivityLog } from "./ActivityLog";

function makeEvents(count: number): EventEnvelope[] {
  return Array.from({ length: count }, (_, index) => ({
    schemaVersion: 1,
    eventId: `e-${index + 1}`,
    sequence: index + 1,
    runId: "r1",
    phaseId: index % 2 === 0 ? "P01" : "P02",
    type: index % 2 === 0 ? "phase.started" : "system.info",
    level: "info" as const,
    timestampUtc: new Date(Date.UTC(2026, 7, 2, 9, 0, index)).toISOString(),
    message:
      index === 0
        ? "short"
        : `long message ${"y".repeat(200)} #${index + 1}`,
    payload: {},
  }));
}

describe("ActivityLog", () => {
  it("virtualizes a large event list and supports follow/export controls", async () => {
    const user = userEvent.setup();
    const events = makeEvents(500);
    render(
      <ActivityLog
        events={events}
        filter={DEFAULT_EVENT_FILTER}
        onFilterChange={() => undefined}
      />,
    );

    expect(screen.getByTestId("activity-log")).toHaveAttribute(
      "data-virtualized",
      "true",
    );
    expect(screen.getByTestId("log-count").textContent).toContain(
      "Showing 500 of 500",
    );
    const rendered = screen.getAllByTestId("log-event").length;
    expect(rendered).toBeLessThan(120);
    expect(screen.getByTestId("log-follow")).toBeChecked();
    expect(screen.getByTestId("log-export")).toBeInTheDocument();

    await user.click(screen.getAllByTestId("log-expand")[0]!);
    expect(screen.getAllByTestId("log-expand")[0]!.textContent).toMatch(
      /Collapse/i,
    );
  });

  it("filters by category controls", async () => {
    const user = userEvent.setup();
    let filter = { ...DEFAULT_EVENT_FILTER };
    const { rerender } = render(
      <ActivityLog
        events={makeEvents(20)}
        filter={filter}
        onFilterChange={(next) => {
          filter = next;
        }}
      />,
    );
    await user.selectOptions(screen.getByTestId("log-category"), "phase");
    rerender(
      <ActivityLog
        events={makeEvents(20)}
        filter={{ ...filter, category: "phase" }}
        onFilterChange={(next) => {
          filter = next;
        }}
      />,
    );
    const rows = screen.getAllByTestId("log-event");
    for (const row of rows) {
      expect(row.getAttribute("data-type") ?? "").toMatch(/^phase\./);
    }
  });
});
