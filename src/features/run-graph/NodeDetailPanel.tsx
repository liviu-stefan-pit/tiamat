import type { PhasePlan, ProjectPlan } from "../../domain/contracts";
import type { PhaseExecutionOutcome } from "../../domain/executor";
import type { SchedulerSnapshot } from "../../domain/scheduler";

interface NodeDetailPanelProps {
  phaseId: string | null;
  plan: ProjectPlan | null;
  scheduler: SchedulerSnapshot | null;
  executor: PhaseExecutionOutcome | null;
}

export function NodeDetailPanel({
  phaseId,
  plan,
  scheduler,
  executor,
}: NodeDetailPanelProps) {
  if (!phaseId) {
    return (
      <section
        className="tiamat-panel tiamat-node-detail"
        aria-label="Node details"
        data-testid="node-detail"
      >
        <h2>Node details</h2>
        <p className="tiamat-muted">Select a phase node to inspect objective, attempts, and evidence.</p>
      </section>
    );
  }

  const phase: PhasePlan | undefined = plan?.phases.find((item) => item.phaseId === phaseId);
  const runtime = scheduler?.phases.find((item) => item.phaseId === phaseId);
  const attempts =
    scheduler?.attempts.filter((item) => item.phaseId === phaseId) ?? [];
  const relatedExecutor =
    executor && executor.phaseId === phaseId ? executor : null;

  return (
    <section
      className="tiamat-panel tiamat-node-detail"
      aria-label="Node details"
      data-testid="node-detail"
      data-phase-id={phaseId}
    >
      <h2>Node details</h2>
      <p data-testid="node-detail-title">
        <strong>{phaseId}</strong>
        {phase ? ` — ${phase.title}` : runtime ? ` — ${runtime.title}` : ""}
      </p>
      <dl className="tiamat-detail-grid">
        <div>
          <dt>Status</dt>
          <dd data-testid="node-detail-status">
            {runtime?.status ?? phase?.status ?? "unknown"}
          </dd>
        </div>
        <div>
          <dt>Model</dt>
          <dd data-testid="node-detail-model">
            {runtime?.selectedModel ?? phase?.modelTier ?? "—"}
          </dd>
        </div>
        <div>
          <dt>Attempts</dt>
          <dd data-testid="node-detail-attempts">{attempts.length}</dd>
        </div>
        <div>
          <dt>Write roots</dt>
          <dd data-testid="node-detail-roots">
            {(runtime?.writeRoots ?? phase?.writeRoots ?? []).join(", ") || "—"}
          </dd>
        </div>
      </dl>
      <h3>Objective</h3>
      <p data-testid="node-detail-objective">
        {phase?.objective ?? runtime?.selectionReason ?? "—"}
      </p>
      <h3>Acceptance criteria</h3>
      <ul data-testid="node-detail-ac">
        {(phase?.acceptanceCriteria ?? []).map((criterion) => (
          <li key={criterion.criterionId}>{criterion.description}</li>
        ))}
        {(phase?.acceptanceCriteria ?? []).length === 0 ? (
          <li className="tiamat-muted">None recorded</li>
        ) : null}
      </ul>
      <h3>Attempt history</h3>
      <ol data-testid="node-detail-attempt-list">
        {attempts.map((attempt) => (
          <li key={attempt.attemptId}>
            #{attempt.attemptNumber} · {attempt.status}
            {attempt.terminalResult ? ` · ${attempt.terminalResult}` : ""} ·{" "}
            {attempt.selectedModel}
          </li>
        ))}
        {attempts.length === 0 ? (
          <li className="tiamat-muted">No attempts yet</li>
        ) : null}
      </ol>
      <h3>Tests / artifacts</h3>
      <ul data-testid="node-detail-tests">
        {(phase?.unitTests ?? []).map((test) => (
          <li key={test.testId}>unit: {test.command.join(" ")}</li>
        ))}
        {(phase?.integrationTests ?? []).map((test) => (
          <li key={test.testId}>integration: {test.command.join(" ")}</li>
        ))}
        {(phase?.e2eTests ?? []).map((test) => (
          <li key={test.testId}>e2e: {test.command.join(" ")}</li>
        ))}
        {(phase?.expectedArtifacts ?? []).map((artifact) => (
          <li key={artifact}>artifact: {artifact}</li>
        ))}
        {!phase ? <li className="tiamat-muted">Plan phase not loaded</li> : null}
      </ul>
      {relatedExecutor ? (
        <>
          <h3>Latest evidence</h3>
          <p data-testid="node-detail-failure">
            {relatedExecutor.ok
              ? relatedExecutor.message
              : `Failure: ${relatedExecutor.message}`}
          </p>
          <ul>
            {relatedExecutor.evidence.map((item) => (
              <li key={item.evidenceId}>
                {item.kind}: {item.classification} (exit {item.exitCode})
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </section>
  );
}
