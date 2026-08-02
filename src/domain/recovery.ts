export type RecoveryOfferStatus =
  | "pending"
  | "resumed"
  | "cancelled"
  | "blocked";

export interface RecoveryOffer {
  offerId: string;
  runId: string;
  status: RecoveryOfferStatus;
  reason: string;
  dbIntegrityOk: boolean;
  processHardFailure: boolean;
  interruptedAttemptCount: number;
  unreconciledSideEffects: number;
  lowDisk: boolean;
  corruptDbBackupPath?: string | null;
  details: Record<string, unknown>;
  createdAtUtc: string;
  resolvedAtUtc?: string | null;
  resolution?: string | null;
  requiresUserChoice: boolean;
  resumeAllowed: boolean;
}

export interface RecoveryScanReport {
  schemaVersion: number;
  scannedAtUtc: string;
  dbIntegrityOk: boolean;
  schemaVersionOk: boolean;
  processReconcile?: {
    inspected: number;
    terminated: number;
    alreadyGone: number;
    unverifiable: number;
    interruptedAttempts: number;
    hardFailure: boolean;
    messages: string[];
  } | null;
  interruptedAttempts: Array<{
    attemptId: string;
    runId: string;
    phaseId: string;
    priorStatus: string;
    terminalResult: string;
  }>;
  unreconciledSideEffects: unknown[];
  lowDisk: boolean;
  freeDiskBytes?: number | null;
  diskPath?: string | null;
  offer?: RecoveryOffer | null;
  messages: string[];
}

export function recoveryBannerText(offer: RecoveryOffer): string {
  if (offer.status === "blocked") {
    return `Recovery blocked: ${offer.reason}`;
  }
  return `Interrupted run detected — choose Resume or Cancel before new work. ${offer.reason}`;
}
