import type { PhaseExecutionOutcome } from "../../domain/executor";

interface EvidencePanelProps {
  executor: PhaseExecutionOutcome | null;
}

export function EvidencePanel({ executor }: EvidencePanelProps) {
  return (
    <section
      className="tiamat-panel tiamat-evidence"
      aria-label="Evidence and tests"
      data-testid="evidence-panel"
    >
      <h2>Evidence / tests / review</h2>
      {!executor ? (
        <p className="tiamat-muted">No executor evidence yet.</p>
      ) : (
        <>
          <p data-testid="evidence-summary">
            Phase {executor.phaseId}: {executor.ok ? "passed" : "failed"} —{" "}
            {executor.message}
          </p>
          <h3>Gate layers</h3>
          <ul data-testid="evidence-layers">
            {executor.layers.map((layer) => (
              <li key={layer.kind}>
                {layer.kind}: pass={layer.passed} fail={layer.failed} skip=
                {layer.skipped}
                {layer.required ? " (required)" : ""}
                {layer.inapplicable ? ` — ${layer.inapplicableReason ?? "n/a"}` : ""}
              </li>
            ))}
          </ul>
          <h3>Evidence records</h3>
          <ul data-testid="evidence-records">
            {executor.evidence.map((item) => (
              <li key={item.evidenceId}>
                {item.kind}/{item.classification}: {item.summary} (exit{" "}
                {item.exitCode}, {item.durationMs}ms)
              </li>
            ))}
            {executor.evidence.length === 0 ? (
              <li className="tiamat-muted">No evidence records</li>
            ) : null}
          </ul>
          {executor.evidenceNotes.length > 0 ? (
            <>
              <h3>Review notes</h3>
              <ul data-testid="evidence-notes">
                {executor.evidenceNotes.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
            </>
          ) : null}
        </>
      )}
    </section>
  );
}
