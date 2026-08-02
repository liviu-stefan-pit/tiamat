import { useCallback, useEffect, useState } from "react";
import type { EventEnvelope } from "../../domain/contracts";
import {
  cliConnectionState,
  type CursorCapabilityReport,
} from "../../domain/cursor";
import type { PreflightReport } from "../../domain/intake";
import { canStartImplementation } from "../../domain/intake";
import { connectEventBridge } from "../../lib/tauri/bridge";
import {
  cancelRun,
  ensureDemoRun,
  getRunStatus,
  probeCursorCapability,
  replayEvents,
  startRun,
  type RunStatusSnapshot,
} from "../../lib/tauri/commands";
import { ActivityLog } from "../activity-log/ActivityLog";
import { IntakePanel } from "../intake/IntakePanel";
import { OutputPanel } from "../output/OutputPanel";
import { CliStatusLight } from "./CliStatusLight";
import "./Shell.css";

const CLI_PROBE_INTERVAL_MS = 30_000;

const TERMINAL_RUN_STATUSES = new Set([
  "idle",
  "completed",
  "failed",
  "cancelled",
  "interrupted",
]);

function isStoppableStatus(status: string | undefined): boolean {
  if (!status) return false;
  return !TERMINAL_RUN_STATUSES.has(status);
}

export function Shell() {
  const [events, setEvents] = useState<EventEnvelope[]>([]);
  const [preflight, setPreflight] = useState<PreflightReport | null>(null);
  const [inputPaths, setInputPaths] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState("");
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [runStatus, setRunStatus] = useState<RunStatusSnapshot | null>(null);
  const [runId, setRunId] = useState<string | null>(null);
  const [capability, setCapability] = useState<CursorCapabilityReport | null>(
    null,
  );
  const [cliProbing, setCliProbing] = useState(false);

  const refreshCli = useCallback(async () => {
    setCliProbing(true);
    try {
      const report = await probeCursorCapability();
      setCapability(report);
    } catch {
      setCapability(null);
    } finally {
      setCliProbing(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let disconnect: (() => void) | undefined;

    (async () => {
      try {
        const demo = await ensureDemoRun();
        if (cancelled) return;
        setRunId(demo.run.runId);
        disconnect = await connectEventBridge((batch) => {
          setEvents((prev) => {
            const seen = new Set(prev.map((e) => e.eventId));
            const merged = [...prev];
            for (const event of batch) {
              if (!seen.has(event.eventId)) {
                seen.add(event.eventId);
                merged.push(event);
              }
            }
            return merged;
          });
        });
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      }
    })();

    return () => {
      cancelled = true;
      disconnect?.();
    };
  }, []);

  useEffect(() => {
    void refreshCli();
    const id = window.setInterval(() => {
      void refreshCli();
    }, CLI_PROBE_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [refreshCli]);

  useEffect(() => {
    if (!runId) return;
    const id = window.setInterval(() => {
      void getRunStatus()
        .then((status) => setRunStatus(status))
        .catch(() => undefined);
    }, 1000);
    return () => window.clearInterval(id);
  }, [runId]);

  const onStart = useCallback(async () => {
    if (!canStartImplementation(preflight)) {
      setError("Intake is not ready. Fix blockers and acknowledge trust.");
      return;
    }
    if (!outputDir.trim()) {
      setError("Choose an output folder first.");
      return;
    }
    if (inputPaths.length === 0) {
      setError("Select at least one input path.");
      return;
    }
    setStarting(true);
    setError(null);
    // Optimistic: Stop must work immediately, even before start_run returns.
    setRunStatus((prev) => ({
      runId: prev?.runId ?? null,
      status: "starting",
      message: "Starting run…",
      activeAttempts: prev?.activeAttempts ?? 0,
      completedPhases: prev?.completedPhases ?? 0,
      totalPhases: prev?.totalPhases ?? 0,
      managedRunRoot: prev?.managedRunRoot ?? null,
    }));
    try {
      const result = await startRun({
        inputPaths,
        outputDir: outputDir.trim(),
      });
      setRunId(result.runId);
      setRunStatus({
        runId: result.runId,
        status: result.status,
        message: result.message,
        activeAttempts: 0,
        completedPhases: 0,
        totalPhases: 0,
        managedRunRoot: result.managedRunRoot,
      });
      const replayed = await replayEvents(result.runId, 0);
      setEvents(replayed);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setRunStatus((prev) =>
        prev
          ? { ...prev, status: "failed", message: "Start failed" }
          : prev,
      );
    } finally {
      setStarting(false);
    }
  }, [preflight, outputDir, inputPaths]);

  const onStop = useCallback(async () => {
    setStopping(true);
    setError(null);
    try {
      const status = await cancelRun();
      setRunStatus(status);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setStopping(false);
    }
  }, []);

  const statusLine = runStatus
    ? [
        runStatus.status,
        runStatus.phase ? `phase ${runStatus.phase}` : null,
        runStatus.totalPhases
          ? `${runStatus.completedPhases}/${runStatus.totalPhases} phases`
          : null,
        runStatus.activeAttempts
          ? `${runStatus.activeAttempts} active`
          : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : "Idle";

  const canRun =
    canStartImplementation(preflight) &&
    outputDir.trim().length > 0 &&
    inputPaths.length > 0 &&
    !starting &&
    !stopping &&
    !isStoppableStatus(runStatus?.status);

  // Stop stays available while starting or while a run is active — never locked by Run.
  const canStop =
    !stopping && (starting || isStoppableStatus(runStatus?.status));

  const cliState = cliConnectionState(capability, cliProbing);

  return (
    <div className="tiamat-shell" data-testid="tiamat-shell">
      <header className="shell-header">
        <div className="shell-header-copy">
          <h1>Tiamat</h1>
          <p className="shell-tagline">
            Pick input, pick output, watch the agents work.
          </p>
        </div>
        <CliStatusLight
          state={cliState}
          probing={cliProbing}
          onRefresh={() => void refreshCli()}
        />
      </header>

      <div className="shell-grid">
        <IntakePanel
          report={preflight}
          onReportChange={setPreflight}
          selectedPaths={inputPaths}
          onPathsChange={setInputPaths}
        />
        <OutputPanel outputDir={outputDir} onOutputDirChange={setOutputDir} />
      </div>

      <div className="run-bar" data-testid="run-bar">
        <button
          type="button"
          data-testid="start-run"
          disabled={!canRun}
          onClick={() => void onStart()}
        >
          {starting ? "Starting…" : "Run"}
        </button>
        <button
          type="button"
          data-testid="stop-run"
          disabled={!canStop}
          onClick={() => void onStop()}
        >
          {stopping ? "Stopping…" : "Stop"}
        </button>
        {error && (
          <p className="error" data-testid="shell-error">
            {error}
          </p>
        )}
      </div>

      <ActivityLog events={events} statusLine={statusLine} />
    </div>
  );
}
