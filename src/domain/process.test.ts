import { describe, expect, it } from "vitest";
import { formatAbortStatus, type AbortSettings } from "./process";

describe("process domain", () => {
  it("formats registered abort status", () => {
    const settings: AbortSettings = {
      shortcut: "Ctrl+Shift+F12",
      registered: true,
      degraded: false,
      degradedAcknowledged: false,
      trayFallbackEnabled: true,
      secondPressForceMs: 3000,
      updatedAtUtc: "2026-08-02T09:00:00Z",
    };
    expect(formatAbortStatus(settings)).toContain("Ctrl+Shift+F12");
  });

  it("requires ack messaging when degraded", () => {
    const settings: AbortSettings = {
      shortcut: "Ctrl+Shift+F12",
      registered: false,
      degraded: true,
      collisionReason: "already registered",
      degradedAcknowledged: false,
      trayFallbackEnabled: true,
      secondPressForceMs: 3000,
      updatedAtUtc: "2026-08-02T09:00:00Z",
    };
    expect(formatAbortStatus(settings)).toMatch(/acknowledge/i);
  });
});
