interface RunControlsProps {
  runId: string | null;
  status: string | null;
  busy: boolean;
  canStart: boolean;
  abortHint: string;
  abortAckVisible: boolean;
  lastAbortMessage: string | null;
  canRetry: boolean;
  canOpenOutput: boolean;
  outputPath: string | null;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onEmergencyAbort: () => void;
  onAcknowledgeDegraded: () => void;
  onRetryFailedPhase: () => void;
  onOpenOutput: () => void;
}

export function RunControls({
  runId,
  status,
  busy,
  canStart,
  abortHint,
  abortAckVisible,
  lastAbortMessage,
  canRetry,
  canOpenOutput,
  outputPath,
  onPause,
  onResume,
  onCancel,
  onEmergencyAbort,
  onAcknowledgeDegraded,
  onRetryFailedPhase,
  onOpenOutput,
}: RunControlsProps) {
  return (
    <section className="tiamat-panel" aria-label="Run controls">
      <h2>Run controls</h2>
      <p className="tiamat-muted" data-testid="run-status">
        {runId
          ? `Run ${runId.slice(0, 8)}… — ${status ?? "unknown"}`
          : "No active run"}
      </p>
      <p className="tiamat-muted" data-testid="start-ready-flag">
        Start ready: {canStart ? "yes" : "no"}
      </p>
      <div className="tiamat-controls" data-testid="run-controls">
        <button type="button" disabled={busy || !runId} onClick={onPause}>
          Pause scheduling
        </button>
        <button type="button" disabled={busy || !runId} onClick={onResume}>
          Resume
        </button>
        <button
          type="button"
          disabled={busy || !canRetry}
          onClick={onRetryFailedPhase}
          data-testid="retry-failed-phase"
        >
          Retry failed phase
        </button>
        <button
          type="button"
          disabled={busy || !canOpenOutput}
          onClick={onOpenOutput}
          data-testid="open-output"
        >
          Open output
        </button>
        <button
          type="button"
          className="danger"
          disabled={busy || !runId}
          onClick={onCancel}
          data-testid="cancel-run"
        >
          Cancel run
        </button>
        <button
          type="button"
          className="danger"
          disabled={busy}
          onClick={onEmergencyAbort}
          data-testid="emergency-abort"
        >
          Emergency stop
        </button>
      </div>
      {outputPath ? (
        <p className="tiamat-muted" data-testid="output-path">
          Output: {outputPath}
        </p>
      ) : null}
      <p className="tiamat-muted" data-testid="emergency-stop-hint">
        {abortHint}
      </p>
      {abortAckVisible ? (
        <button
          type="button"
          data-testid="ack-degraded-abort"
          onClick={onAcknowledgeDegraded}
        >
          Acknowledge degraded global abort
        </button>
      ) : null}
      {lastAbortMessage ? (
        <p data-testid="abort-ack" role="status">
          {lastAbortMessage}
        </p>
      ) : null}
    </section>
  );
}
