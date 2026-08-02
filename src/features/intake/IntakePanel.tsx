import { useState, type DragEvent } from "react";
import type { PreflightReport } from "../../domain/intake";
import {
  confirmIntakeTrust,
  pickIntakePaths,
  runIntakePreflight,
} from "../../lib/tauri/commands";

interface IntakePanelProps {
  report: PreflightReport | null;
  onReportChange: (report: PreflightReport | null) => void;
  selectedPaths: string[];
  onPathsChange: (paths: string[]) => void;
}

export function IntakePanel({
  report,
  onReportChange,
  selectedPaths,
  onPathsChange,
}: IntakePanelProps) {
  const [pathDraft, setPathDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragActive, setDragActive] = useState(false);
  const [trustAck, setTrustAck] = useState(false);

  async function analyze(paths: string[]) {
    if (paths.length === 0) {
      setError("Select at least one file or folder.");
      return;
    }
    setBusy(true);
    setError(null);
    setTrustAck(false);
    try {
      const next = await runIntakePreflight(paths);
      onReportChange(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      onReportChange(null);
    } finally {
      setBusy(false);
    }
  }

  async function onPick(kind: "file" | "folder") {
    setBusy(true);
    setError(null);
    try {
      const picked = await pickIntakePaths(kind);
      if (picked.length === 0) {
        setBusy(false);
        return;
      }
      const next = [...new Set([...selectedPaths, ...picked])];
      onPathsChange(next);
      await analyze(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }

  async function onTrustToggle(checked: boolean) {
    setTrustAck(checked);
    if (!report || !checked) return;
    setBusy(true);
    try {
      const next = await confirmIntakeTrust(report.manifest.intakeId, true, true);
      onReportChange(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  function onDrop(event: DragEvent) {
    event.preventDefault();
    setDragActive(false);
    const files = Array.from(event.dataTransfer.files);
    const paths = files
      .map((f) => {
        const withPath = f as File & { path?: string };
        return withPath.path || f.name;
      })
      .filter(Boolean);
    if (paths.length === 0) return;
    const next = [...new Set([...selectedPaths, ...paths])];
    onPathsChange(next);
    void analyze(next);
  }

  const needsTrust =
    !!report &&
    (!report.trust.acknowledgedUntrusted ||
      !report.trust.acknowledgedExecutionRisk);
  const hasBlockers = (report?.blockers.length ?? 0) > 0;
  const trustConfirmed = Boolean(report?.trust.confirmed);
  // Keep the control mounted after ack so the checked state stays visible (and so
  // automation is not left asserting against a detached checkbox).
  const showTrust = !!report && !hasBlockers && (needsTrust || trustConfirmed);

  return (
    <section className="pane" data-testid="intake-panel">
      <header className="pane-header">
        <h2>Input</h2>
        <p>Select the files or folders the project should be built from.</p>
      </header>

      <div
        className={`drop-zone${dragActive ? " active" : ""}`}
        data-testid="intake-drop-zone"
        onDragOver={(e) => {
          e.preventDefault();
          setDragActive(true);
        }}
        onDragLeave={() => setDragActive(false)}
        onDrop={onDrop}
      >
        Drop files or folders here
      </div>

      <div className="button-row">
        <button
          type="button"
          data-testid="pick-files"
          disabled={busy}
          onClick={() => void onPick("file")}
        >
          Pick files
        </button>
        <button
          type="button"
          data-testid="pick-folder"
          disabled={busy}
          onClick={() => void onPick("folder")}
        >
          Pick folder
        </button>
      </div>

      <form
        className="path-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (!pathDraft.trim()) return;
          const next = [...new Set([...selectedPaths, pathDraft.trim()])];
          onPathsChange(next);
          setPathDraft("");
          void analyze(next);
        }}
      >
        <input
          data-testid="intake-path-input"
          value={pathDraft}
          onChange={(e) => setPathDraft(e.target.value)}
          placeholder="Or paste a path and press Enter"
          disabled={busy}
        />
        <button type="submit" disabled={busy || !pathDraft.trim()}>
          Add
        </button>
      </form>

      {selectedPaths.length > 0 && (
        <ul className="path-list" data-testid="intake-paths">
          {selectedPaths.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}

      {error && (
        <p className="error" data-testid="intake-error">
          {error}
        </p>
      )}

      {report && (
        <div className="preflight-summary" data-testid="preflight-summary">
          <p>
            {report.manifest.projects.length} project(s) ·{" "}
            {report.canStart ? "ready" : "needs attention"}
          </p>
          {hasBlockers && (
            <ul className="blockers">
              {report.blockers.map((b) => (
                <li key={b}>{b}</li>
              ))}
            </ul>
          )}
          {showTrust && (
            <label className="trust-ack" data-testid="trust-ack">
              <input
                type="checkbox"
                checked={trustConfirmed || trustAck}
                onChange={(e) => void onTrustToggle(e.target.checked)}
                disabled={busy || trustConfirmed}
              />
              I understand these sources will be read and agents will run
              commands in the output folder.
            </label>
          )}
        </div>
      )}
    </section>
  );
}
