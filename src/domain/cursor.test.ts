import { describe, expect, it } from "vitest";
import {
  EMPTY_FEATURES,
  cliConnectionState,
  formatCursorStatus,
  hasNoninteractiveApproval,
  type CursorCapabilityReport,
} from "./cursor";

function report(
  overrides: Partial<CursorCapabilityReport> = {},
): CursorCapabilityReport {
  return {
    status: "available",
    message: "ok",
    minimumVersion: "0.1.0",
    features: { ...EMPTY_FEATURES, force: true, modePlan: true },
    auth: "ready",
    models: [],
    version: "1.2.3",
    executable: "/bin/agent",
    probedAtUtc: "2026-08-02T00:00:00Z",
    ...overrides,
  };
}

describe("cursor domain", () => {
  it("formats status for the header", () => {
    expect(formatCursorStatus(report())).toBe("available v1.2.3 · auth ready");
    expect(hasNoninteractiveApproval(report().features)).toBe(true);
  });

  it("requires force or auto-review for noninteractive approval", () => {
    expect(hasNoninteractiveApproval(EMPTY_FEATURES)).toBe(false);
    expect(
      hasNoninteractiveApproval({ ...EMPTY_FEATURES, autoReview: true }),
    ).toBe(true);
  });

  it("maps capability into connection light states", () => {
    expect(cliConnectionState(null, true).kind).toBe("checking");
    expect(cliConnectionState(report()).kind).toBe("connected");
    expect(cliConnectionState(report({ auth: "unauthenticated" })).kind).toBe(
      "auth_needed",
    );
    expect(cliConnectionState(report({ status: "absent" })).kind).toBe(
      "disconnected",
    );
    expect(
      cliConnectionState(
        report({ features: { ...EMPTY_FEATURES, modePlan: false } }),
      ).kind,
    ).toBe("disconnected");
  });
});
