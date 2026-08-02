import { describe, expect, it } from "vitest";
import { recoveryBannerText, type RecoveryOffer } from "./recovery";

const base: RecoveryOffer = {
  offerId: "o1",
  runId: "11111111-1111-4111-8111-111111111111",
  status: "pending",
  reason: "interrupted run detected",
  dbIntegrityOk: true,
  processHardFailure: false,
  interruptedAttemptCount: 1,
  unreconciledSideEffects: 0,
  lowDisk: false,
  details: {},
  createdAtUtc: "2026-08-02T00:00:00Z",
  requiresUserChoice: true,
  resumeAllowed: true,
};

describe("recovery domain", () => {
  it("formats pending banner", () => {
    const text = recoveryBannerText(base);
    expect(text).toContain("Resume or Cancel");
    expect(text).toContain("interrupted run detected");
  });

  it("formats blocked banner", () => {
    const text = recoveryBannerText({
      ...base,
      status: "blocked",
      resumeAllowed: false,
      reason: "database integrity failure",
    });
    expect(text).toContain("Recovery blocked");
    expect(text).toContain("database integrity failure");
  });
});
