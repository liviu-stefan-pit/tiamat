import { useCallback, useEffect, useState } from "react";
import type { EventEnvelope } from "../../domain/contracts";
import type { PreflightReport } from "../../domain/intake";
import { canStartImplementation } from "../../domain/intake";
import { connectEventBridge } from "../../lib/tauri/bridge";
import {
  cancelRun,
  ensureDemoRun,
  getRunStatus,
  replayEvents,
  startRun,
  type RunStatusSnapshot,
} from "../../lib/tauri/commands";
import { ActivityLog } from "../activity-log/ActivityLog";
import { IntakePanel } from "../intake/IntakePanel";
import { OutputPanel } from "../output/OutputPanel";
import "./Shell.css";

export function Shell() {
  const [events, setEvents] = useState<EventEnvelope[]>([]);
  const [preflight, setPreflight] = useState<PreflightReport | null>(null);
  const [inputPaths, setInputPaths] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [runStatus, setRunStatus] = useState<RunStatusSnapshot | null>(null);
  const [runId, setRunId] = useState<string | null>(null);

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
    setBusy(true);
    setError(null);
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
    } finally {
      setBusy(false);
    }
  }, [preflight, outputDir, inputPaths]);

  const onStop = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const status = await cancelRun();
      setRunStatus(status);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
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
    !busy;

  return (
    <div className="tiamat-shell" data-testid="tiamat-shell">
      <header className="shell-header">
        <h1>Tiamat</h1>
        <p className="shell-tagline">
          Pick input, pick output, watch the agents work.
        </p>
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
          {busy ? "Working…" : "Run"}
        </button>
        <button
          type="button"
          data-testid="stop-run"
          disabled={busy || !runStatus || runStatus.status === "idle"}
          onClick={() => void onStop()}
        >
          Stop
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
