import { describe, expect, it } from "vitest";
import {
  EMPTY_FEATURES,
  formatCursorStatus,
  hasNoninteractiveApproval,
  type CursorCapabilityReport,
} from "./cursor";

describe("cursor domain", () => {
  it("formats status for the header", () => {
    const report: CursorCapabilityReport = {
      status: "available",
      message: "ok",
      minimumVersion: "0.1.0",
      features: { ...EMPTY_FEATURES, force: true },
      auth: "ready",
      models: [],
      version: "1.2.3",
      probedAtUtc: "2026-08-02T00:00:00Z",
    };
    expect(formatCursorStatus(report)).toBe("available v1.2.3 · auth ready");
    expect(hasNoninteractiveApproval(report.features)).toBe(true);
  });

  it("requires force or auto-review for noninteractive approval", () => {
    expect(hasNoninteractiveApproval(EMPTY_FEATURES)).toBe(false);
    expect(
      hasNoninteractiveApproval({ ...EMPTY_FEATURES, autoReview: true }),
    ).toBe(true);
  });
});
