export type PhaseRuntimeStatus =
  | "draft"
  | "ready"
  | "queued"
  | "running"
  | "verifying"
  | "passed"
  | "failed"
  | "blocked"
  | "cancelled"
  | "skipped"
  | "needs_review";

export interface SchedulerPhaseView {
  phaseId: string;
  title: string;
  status: PhaseRuntimeStatus;
  modelTier: string;
  selectedModel?: string;
  selectionReason?: string;
  attemptCount: number;
  writeRoots: string[];
}

export interface SchedulerAttemptView {
  attemptId: string;
  phaseId: string;
  attemptNumber: number;
  status: string;
  selectedModel: string;
  selectionReason: string;
  terminalResult?: string;
}

export interface SchedulerSnapshot {
  runId: string;
  mode: string;
  paused: boolean;
  epoch: number;
  maxConcurrent: number;
  activeAttempts: number;
  phases: SchedulerPhaseView[];
  attempts: SchedulerAttemptView[];
  heldLocks: string[];
}

export interface TickResult {
  epoch: number;
  started: string[];
  blocked: string[];
  skippedDueToPause: boolean;
  skippedDueToCapacity: boolean;
  message: string;
}

export function projectGraphFromScheduler(
  title: string,
  runId: string,
  snap: SchedulerSnapshot,
): {
  runId: string;
  title: string;
  nodes: Array<{
    phaseId: string;
    title: string;
    status: string;
    modelTier: string;
    objective: string;
  }>;
  edges: Array<{ from: string; to: string }>;
} {
  return {
    runId,
    title,
    nodes: snap.phases.map((phase) => ({
      phaseId: phase.phaseId,
      title: phase.title,
      status: phase.status,
      modelTier: phase.modelTier,
      objective: phase.selectionReason ?? phase.title,
    })),
    edges: [],
  };
}
