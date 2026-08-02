import type { SchedulerSnapshot } from "../../domain/scheduler";
import "./Reports.css";

interface AttemptTimelineProps {
  scheduler: SchedulerSnapshot | null;
  selectedPhaseId?: string | null;
  onSelectPhase?: (phaseId: string) => void;
}

export function AttemptTimeline({
  scheduler,
  selectedPhaseId = null,
  onSelectPhase,
}: AttemptTimelineProps) {
  const attempts = scheduler?.attempts ?? [];
  return (
    <section
      className="tiamat-panel tiamat-timeline"
      aria-label="Attempt timeline"
      data-testid="attempt-timeline"
    >
      <h2>Attempt timeline</h2>
      {attempts.length === 0 ? (
        <p className="tiamat-muted">No attempts recorded yet.</p>
      ) : (
        <ol className="tiamat-timeline-list">
          {attempts.map((attempt) => (
            <li
              key={attempt.attemptId}
              data-testid="timeline-attempt"
              data-phase-id={attempt.phaseId}
              data-attempt-id={attempt.attemptId}
              className={
                selectedPhaseId === attempt.phaseId ? "is-selected" : undefined
              }
            >
              <button
                type="button"
                className="tiamat-timeline-button"
                onClick={() => onSelectPhase?.(attempt.phaseId)}
              >
                <span className="tiamat-timeline-phase">{attempt.phaseId}</span>
                <span>
                  attempt #{attempt.attemptNumber} · {attempt.status}
                  {attempt.terminalResult ? ` · ${attempt.terminalResult}` : ""}
                </span>
                <span className="tiamat-muted">{attempt.selectedModel}</span>
              </button>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
