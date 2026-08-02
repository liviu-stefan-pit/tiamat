import { useEffect, useMemo, useRef, useState } from "react";
import type { EventEnvelope, ProjectPlan } from "../../domain/contracts";
import type { CursorCapabilityReport } from "../../domain/cursor";
import { formatCursorStatus } from "../../domain/cursor";
import type { PreflightReport } from "../../domain/intake";
import type { RunWorkspaceManifest } from "../../domain/workspace";
import {
  DEFAULT_EVENT_FILTER,
  type EventFilter,
} from "../../domain/events";
import { buildCompletionSummary } from "../../domain/reports";
import { connectEventBridge, mergeEvents } from "../../lib/tauri/bridge";
import type { RecoveryOffer } from "../../domain/recovery";
import {
  acknowledgeDegradedAbort,
  applyClosePolicy,
  emergencyAbort,
  ensureDemoRun,
  executePhaseFixture,
  exportRunReport,
  getAbortSettings,
  getCursorCapability,
  getGraphProjection,
  getProcessRegistry,
  getProjectPlan,
  getSchedulerSnapshot,
  materializeWorkspace,
  openRunOutput,
  promoteWorkspace,
  exportWorkspaceProject,
  recoveryCancel,
  recoveryResume,
  replayEvents,
  runArchitectPipeline,
  runProcessFixture,
  runStartupRecovery,
  schedulerCompleteAttempt,
  schedulerPause,
  schedulerResume,
  schedulerRetryPhase,
  schedulerTick,
  startScheduler,
  transitionRunStatus,
  type RunRecord,
} from "../../lib/tauri/commands";
import { RecoveryOfferBanner } from "../recovery/RecoveryOffer";
import type { AbortSettings } from "../../domain/process";
import { formatAbortStatus } from "../../domain/process";
import type { ArchitectRunResult, GraphProjection } from "../../domain/plan";
import { projectGraphFromPlan } from "../../domain/plan";
import type { SchedulerSnapshot } from "../../domain/scheduler";
import type { PhaseExecutionOutcome } from "../../domain/executor";
import { checkpointAfterAllGates } from "../../domain/executor";
import { ActivityLog } from "../activity-log/ActivityLog";
import { IntakePanel } from "../intake/IntakePanel";
import { GraphPanel } from "../run-graph/GraphPanel";
import { NodeDetailPanel } from "../run-graph/NodeDetailPanel";
import { RunControls } from "../run-controls/RunControls";
import { AttemptTimeline } from "../reports/AttemptTimeline";
import { EvidencePanel } from "../reports/EvidencePanel";
import { CompletionSummaryPanel } from "../reports/CompletionSummary";
import { SettingsPanel } from "../settings/SettingsPanel";
import { WorkspacePanel } from "../workspace/WorkspacePanel";
import "./Shell.css";

export function Shell() {
  const [events, setEvents] = useState<EventEnvelope[]>([]);
  const [run, setRun] = useState<RunRecord | null>(null);
  const [filter, setFilter] = useState<EventFilter>(DEFAULT_EVENT_FILTER);
  const [cursorStatus, setCursorStatus] = useState("checking…");
  const [capability, setCapability] = useState<CursorCapabilityReport | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preflight, setPreflight] = useState<PreflightReport | null>(null);
  const [workspace, setWorkspace] = useState<RunWorkspaceManifest | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [graph, setGraph] = useState<GraphProjection | null>(null);
  const [plan, setPlan] = useState<ProjectPlan | null>(null);
  const [architect, setArchitect] = useState<ArchitectRunResult | null>(null);
  const [scheduler, setScheduler] = useState<SchedulerSnapshot | null>(null);
  const [executor, setExecutor] = useState<PhaseExecutionOutcome | null>(null);
  const [abortSettings, setAbortSettings] = useState<AbortSettings | null>(null);
  const [abortMessage, setAbortMessage] = useState<string | null>(null);
  const [closePolicyOpen, setClosePolicyOpen] = useState(false);
  const [selectedPhaseId, setSelectedPhaseId] = useState<string | null>(null);
  const [processRegistryEmpty, setProcessRegistryEmpty] = useState(true);
  const [exportMessage, setExportMessage] = useState<string | null>(null);
  const [recoveryOffer, setRecoveryOffer] = useState<RecoveryOffer | null>(
    null,
  );
  const runRef = useRef<RunRecord | null>(null);
  runRef.current = run;

  useEffect(() => {
    async function refreshFromStore() {
      const current = runRef.current;
      if (!current) return;
      const replayed = await replayEvents(current.runId, 0);
      setEvents(replayed);
      const graphProjection = await getGraphProjection();
      if (graphProjection) setGraph(graphProjection);
    }
    (
      window as unknown as { __tiamatRefreshEvents?: () => Promise<void> }
    ).__tiamatRefreshEvents = refreshFromStore;
    return () => {
      delete (window as unknown as { __tiamatRefreshEvents?: () => Promise<void> })
        .__tiamatRefreshEvents;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;

    async function boot() {
      try {
        const cursor = await getCursorCapability();
        if (!cancelled) {
          setCapability(cursor);
          setCursorStatus(formatCursorStatus(cursor));
        }
        const abort = await getAbortSettings();
        if (!cancelled) {
          setAbortSettings(abort);
        }
        const demo = await ensureDemoRun();
        if (!cancelled) {
          setRun(demo.run);
        }
        const recovery = await runStartupRecovery();
        if (!cancelled) {
          setRecoveryOffer(recovery.offer ?? null);
          if (recovery.offer?.runId) {
            const replayed = await replayEvents(recovery.offer.runId, 0);
            setEvents(replayed);
          }
        }
        unsubscribe = await connectEventBridge((incoming) => {
          if (cancelled) return;
          setEvents((prev) => mergeEvents(prev, incoming));
        });
        const existingPlan = await getProjectPlan();
        const graphProjection = await getGraphProjection();
        if (!cancelled) {
          if (existingPlan) setPlan(existingPlan);
          if (graphProjection) setGraph(graphProjection);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setCursorStatus("error");
        }
      }
    }

    void boot();
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, []);

  async function applyStatus(next: string, message: string) {
    if (!run) return;
    setBusy(true);
    setError(null);
    try {
      if (next === "paused") {
        const snap = await schedulerPause(run.runId);
        setScheduler(snap);
        setRun({ ...run, status: "paused" });
        const graphProjection = await getGraphProjection();
        if (graphProjection) setGraph(graphProjection);
        return;
      }
      if (next === "executing") {
        const snap = await schedulerResume(run.runId);
        setScheduler(snap);
        setRun({ ...run, status: "executing" });
        const graphProjection = await getGraphProjection();
        if (graphProjection) setGraph(graphProjection);
        return;
      }
      const result = await transitionRunStatus(run.runId, next, message);
      setRun(result.run);
      setEvents((prev) => mergeEvents(prev, [result.event]));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onEmergencyAbort(force = false) {
    setBusy(true);
    setError(null);
    try {
      const current = runRef.current;
      const result = await emergencyAbort(current?.runId, force);
      setAbortMessage(result.message);
      if (
        current &&
        (result.forced ||
          result.action === "begin_emergency_cancel" ||
          result.action === "force_terminate")
      ) {
        setRun({ ...current, status: "cancelled" });
      }
      const registry = await getProcessRegistry(current?.runId);
      setProcessRegistryEmpty((registry?.activeCount ?? 0) === 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onCancelRun() {
    await onEmergencyAbort(false);
  }

  async function onAcknowledgeDegraded() {
    const settings = await acknowledgeDegradedAbort();
    setAbortSettings(settings);
  }

  async function onClosePolicy(choice: "keep_running" | "stop_all_and_exit") {
    const result = await applyClosePolicy(choice, run?.runId);
    setAbortMessage(result.message);
    setClosePolicyOpen(false);
    if (choice === "stop_all_and_exit" && run) {
      setRun({ ...run, status: "cancelled" });
    }
  }

  async function onRetryFailedPhase() {
    if (!run) return;
    setBusy(true);
    setError(null);
    try {
      const snap = await schedulerRetryPhase(run.runId, selectedPhaseId ?? undefined);
      setScheduler(snap);
      await schedulerTick(run.runId);
      await refreshSchedulerGraph(run.runId);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onOpenOutput() {
    setBusy(true);
    setError(null);
    try {
      const result = await openRunOutput();
      setExportMessage(result.message);
      setAbortMessage(`Output: ${result.path}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onExportReport() {
    if (!run) return;
    try {
      const result = await exportRunReport(run.runId);
      const blob = new Blob([result.reportJson], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `tiamat-report-${run.runId.slice(0, 8)}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
      setExportMessage(
        `Exported report${result.relativePath ? ` (${result.relativePath})` : ""}`,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function onExportWorkspaceProject(projectId: string) {
    setBusy(true);
    setError(null);
    try {
      // Browser/fake mode: fixed export dir under managed root (no native folder picker).
      const exportDir = workspace
        ? `${workspace.managedRunRoot}\\exports`
        : undefined;
      const manifest = await exportWorkspaceProject(projectId, exportDir);
      setWorkspace(manifest);
      setExportMessage(
        `Exported ${projectId}${
          manifest.promotion.exportPath
            ? ` → ${manifest.promotion.exportPath}`
            : ""
        }`,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onPromoteWorkspace() {
    setBusy(true);
    setError(null);
    try {
      const manifest = await promoteWorkspace(
        "Accepted managed output for external merge",
      );
      setWorkspace(manifest);
      setExportMessage(`Promotion status: ${manifest.promotion.status}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.ctrlKey && event.shiftKey && event.key === "F12") {
        event.preventDefault();
        setBusy(true);
        setError(null);
        const current = runRef.current;
        void emergencyAbort(current?.runId, false)
          .then((result) => {
            setAbortMessage(result.message);
            if (
              current &&
              (result.forced ||
                result.action === "begin_emergency_cancel" ||
                result.action === "force_terminate")
            ) {
              setRun({ ...current, status: "cancelled" });
            }
          })
          .catch((err) => {
            setError(err instanceof Error ? err.message : String(err));
          })
          .finally(() => setBusy(false));
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const abortCanStart =
    !abortSettings ||
    !abortSettings.degraded ||
    abortSettings.degradedAcknowledged;
  const startReady =
    Boolean(preflight?.canStart) &&
    abortCanStart &&
    !recoveryOffer?.requiresUserChoice;

  const canRetry = Boolean(
    scheduler?.phases.some((phase) =>
      ["failed", "blocked", "needs_review"].includes(phase.status),
    ),
  );
  const canOpenOutput = Boolean(workspace?.managedRunRoot);

  const completion = useMemo(() => {
    if (!run) return null;
    const terminal = ["completed", "failed", "cancelled"].includes(run.status);
    if (!terminal && !scheduler) return null;
    return buildCompletionSummary({
      runId: run.runId,
      runStatus: run.status,
      plan,
      scheduler,
      executor,
      workspace,
      events,
      processRegistryEmpty,
    });
  }, [run, plan, scheduler, executor, workspace, events, processRegistryEmpty]);

  async function refreshSchedulerGraph(runId: string) {
    const snap = await getSchedulerSnapshot(runId);
    setScheduler(snap);
    const graphProjection = await getGraphProjection();
    if (graphProjection) {
      setGraph(graphProjection);
    }
    const latestPlan = await getProjectPlan();
    if (latestPlan) setPlan(latestPlan);
  }

  async function onStartImplementation() {
    if (!startReady || !run) return;
    setBusy(true);
    setError(null);
    try {
      const manifest = await materializeWorkspace(run.runId, true);
      setWorkspace(manifest);
      const architectResult = await runArchitectPipeline(run.runId);
      setArchitect(architectResult);
      if (architectResult.plan) {
        setPlan(architectResult.plan);
        setGraph(projectGraphFromPlan(architectResult.plan));
      } else {
        setGraph(null);
      }
      if (!architectResult.ok) {
        throw new Error(
          architectResult.error ?? "Architect planning failed after repair",
        );
      }
      const result = await transitionRunStatus(
        run.runId,
        "executing",
        `Plan compiled (${architectResult.plan?.phases.length ?? 0} phase(s); model=${architectResult.modelSelection.selectedModel}; source unchanged=${manifest.sourceUnchanged})`,
        "plan.compiled",
      );
      setRun(result.run);
      setEvents((prev) => mergeEvents(prev, [result.event]));

      const snap = await startScheduler(run.runId, 2);
      setScheduler(snap);
      await schedulerTick(run.runId);

      const afterStart = await getSchedulerSnapshot(run.runId);
      const sourcePath = (preflight?.manifest.sources[0]?.path ?? "").toLowerCase();
      if (sourcePath.includes("executor")) {
        const outcome = await executePhaseFixture(run.runId, "P01", "impl_success");
        setExecutor(outcome);
        if (outcome.ok) {
          const graphProjection = await getGraphProjection();
          if (graphProjection) setGraph(graphProjection);
        }
      } else if (afterStart && afterStart.phases.length > 1) {
        const p01 = afterStart.attempts.find((a) => a.phaseId === "P01");
        if (p01) {
          await schedulerCompleteAttempt(p01.attemptId, false, "timeout");
          await schedulerTick(run.runId);
        }
        const p01b = (await getSchedulerSnapshot(run.runId))?.attempts
          .filter((a) => a.phaseId === "P01")
          .at(-1);
        if (p01b && p01b.status === "running") {
          await schedulerCompleteAttempt(p01b.attemptId, false, "policy");
        }
        const pausedSnap = await schedulerPause(run.runId);
        setScheduler(pausedSnap);
        setRun((prev) =>
          prev ? { ...prev, status: "paused", updatedAtUtc: new Date().toISOString() } : prev,
        );
      }
      await refreshSchedulerGraph(run.runId);
      const registry = await getProcessRegistry(run.runId);
      setProcessRegistryEmpty((registry?.activeCount ?? 0) === 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="tiamat-shell" data-testid="tiamat-shell">
      <header className="tiamat-header">
        <div>
          <h1>Tiamat</h1>
          <p className="tiamat-subtitle">Desktop implementation orchestrator</p>
        </div>
        <div className="tiamat-header-meta" data-testid="header-meta">
          <span data-testid="cursor-status">Cursor: {cursorStatus}</span>
          <span data-testid="workspace-label">
            Workspace:{" "}
            {workspace
              ? workspace.sourceUnchanged
                ? "isolated (source unchanged)"
                : "isolated (source CHANGED)"
              : run
                ? run.title
                : "none"}
          </span>
          <button
            type="button"
            className="tiamat-linkish"
            data-testid="open-settings"
            onClick={() => setShowSettings((value) => !value)}
          >
            {showSettings ? "Hide settings" : "Settings"}
          </button>
        </div>
      </header>

      {error ? (
        <p className="tiamat-error" role="alert">
          {error}
        </p>
      ) : null}
      {exportMessage ? (
        <p className="tiamat-muted" role="status" data-testid="export-status">
          {exportMessage}
        </p>
      ) : null}

      {recoveryOffer?.requiresUserChoice ? (
        <RecoveryOfferBanner
          offer={recoveryOffer}
          busy={busy}
          onResume={async () => {
            if (!recoveryOffer) return;
            setBusy(true);
            setError(null);
            try {
              const next = await recoveryResume(recoveryOffer.runId);
              setRecoveryOffer(next);
              setRun((prev) =>
                prev
                  ? { ...prev, status: "executing", updatedAtUtc: next.resolvedAtUtc ?? prev.updatedAtUtc }
                  : prev,
              );
              const replayed = await replayEvents(recoveryOffer.runId, 0);
              setEvents(replayed);
            } catch (err) {
              setError(err instanceof Error ? err.message : String(err));
            } finally {
              setBusy(false);
            }
          }}
          onCancel={async () => {
            if (!recoveryOffer) return;
            setBusy(true);
            setError(null);
            try {
              const next = await recoveryCancel(recoveryOffer.runId);
              setRecoveryOffer(next);
              setRun((prev) =>
                prev
                  ? { ...prev, status: "cancelled", updatedAtUtc: next.resolvedAtUtc ?? prev.updatedAtUtc }
                  : prev,
              );
              const replayed = await replayEvents(recoveryOffer.runId, 0);
              setEvents(replayed);
            } catch (err) {
              setError(err instanceof Error ? err.message : String(err));
            } finally {
              setBusy(false);
            }
          }}
        />
      ) : null}

      {/* recovery status probe for tests */}
      <span hidden data-testid="recovery-offer-probe">
        {recoveryOffer?.status ?? "none"}
      </span>

      <div className="tiamat-layout">
        <aside className="tiamat-sidebar">
          <IntakePanel
            report={preflight}
            onReportChange={setPreflight}
            onStart={() => void onStartImplementation()}
            starting={busy}
          />
          <RunControls
            runId={run?.runId ?? null}
            status={run?.status ?? null}
            busy={busy}
            canStart={startReady}
            canRetry={canRetry}
            canOpenOutput={canOpenOutput}
            outputPath={workspace?.managedRunRoot ?? null}
            abortHint={
              abortSettings
                ? formatAbortStatus(abortSettings)
                : "Emergency stop: Ctrl+Shift+F12"
            }
            abortAckVisible={Boolean(
              abortSettings?.degraded && !abortSettings.degradedAcknowledged,
            )}
            lastAbortMessage={abortMessage}
            onPause={() => void applyStatus("paused", "Scheduling paused")}
            onResume={() => void applyStatus("executing", "Scheduling resumed")}
            onCancel={() => void onCancelRun()}
            onEmergencyAbort={() => void onEmergencyAbort(false)}
            onAcknowledgeDegraded={() => void onAcknowledgeDegraded()}
            onRetryFailedPhase={() => void onRetryFailedPhase()}
            onOpenOutput={() => void onOpenOutput()}
          />
          {closePolicyOpen ? (
            <div
              className="tiamat-panel"
              data-testid="close-policy-dialog"
              role="dialog"
              aria-label="Close policy"
            >
              <p>Active work detected. Keep Tiamat running or stop all and exit?</p>
              <div className="tiamat-controls">
                <button
                  type="button"
                  data-testid="keep-running"
                  onClick={() => void onClosePolicy("keep_running")}
                >
                  Keep Tiamat running
                </button>
                <button
                  type="button"
                  className="danger"
                  data-testid="stop-all-exit"
                  onClick={() => void onClosePolicy("stop_all_and_exit")}
                >
                  Stop all and exit
                </button>
              </div>
            </div>
          ) : null}
          <button
            type="button"
            className="tiamat-linkish"
            data-testid="simulate-close-policy"
            onClick={() => setClosePolicyOpen(true)}
            style={{ display: "none" }}
          >
            Simulate close
          </button>
          <button
            type="button"
            className="tiamat-linkish"
            data-testid="run-timeout-fixture"
            style={{ display: "none" }}
            onClick={() => {
              if (!run) return;
              void runProcessFixture({
                runId: run.runId,
                mode: "silent_hang",
                warnAfterMs: 40,
                gracefulAfterMs: 100,
                forceGraceMs: 40,
              }).then((outcome) => {
                setAbortMessage(
                  outcome.resume
                    ? `Timeout resume chat=${outcome.resume.chatId} model=${outcome.resume.nextModel}`
                    : "Fixture complete",
                );
              });
            }}
          >
            Timeout fixture
          </button>
          {scheduler ? (
            <p
              className="tiamat-muted"
              data-testid="scheduler-summary"
              style={{ margin: 0, fontSize: "0.85rem" }}
            >
              Scheduler: {scheduler.mode} · epoch={scheduler.epoch} · active=
              {scheduler.activeAttempts} · paused=
              {scheduler.paused ? "yes" : "no"} · locks=
              {scheduler.heldLocks.length}
            </p>
          ) : null}
          {executor ? (
            <p
              className="tiamat-muted"
              data-testid="executor-summary"
              style={{ margin: 0, fontSize: "0.85rem" }}
            >
              Executor: {executor.ok ? "passed" : "failed"} · gates=
              {executor.layers
                .filter((l) => l.required)
                .map((l) => `${l.kind}:${l.failed === 0 ? "ok" : "fail"}`)
                .join(",")}{" "}
              · checkpoint=
              {executor.projectCheckpoint
                ? checkpointAfterAllGates(executor)
                  ? "ready"
                  : "present"
                : "none"}{" "}
              · boundary={executor.boundaryOk ? "ok" : "escape"} · projected=
              {executor.planProjected ? "yes" : "no"}
            </p>
          ) : null}
          <WorkspacePanel
            manifest={workspace}
            busy={busy}
            onExportProject={(projectId) => void onExportWorkspaceProject(projectId)}
            onPromote={() => void onPromoteWorkspace()}
          />
          {architect ? (
            <p
              className="tiamat-muted"
              data-testid="architect-summary"
              style={{ margin: 0, fontSize: "0.85rem" }}
            >
              Architect: {architect.ok ? "compiled" : "failed"} ·{" "}
              {architect.modelSelection.selectedModel}
              {architect.degradedMode ? " (degraded)" : ""} · planMode=
              {architect.attempts[0]?.proof.planMode ? "yes" : "no"} · force=
              {architect.attempts[0]?.proof.force ? "yes" : "no"}
            </p>
          ) : null}
          {showSettings ? (
            <SettingsPanel
              capability={capability}
              abortSettings={abortSettings}
              onCapabilityChange={(report) => {
                setCapability(report);
                setCursorStatus(formatCursorStatus(report));
              }}
              onAbortSettingsChange={setAbortSettings}
            />
          ) : null}
        </aside>
        <main className="tiamat-main">
          <GraphPanel
            graph={graph}
            selectedPhaseId={selectedPhaseId}
            onSelectPhase={setSelectedPhaseId}
          />
          <div className="tiamat-main-split">
            <NodeDetailPanel
              phaseId={selectedPhaseId}
              plan={plan}
              scheduler={scheduler}
              executor={executor}
            />
            <AttemptTimeline
              scheduler={scheduler}
              selectedPhaseId={selectedPhaseId}
              onSelectPhase={setSelectedPhaseId}
            />
          </div>
          <EvidencePanel executor={executor} />
          <ActivityLog
            events={events}
            filter={filter}
            onFilterChange={setFilter}
            selectedPhaseId={selectedPhaseId}
            onExportReport={() => void onExportReport()}
          />
          <CompletionSummaryPanel
            summary={completion}
            busy={busy}
            defaultProjectId={workspace?.projects[0]?.projectId ?? null}
            onExportProject={(projectId) => void onExportWorkspaceProject(projectId)}
            onPromote={() => void onPromoteWorkspace()}
          />
        </main>
      </div>
    </div>
  );
}
