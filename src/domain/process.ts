export type ProcessState =
  | "registered"
  | "spawned"
  | "active"
  | "graceful_stop"
  | "forced_stop"
  | "reaped";

export interface ProcessRecord {
  processId: string;
  runId: string;
  phaseId?: string;
  attemptId?: string;
  executable: string;
  argsRedacted: string[];
  pid?: number;
  state: ProcessState;
  chatId?: string;
  terminalReason?: string;
  resumeMetadata: Record<string, unknown>;
  cleanupEvidence: Record<string, unknown>;
}

export interface AbortSettings {
  shortcut: string;
  registered: boolean;
  degraded: boolean;
  collisionReason?: string;
  degradedAcknowledged: boolean;
  trayFallbackEnabled: boolean;
  secondPressForceMs: number;
  updatedAtUtc: string;
}

export interface ProcessRegistrySnapshot {
  activeCount: number;
  processes: ProcessRecord[];
  abort: AbortSettings;
  canStart: boolean;
  cleanupIncomplete: boolean;
}

export interface AbortPressResult {
  action:
    | "begin_emergency_cancel"
    | "force_terminate"
    | "prompt_confirm"
    | "acknowledged";
  forced: boolean;
  activeRun: boolean;
  message: string;
  processesStopped: number;
  cleanupOk: boolean;
}

export interface HostedProcessOutcome {
  processId: string;
  exitCode?: number;
  timedOut: boolean;
  cancelled: boolean;
  killed: boolean;
  stdout: string;
  stderr: string;
  durationMs: number;
  chatId?: string;
  resume?: {
    chatId?: string;
    nextModel?: string;
    nextTier?: string;
    reason: string;
    progressUseful: boolean;
    recoveryPrompt: string;
  };
  cleanupOk: boolean;
  zeroSurvivors: boolean;
  activeAfterCleanup: number;
}

export interface ClosePolicyEvent {
  message: string;
  choices: string[];
}

export function formatAbortStatus(settings: AbortSettings): string {
  if (settings.registered && !settings.degraded) {
    return `Global abort: ${settings.shortcut}`;
  }
  if (settings.degraded) {
    return settings.degradedAcknowledged
      ? `Global abort degraded (tray fallback) — ${settings.collisionReason ?? "collision"}`
      : `Global abort degraded — acknowledge before Start`;
  }
  return `Global abort pending registration`;
}
