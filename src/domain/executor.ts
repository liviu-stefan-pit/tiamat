export type EvidenceClassification =
  | "pass"
  | "fail"
  | "baseline_fail"
  | "flaky_pass"
  | "flaky_fail"
  | "skipped"
  | "policy_denied";

export type PhaseResultStatus = "passed" | "failed" | "needs_review";

export interface PhaseResult {
  schemaVersion: number;
  phaseId: string;
  attemptId?: string;
  status: PhaseResultStatus;
  summary: string;
  changedFiles: string[];
  evidenceIds: string[];
  acceptanceSatisfied: string[];
  artifacts: string[];
  notes?: string[];
  immutable: true;
  progressUseful?: boolean;
  interruption?: string;
}

export interface EvidenceRecord {
  schemaVersion: number;
  evidenceId: string;
  kind: string;
  testId?: string;
  command: string[];
  workingDirectory: string;
  exitCode: number;
  durationMs: number;
  summary: string;
  artifactHashes: string[];
  covers: string[];
  trustworthy: boolean;
  partial: boolean;
  classification: EvidenceClassification;
  startedAtUtc: string;
  endedAtUtc: string;
  baselineExitCode?: number;
  flakyRetry?: boolean;
}

export interface LayerGateSummary {
  kind: string;
  required: boolean;
  executed: number;
  passed: number;
  failed: number;
  skipped: number;
  inapplicable: boolean;
  inapplicableReason?: string;
}

export interface RecoveryReport {
  decision: string;
  progressUseful: boolean;
  reason: string;
  resumed: boolean;
  rolledBack: boolean;
}

export interface PhaseExecutionOutcome {
  ok: boolean;
  runId: string;
  phaseId: string;
  attemptId?: string;
  terminalStatus: string;
  phaseResult?: PhaseResult;
  evidence: EvidenceRecord[];
  layers: LayerGateSummary[];
  changedFiles: string[];
  boundaryOk: boolean;
  quarantined?: { quarantineId: string; reason: string };
  projectCheckpoint?: { checkpointId: string; commit: string; message: string };
  controlCheckpoint?: { checkpointId: string; commit: string; message: string };
  planProjected: boolean;
  recovery?: RecoveryReport;
  chatId?: string;
  message: string;
  evidenceNotes: string[];
}

export function checkpointAfterAllGates(
  outcome: PhaseExecutionOutcome,
): boolean {
  if (!outcome.ok || !outcome.projectCheckpoint) return false;
  const required = outcome.layers.filter((l) => l.required && !l.inapplicable);
  return (
    required.length >= 3 &&
    required.every((l) => l.failed === 0 && l.passed > 0) &&
    outcome.planProjected
  );
}
