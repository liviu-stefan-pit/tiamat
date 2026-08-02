import type {
  AppInfo,
  ContractSchemaName,
  ContractValidationResult,
  EventEnvelope,
  OrchestratorStatus,
} from "../../domain/contracts";
import type {
  CursorCapabilityReport,
  CursorCommandPreview,
  CursorModelsReport,
} from "../../domain/cursor";
import type { PreflightReport } from "../../domain/intake";
import type {
  RootValidationResult,
  RunWorkspaceManifest,
} from "../../domain/workspace";
import { browserInvoke } from "./browser-store";
import { isTauriRuntime } from "./runtime";

async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isTauriRuntime()) {
    const { invoke } = await import("@tauri-apps/api/core");
    if (args === undefined) {
      return invoke<T>(command);
    }
    return invoke<T>(command, args);
  }
  return browserInvoke<T>(command, args);
}

export interface RunRecord {
  runId: string;
  status: string;
  title: string;
  createdAtUtc: string;
  updatedAtUtc: string;
  metadata: Record<string, unknown>;
}

export interface ArtifactRecord {
  artifactId: string;
  contentHash: string;
  byteSize: number;
  mediaType?: string;
  relativePath?: string;
  createdAtUtc: string;
  metadata: Record<string, unknown>;
}

export interface DemoRunSnapshot {
  run: RunRecord;
  events: EventEnvelope[];
  artifacts: ArtifactRecord[];
}

export interface TransitionResult {
  run: RunRecord;
  event: EventEnvelope;
}

export async function getAppInfo(): Promise<AppInfo> {
  return invokeCommand<AppInfo>("get_app_info");
}

export async function validateContractJson(
  schemaName: ContractSchemaName,
  jsonText: string,
): Promise<ContractValidationResult> {
  return invokeCommand<ContractValidationResult>("validate_contract_json", {
    schemaName,
    jsonText,
  });
}

export async function getOrchestratorStatus(): Promise<OrchestratorStatus> {
  return invokeCommand<OrchestratorStatus>("orchestrator_status");
}

export async function ensureDemoRun(): Promise<DemoRunSnapshot> {
  return invokeCommand<DemoRunSnapshot>("ensure_demo_run");
}

export async function listRuns(): Promise<RunRecord[]> {
  return invokeCommand<RunRecord[]>("list_runs");
}

export async function replayEvents(
  runId: string,
  afterSequence = 0,
): Promise<EventEnvelope[]> {
  return invokeCommand<EventEnvelope[]>("replay_events", {
    runId,
    afterSequence,
  });
}

export async function listArtifacts(): Promise<ArtifactRecord[]> {
  return invokeCommand<ArtifactRecord[]>("list_artifacts");
}

export async function transitionRunStatus(
  runId: string,
  newStatus: string,
  message: string,
  eventType?: string,
): Promise<TransitionResult> {
  return invokeCommand<TransitionResult>("transition_run_status", {
    runId,
    newStatus,
    message,
    eventType,
  });
}

export async function pickIntakePaths(
  kind: "file" | "folder",
): Promise<string[]> {
  return invokeCommand<string[]>("pick_intake_paths", { kind });
}

export async function pickOutputDir(): Promise<string | null> {
  return invokeCommand<string | null>("pick_output_dir");
}

export interface StartRunResult {
  runId: string;
  status: string;
  message: string;
  managedRunRoot?: string | null;
}

export interface RunStatusSnapshot {
  runId?: string | null;
  status: string;
  phase?: string | null;
  message: string;
  activeAttempts: number;
  completedPhases: number;
  totalPhases: number;
  managedRunRoot?: string | null;
}

export async function startRun(input: {
  inputPaths: string[];
  outputDir: string;
  maxConcurrent?: number;
  fakeCliMode?: string;
}): Promise<StartRunResult> {
  return invokeCommand<StartRunResult>("start_run", {
    inputPaths: input.inputPaths,
    outputDir: input.outputDir,
    maxConcurrent: input.maxConcurrent,
    fakeCliMode: input.fakeCliMode,
  });
}

export async function cancelRun(): Promise<RunStatusSnapshot> {
  return invokeCommand<RunStatusSnapshot>("cancel_run");
}

export async function getRunStatus(): Promise<RunStatusSnapshot> {
  return invokeCommand<RunStatusSnapshot>("get_run_status");
}

export async function runIntakePreflight(
  paths: string[],
): Promise<PreflightReport> {
  return invokeCommand<PreflightReport>("run_intake_preflight", { paths });
}

export async function confirmIntakeTrust(
  intakeId: string,
  acknowledgedUntrusted: boolean,
  acknowledgedExecutionRisk: boolean,
): Promise<PreflightReport> {
  return invokeCommand<PreflightReport>("confirm_intake_trust", {
    intakeId,
    acknowledgedUntrusted,
    acknowledgedExecutionRisk,
  });
}

export async function getIntakePreflight(): Promise<PreflightReport | null> {
  return invokeCommand<PreflightReport | null>("get_intake_preflight");
}

export async function clearIntakePreflight(): Promise<void> {
  await invokeCommand<void>("clear_intake_preflight");
}

export async function probeCursorCapability(): Promise<CursorCapabilityReport> {
  return invokeCommand<CursorCapabilityReport>("probe_cursor_capability");
}

export async function getCursorCapability(): Promise<CursorCapabilityReport> {
  return invokeCommand<CursorCapabilityReport>("get_cursor_capability");
}

export async function listCursorModels(): Promise<CursorModelsReport> {
  return invokeCommand<CursorModelsReport>("list_cursor_models");
}

export async function previewCursorCommand(input: {
  workspace: string;
  prompt: string;
  model?: string;
  resumeChatId?: string;
  force?: boolean;
  trust?: boolean;
  planMode?: boolean;
  apiKey?: string;
  timeoutMs?: number;
}): Promise<CursorCommandPreview> {
  return invokeCommand<CursorCommandPreview>("preview_cursor_command", {
    args: {
      workspace: input.workspace,
      prompt: input.prompt,
      model: input.model,
      resumeChatId: input.resumeChatId,
      force: input.force,
      trust: input.trust,
      planMode: input.planMode,
      apiKey: input.apiKey,
      timeoutMs: input.timeoutMs,
    },
  });
}

export async function materializeWorkspace(
  runId: string,
  createInternalWorktrees = true,
): Promise<RunWorkspaceManifest> {
  return invokeCommand<RunWorkspaceManifest>("materialize_workspace", {
    runId,
    createInternalWorktrees,
  });
}

export async function getWorkspaceManifest(): Promise<RunWorkspaceManifest | null> {
  return invokeCommand<RunWorkspaceManifest | null>("get_workspace_manifest");
}

export async function validateWorkspaceRoots(
  writeRoots: string[],
  readRoots: string[],
): Promise<RootValidationResult> {
  return invokeCommand<RootValidationResult>("validate_workspace_roots", {
    writeRoots,
    readRoots,
  });
}

export async function createWorkspaceCheckpoint(
  projectId: string,
  message: string,
): Promise<RunWorkspaceManifest> {
  return invokeCommand<RunWorkspaceManifest>("create_workspace_checkpoint", {
    projectId,
    message,
  });
}

export async function exportWorkspaceProject(
  projectId: string,
  exportDir?: string | null,
): Promise<RunWorkspaceManifest> {
  return invokeCommand<RunWorkspaceManifest>("export_workspace_project", {
    projectId,
    exportDir: exportDir ?? null,
  });
}

export async function promoteWorkspace(
  notes?: string | null,
): Promise<RunWorkspaceManifest> {
  return invokeCommand<RunWorkspaceManifest>("promote_workspace", {
    notes: notes ?? null,
  });
}

export async function runArchitectPipeline(
  runId: string,
): Promise<import("../../domain/plan").ArchitectRunResult> {
  return invokeCommand("run_architect_pipeline", { runId });
}

export async function getProjectPlan(): Promise<
  import("../../domain/contracts").ProjectPlan | null
> {
  return invokeCommand("get_project_plan");
}

export async function getGraphProjection(): Promise<
  import("../../domain/plan").GraphProjection | null
> {
  return invokeCommand("get_graph_projection");
}

export async function getArchitectResult(): Promise<
  import("../../domain/plan").ArchitectRunResult | null
> {
  return invokeCommand("get_architect_result");
}

export async function startScheduler(
  runId: string,
  maxConcurrent?: number,
): Promise<import("../../domain/scheduler").SchedulerSnapshot> {
  return invokeCommand("start_scheduler", { runId, maxConcurrent });
}

export async function schedulerTick(
  runId: string,
): Promise<import("../../domain/scheduler").TickResult> {
  return invokeCommand("scheduler_tick", { runId });
}

export async function schedulerCompleteAttempt(
  attemptId: string,
  success: boolean,
  failureKind?: string,
  progressUseful?: boolean,
): Promise<import("../../domain/scheduler").SchedulerPhaseView> {
  return invokeCommand("scheduler_complete_attempt", {
    attemptId,
    success,
    failureKind,
    progressUseful,
  });
}

export async function schedulerPause(
  runId: string,
): Promise<import("../../domain/scheduler").SchedulerSnapshot> {
  return invokeCommand("scheduler_pause", { runId });
}

export async function schedulerResume(
  runId: string,
): Promise<import("../../domain/scheduler").SchedulerSnapshot> {
  return invokeCommand("scheduler_resume", { runId });
}

export async function getSchedulerSnapshot(
  runId?: string,
): Promise<import("../../domain/scheduler").SchedulerSnapshot | null> {
  return invokeCommand("get_scheduler_snapshot", { runId });
}

export async function emergencyAbort(
  runId?: string,
  force = false,
): Promise<import("../../domain/process").AbortPressResult> {
  return invokeCommand("emergency_abort", { runId, force });
}

export async function getProcessRegistry(
  runId?: string,
): Promise<import("../../domain/process").ProcessRegistrySnapshot> {
  return invokeCommand("get_process_registry", { runId });
}

export async function getAbortSettings(): Promise<
  import("../../domain/process").AbortSettings
> {
  return invokeCommand("get_abort_settings");
}

export async function acknowledgeDegradedAbort(): Promise<
  import("../../domain/process").AbortSettings
> {
  return invokeCommand("acknowledge_degraded_abort");
}

export async function rebindAbortShortcut(
  shortcut: string,
): Promise<import("../../domain/process").AbortSettings> {
  return invokeCommand("rebind_abort_shortcut", { shortcut });
}

export async function applyClosePolicy(
  choice: "keep_running" | "stop_all_and_exit",
  runId?: string,
): Promise<import("../../domain/process").AbortPressResult> {
  return invokeCommand("apply_close_policy", { runId, choice });
}

export async function reconcileProcesses(): Promise<{
  inspected: number;
  terminated: number;
  alreadyGone: number;
  unverifiable: number;
  interruptedAttempts: number;
  hardFailure: boolean;
  messages: string[];
}> {
  return invokeCommand("reconcile_processes");
}

export async function runProcessFixture(input: {
  runId: string;
  mode: string;
  warnAfterMs?: number;
  gracefulAfterMs?: number;
  forceGraceMs?: number;
}): Promise<import("../../domain/process").HostedProcessOutcome> {
  return invokeCommand("run_process_fixture", {
    runId: input.runId,
    mode: input.mode,
    warnAfterMs: input.warnAfterMs,
    gracefulAfterMs: input.gracefulAfterMs,
    forceGraceMs: input.forceGraceMs,
  });
}

export async function executePhaseFixture(
  runId: string,
  phaseId = "P01",
  mode = "impl_success",
): Promise<import("../../domain/executor").PhaseExecutionOutcome> {
  return invokeCommand("execute_phase_fixture", { runId, phaseId, mode });
}

export async function getExecutorOutcome(): Promise<
  import("../../domain/executor").PhaseExecutionOutcome | null
> {
  return invokeCommand("get_executor_outcome");
}

export interface SeedPerfResult {
  runId: string;
  seeded: number;
  totalEvents: number;
  firstSequence: number;
  lastSequence: number;
}

export interface BurstResult {
  runId: string;
  emitted: number;
  events: EventEnvelope[];
  elapsedMs: number;
}

export interface ExportReportResult {
  runId: string;
  reportJson: string;
  artifactId?: string | null;
  relativePath?: string | null;
}

export interface OpenOutputResult {
  path: string;
  opened: boolean;
  message: string;
}

export async function seedPerfEvents(
  runId: string,
  count: number,
): Promise<SeedPerfResult> {
  return invokeCommand("seed_perf_events", { runId, count });
}

export async function emitEventBurst(
  runId: string,
  count: number,
): Promise<BurstResult> {
  return invokeCommand("emit_event_burst", { runId, count });
}

export async function exportRunReport(
  runId: string,
): Promise<ExportReportResult> {
  return invokeCommand("export_run_report", { runId });
}

export async function schedulerRetryPhase(
  runId: string,
  phaseId?: string,
): Promise<import("../../domain/scheduler").SchedulerSnapshot> {
  return invokeCommand("scheduler_retry_phase", { runId, phaseId });
}

export async function openRunOutput(): Promise<OpenOutputResult> {
  return invokeCommand("open_run_output");
}

export async function runStartupRecovery(): Promise<
  import("../../domain/recovery").RecoveryScanReport
> {
  return invokeCommand("run_startup_recovery");
}

export async function getRecoveryOffer(
  runId?: string,
): Promise<import("../../domain/recovery").RecoveryOffer | null> {
  return invokeCommand("get_recovery_offer", { runId });
}

export async function recoveryResume(
  runId: string,
): Promise<import("../../domain/recovery").RecoveryOffer> {
  return invokeCommand("recovery_resume", { runId });
}

export async function recoveryCancel(
  runId: string,
): Promise<import("../../domain/recovery").RecoveryOffer> {
  return invokeCommand("recovery_cancel", { runId });
}

export async function redactText(text: string): Promise<{
  text: string;
  originalBytes: number;
  redactedBytes: number;
  contentHash: string;
  replacementCount: number;
}> {
  return invokeCommand("redact_text", { text });
}

export async function scanPromptInjection(text: string): Promise<{
  suspicious: boolean;
  markers: string[];
  message: string;
}> {
  return invokeCommand("scan_prompt_injection", { text });
}

export async function applyOutputLimitsFixture(
  text: string,
  maxLineBytes?: number,
  maxTotalBytes?: number,
): Promise<{
  text: string;
  truncated: boolean;
  originalBytes: number;
  keptBytes: number;
  linesDropped: number;
  floodDetected: boolean;
  message?: string | null;
}> {
  return invokeCommand("apply_output_limits_fixture", {
    text,
    maxLineBytes,
    maxTotalBytes,
  });
}

export interface AppSettings {
  cursorCliPath?: string | null;
  canaryCapabilityHash?: string | null;
  canaryConsentedAtUtc?: string | null;
  canaryLastSuccessAtUtc?: string | null;
  canaryLastVersion?: string | null;
  updatedAtUtc: string;
}

export async function getAppSettings(): Promise<AppSettings> {
  return invokeCommand("get_app_settings");
}

export async function setCursorCliPath(
  path: string | null,
): Promise<AppSettings> {
  return invokeCommand("set_cursor_cli_path", { path });
}

export async function planUninstallRetention(): Promise<{
  removeProgramFiles: boolean;
  removeStartMenuShortcuts: boolean;
  removeAppDataDb: boolean;
  removeManagedWorkspaces: boolean;
  retainUnpromotedWorkspaces: boolean;
  retainedPaths: string[];
  warnings: string[];
}> {
  return invokeCommand("plan_uninstall_retention");
}

export async function simulateUpgradePreserve(
  appDataRoot: string,
  previousVersion: string,
  nextVersion: string,
): Promise<{
  dbPreserved: boolean;
  settingsPreserved: boolean;
  workspacesPreserved: boolean;
  previousVersion: string;
  nextVersion: string;
  messages: string[];
}> {
  return invokeCommand("simulate_upgrade_preserve", {
    appDataRoot,
    previousVersion,
    nextVersion,
  });
}

export async function createLongPathFixture(root: string): Promise<string> {
  return invokeCommand("create_long_path_fixture", { root });
}

export async function provePackagedCleanup(
  runId: string,
  outDir: string,
): Promise<{
  runId: string;
  activeProcessCount: number;
  zeroOwnedProcesses: boolean;
  proofs: unknown[];
  artifactPath?: string | null;
}> {
  return invokeCommand("prove_packaged_cleanup", { runId, outDir });
}

export async function materializeTestbench(dest: string): Promise<{
  destination: string;
  longPathMarker: string;
  cases: string[];
}> {
  return invokeCommand("materialize_testbench", { dest });
}
