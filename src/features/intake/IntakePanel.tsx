import { useCallback, useState, type DragEvent } from "react";
import type { PreflightReport } from "../../domain/intake";
import { canStartImplementation } from "../../domain/intake";
import {
  confirmIntakeTrust,
  pickIntakePaths,
  runIntakePreflight,
} from "../../lib/tauri/commands";
import { PreflightCard } from "../preflight/PreflightCard";

interface IntakePanelProps {
  report: PreflightReport | null;
  onReportChange: (report: PreflightReport | null) => void;
  onStart: () => void;
  starting?: boolean;
}

export function IntakePanel({
  report,
  onReportChange,
  onStart,
  starting = false,
}: IntakePanelProps) {
  const [pathDraft, setPathDraft] = useState("");
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragActive, setDragActive] = useState(false);

  const mergePaths = useCallback((incoming: string[]) => {
    setSelectedPaths((prev) => {
      const set = new Set(prev);
      for (const path of incoming) {
        const trimmed = path.trim();
        if (trimmed) set.add(trimmed);
      }
      return [...set];
    });
  }, []);

  async function analyze(paths: string[]) {
    if (paths.length === 0) {
      setError("Select at least one file or folder.");
      return;
    }
    setBusy(true);
    setError(null);
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
      mergePaths(picked);
      await analyze([...new Set([...selectedPaths, ...picked])]);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }

  async function onTrustChange(
    acknowledgedUntrusted: boolean,
    acknowledgedExecutionRisk: boolean,
  ) {
    if (!report) return;
    setBusy(true);
    setError(null);
    try {
      const next = await confirmIntakeTrust(
        report.manifest.intakeId,
        acknowledgedUntrusted,
        acknowledgedExecutionRisk,
      );
      onReportChange(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragActive(false);
    const files = [...event.dataTransfer.files];
    const paths = files
      .map((file) => {
        const withPath = file as File & { path?: string };
        return withPath.path || file.name;
      })
      .filter(Boolean);
    if (paths.length === 0) {
      setError("Drop did not include usable paths. Paste an absolute path below.");
      return;
    }
    mergePaths(paths);
    void analyze(paths);
  }

  return (
    <section className="tiamat-panel" aria-label="Intake">
      <h2>Intake</h2>
      <div
        className={`tiamat-dropzone${dragActive ? " is-active" : ""}`}
        data-testid="intake-dropzone"
        onDragEnter={(event) => {
          event.preventDefault();
          setDragActive(true);
        }}
        onDragOver={(event) => event.preventDefault()}
        onDragLeave={() => setDragActive(false)}
        onDrop={onDrop}
      >
        <p>Drop folders or files to begin preflight.</p>
        <p className="tiamat-muted">
          Selected content is treated as untrusted project data.
        </p>
        <div className="tiamat-controls">
          <button
            type="button"
            data-testid="intake-pick-files"
            disabled={busy}
            onClick={() => void onPick("file")}
          >
            Add files
          </button>
          <button
            type="button"
            data-testid="intake-pick-folder"
            disabled={busy}
            onClick={() => void onPick("folder")}
          >
            Add folder
          </button>
        </div>
        <label className="tiamat-path-entry">
          <span>Path</span>
          <input
            data-testid="intake-path-input"
            value={pathDraft}
            placeholder="C:\path\to\project"
            onChange={(event) => setPathDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && pathDraft.trim()) {
                mergePaths([pathDraft]);
                void analyze([pathDraft.trim()]);
                setPathDraft("");
              }
            }}
          />
        </label>
        <button
          type="button"
          data-testid="intake-analyze"
          disabled={busy || (!pathDraft.trim() && selectedPaths.length === 0)}
          onClick={() => {
            const paths = pathDraft.trim()
              ? [...selectedPaths, pathDraft.trim()]
              : selectedPaths;
            mergePaths(paths);
            setPathDraft("");
            void analyze(paths);
          }}
        >
          {busy ? "Analyzing…" : "Run preflight"}
        </button>
        {selectedPaths.length > 0 ? (
          <ul className="tiamat-path-list" data-testid="intake-selected-paths">
            {selectedPaths.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        ) : null}
      </div>

      {error ? (
        <p className="tiamat-error" role="alert" data-testid="intake-error">
          {error}
        </p>
      ) : null}

      <PreflightCard
        report={report}
        busy={busy || starting}
        onTrustChange={(untrusted, executionRisk) =>
          void onTrustChange(untrusted, executionRisk)
        }
        onStart={onStart}
        canStart={canStartImplementation(report)}
      />
    </section>
  );
}
