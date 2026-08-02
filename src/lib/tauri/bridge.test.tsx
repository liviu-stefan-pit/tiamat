import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, beforeEach } from "vitest";
import { Shell } from "../../features/shell/Shell";
import { resetBrowserStoreForTests } from "./browser-store";
import { mergeEvents } from "./bridge";
import type { EventEnvelope } from "../../domain/contracts";

describe("event bridge", () => {
  beforeEach(() => {
    resetBrowserStoreForTests();
  });

  it("mergeEvents keeps monotonic unique order", () => {
    const a: EventEnvelope = {
      schemaVersion: 1,
      eventId: "a",
      sequence: 1,
      runId: "r",
      type: "run.created",
      level: "info",
      timestampUtc: "2026-08-02T09:00:00Z",
      message: "a",
      payload: {},
    };
    const b: EventEnvelope = {
      ...a,
      eventId: "b",
      sequence: 2,
      message: "b",
    };
    const merged = mergeEvents([b, a], [a, b]);
    expect(merged.map((event) => event.eventId)).toEqual(["a", "b"]);
  });

  it("remount replays the same ordered events", async () => {
    const { unmount } = render(<Shell />);
    await waitFor(() => {
      expect(screen.getAllByTestId("log-event").length).toBeGreaterThan(3);
    });
    const first = screen
      .getAllByTestId("log-event")
      .map((node) => node.getAttribute("data-event-id"));

    unmount();

    render(<Shell />);
    await waitFor(() => {
      expect(screen.getAllByTestId("log-event").length).toBe(first.length);
    });
    const second = screen
      .getAllByTestId("log-event")
      .map((node) => node.getAttribute("data-event-id"));
    expect(second).toEqual(first);

    const sequences = screen
      .getAllByTestId("log-event")
      .map((node) => Number(node.getAttribute("data-sequence")));
    expect(sequences).toEqual([...sequences].sort((a, b) => a - b));
  });
});
