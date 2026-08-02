import type { PreflightReport } from "../../domain/intake";

interface PreflightCardProps {
  report: PreflightReport | null;
  busy: boolean;
  canStart: boolean;
  onTrustChange: (
    acknowledgedUntrusted: boolean,
    acknowledgedExecutionRisk: boolean,
  ) => void;
  onStart: () => void;
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}

export function PreflightCard({
  report,
  busy,
  canStart,
  onTrustChange,
  onStart,
}: PreflightCardProps) {
  if (!report) {
    return (
      <div className="tiamat-preflight" data-testid="preflight-empty">
        <h3>Preflight</h3>
        <p className="tiamat-muted">
          Run intake to review projects, warnings, and trust requirements.
        </p>
        <button
          type="button"
          data-testid="start-implementation"
          disabled
          title="Complete preflight and trust first"
        >
          Start implementation
        </button>
      </div>
    );
  }

  return (
    <div className="tiamat-preflight" data-testid="preflight-card">
      <h3>Preflight</h3>
      <p className="tiamat-muted" data-testid="preflight-notice">
        {report.untrustedContentNotice}
      </p>

      <dl className="tiamat-preflight-meta">
        <div>
          <dt>Sources</dt>
          <dd data-testid="preflight-source-count">
            {report.manifest.sources.length}
          </dd>
        </div>
        <div>
          <dt>Projects</dt>
          <dd data-testid="preflight-project-count">
            {report.manifest.projects.length}
          </dd>
        </div>
        <div>
          <dt>Files</dt>
          <dd data-testid="preflight-file-count">{report.inventory.fileCount}</dd>
        </div>
        <div>
          <dt>Copy estimate</dt>
          <dd data-testid="preflight-disk-estimate">
            {formatBytes(report.inventory.estimatedCopyBytes)}
          </dd>
        </div>
      </dl>

      <div data-testid="preflight-projects">
        <h4>Detected projects</h4>
        <ul className="tiamat-project-list">
          {report.manifest.projects.map((project) => (
            <li key={project.projectId} data-testid="preflight-project">
              <strong>{project.projectId}</strong> · {project.kind}
              <div className="tiamat-muted">{project.root}</div>
              <div className="tiamat-muted">
                {[
                  project.languages.join(", ") || "no language",
                  project.buildSystems.join(", ") || "no build",
                  project.testCommands.join(", ") || "no tests",
                ].join(" · ")}
              </div>
            </li>
          ))}
        </ul>
      </div>

      <div>
        <h4>Read roots</h4>
        <ul data-testid="preflight-read-roots">
          {report.readRoots.map((root) => (
            <li key={root}>{root}</li>
          ))}
        </ul>
        <h4>Write roots (preview)</h4>
        <ul data-testid="preflight-write-roots">
          {report.writeRootsPreview.map((root) => (
            <li key={root}>{root}</li>
          ))}
        </ul>
      </div>

      {report.warnings.length > 0 ? (
        <div data-testid="preflight-warnings">
          <h4>Warnings</h4>
          <ul>
            {report.warnings.map((warning) => (
              <li key={warning} data-testid="preflight-warning">
                {warning}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {report.blockers.length > 0 ? (
        <div data-testid="preflight-blockers">
          <h4>Blockers</h4>
          <ul>
            {report.blockers.map((blocker) => (
              <li key={blocker} data-testid="preflight-blocker">
                {blocker}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {report.secretRisks.length > 0 ? (
        <div data-testid="preflight-secret-risks">
          <h4>Secret-risk metadata</h4>
          <ul>
            {report.secretRisks.map((finding) => (
              <li key={`${finding.relativePath}:${finding.patternId}`}>
                {finding.relativePath} · {finding.patternId} · hash{" "}
                {finding.matchHash.slice(0, 12)}…
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <p className="tiamat-muted" data-testid="preflight-cursor">
        Cursor: {report.cursor.status} — {report.cursor.message}
      </p>

      <div className="tiamat-trust" data-testid="preflight-trust">
        <label>
          <input
            type="checkbox"
            data-testid="trust-untrusted"
            checked={report.trust.acknowledgedUntrusted}
            disabled={busy || report.blockers.length > 0}
            onChange={(event) =>
              onTrustChange(
                event.target.checked,
                report.trust.acknowledgedExecutionRisk,
              )
            }
          />
          I understand selected content is untrusted and cannot override Tiamat
          policy.
        </label>
        <label>
          <input
            type="checkbox"
            data-testid="trust-execution"
            checked={report.trust.acknowledgedExecutionRisk}
            disabled={busy || report.blockers.length > 0}
            onChange={(event) =>
              onTrustChange(
                report.trust.acknowledgedUntrusted,
                event.target.checked,
              )
            }
          />
          I acknowledge build/test code will run with my non-elevated account
          rights.
        </label>
      </div>

      <button
        type="button"
        data-testid="start-implementation"
        disabled={!canStart || busy}
        onClick={onStart}
      >
        Start implementation
      </button>
      {!canStart ? (
        <p className="tiamat-muted" data-testid="start-gated-reason">
          Start stays disabled until preflight has no blockers and both trust
          confirmations are checked.
        </p>
      ) : null}
    </div>
  );
}
