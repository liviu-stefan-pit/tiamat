import type { CompletionSummary } from "../../domain/reports";
import "./Reports.css";

interface CompletionSummaryProps {
  summary: CompletionSummary | null;
  busy?: boolean;
  onExportProject?: (projectId: string) => void;
  onPromote?: () => void;
  defaultProjectId?: string | null;
}

export function CompletionSummaryPanel({
  summary,
  busy = false,
  onExportProject,
  onPromote,
  defaultProjectId,
}: CompletionSummaryProps) {
  if (!summary) {
    return (
      <section
        className="tiamat-panel tiamat-completion"
        aria-label="Completion summary"
        data-testid="completion-summary"
      >
        <h2>Completion summary</h2>
        <p className="tiamat-muted">Run completion report appears when a run finishes.</p>
      </section>
    );
  }

  const projectId = defaultProjectId ?? null;

  return (
    <section
      className="tiamat-panel tiamat-completion"
      aria-label="Completion summary"
      data-testid="completion-summary"
      data-complete={summary.complete ? "true" : "false"}
      role="status"
    >
      <h2>Completion summary</h2>
      <p data-testid="completion-title">
        {summary.title} — {summary.runStatus}
      </p>
      <dl className="tiamat-completion-counts" data-testid="completion-counts">
        <div>
          <dt>Completed</dt>
          <dd>{summary.counts.completed}</dd>
        </div>
        <div>
          <dt>Failed</dt>
          <dd>{summary.counts.failed}</dd>
        </div>
        <div>
          <dt>Blocked</dt>
          <dd>{summary.counts.blocked}</dd>
        </div>
        <div>
          <dt>Skipped</dt>
          <dd>{summary.counts.skipped}</dd>
        </div>
      </dl>
      <h3>Phases</h3>
      <ul data-testid="completion-phases">
        {summary.tallies.map((tally) => (
          <li key={tally.phaseId}>
            {tally.phaseId} {tally.title}: {tally.status} ({tally.runtimeStatus})
          </li>
        ))}
      </ul>
      <h3>Output</h3>
      <p data-testid="completion-output">
        {summary.outputPath ?? "No managed output yet"}
        {summary.branch ? ` · branch ${summary.branch}` : ""}
      </p>
      <h3>Tests</h3>
      <ul data-testid="completion-tests">
        {summary.testResults.map((test) => (
          <li key={test.evidenceId}>
            {test.kind}: {test.classification} — {test.summary}
          </li>
        ))}
        {summary.testResults.length === 0 ? (
          <li className="tiamat-muted">No test evidence captured</li>
        ) : null}
      </ul>
      <h3>Reviews</h3>
      <ul data-testid="completion-reviews">
        {summary.reviewFindings.map((finding) => (
          <li key={finding}>{finding}</li>
        ))}
        {summary.reviewFindings.length === 0 ? (
          <li className="tiamat-muted">No review findings</li>
        ) : null}
      </ul>
      <h3>Docs / TestBench</h3>
      <p data-testid="completion-docs">
        Docs: {summary.docsPath ?? "—"} · TestBench: {summary.testBenchPath ?? "—"}
      </p>
      <h3>Promotion</h3>
      <ul data-testid="completion-promotion">
        {summary.promotionInstructions.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
      {onExportProject || onPromote ? (
        <div className="tiamat-completion-actions" data-testid="completion-actions">
          {onExportProject && projectId ? (
            <button
              type="button"
              data-testid="completion-export"
              disabled={busy}
              onClick={() => onExportProject(projectId)}
            >
              Export
            </button>
          ) : null}
          {onPromote ? (
            <button
              type="button"
              data-testid="completion-promote"
              disabled={busy}
              onClick={() => onPromote()}
            >
              Promote
            </button>
          ) : null}
        </div>
      ) : null}
      <p data-testid="completion-cleanup">
        Process registry empty: {summary.processRegistryEmpty ? "yes" : "no"} ·
        Cleanup confirmed: {summary.cleanupConfirmed ? "yes" : "no"}
      </p>
    </section>
  );
}
