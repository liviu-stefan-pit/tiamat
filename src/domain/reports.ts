import type { EventEnvelope, ProjectPlan } from "./contracts";
import type { PhaseExecutionOutcome } from "./executor";
import type { SchedulerSnapshot } from "./scheduler";
import type { RunWorkspaceManifest } from "./workspace";

export type PhaseTallyStatus =
  | "completed"
  | "failed"
  | "blocked"
  | "skipped"
  | "in_progress"
  | "pending";

export interface PhaseTally {
  phaseId: string;
  title: string;
  status: PhaseTallyStatus;
  runtimeStatus: string;
}

export interface CompletionSummary {
  runId: string;
  title: string;
  runStatus: string;
  tallies: PhaseTally[];
  counts: Record<PhaseTallyStatus, number>;
  outputPath: string | null;
  branch: string | null;
  testResults: Array<{
    evidenceId: string;
    kind: string;
    classification: string;
    summary: string;
    exitCode: number;
  }>;
  reviewFindings: string[];
  docsPath: string | null;
  testBenchPath: string | null;
  promotionInstructions: string[];
  processRegistryEmpty: boolean;
  cleanupConfirmed: boolean;
  complete: boolean;
}

function mapRuntimeToTally(status: string): PhaseTallyStatus {
  switch (status) {
    case "passed":
      return "completed";
    case "failed":
      return "failed";
    case "blocked":
    case "needs_review":
      return "blocked";
    case "skipped":
    case "cancelled":
      return "skipped";
    case "running":
    case "verifying":
    case "queued":
      return "in_progress";
    default:
      return "pending";
  }
}

export function buildCompletionSummary(input: {
  runId: string;
  runStatus: string;
  plan: ProjectPlan | null;
  scheduler: SchedulerSnapshot | null;
  executor: PhaseExecutionOutcome | null;
  workspace: RunWorkspaceManifest | null;
  events: EventEnvelope[];
  processRegistryEmpty: boolean;
}): CompletionSummary {
  const { runId, runStatus, plan, scheduler, executor, workspace, events } =
    input;

  const tallies: PhaseTally[] = (scheduler?.phases ?? plan?.phases ?? []).map(
    (phase) => {
      const runtimeStatus =
        "status" in phase ? String(phase.status) : "draft";
      return {
        phaseId: phase.phaseId,
        title: phase.title,
        status: mapRuntimeToTally(runtimeStatus),
        runtimeStatus,
      };
    },
  );

  const counts: Record<PhaseTallyStatus, number> = {
    completed: 0,
    failed: 0,
    blocked: 0,
    skipped: 0,
    in_progress: 0,
    pending: 0,
  };
  for (const tally of tallies) {
    counts[tally.status] += 1;
  }

  const testResults =
    executor?.evidence.map((item) => ({
      evidenceId: item.evidenceId,
      kind: item.kind,
      classification: item.classification,
      summary: item.summary,
      exitCode: item.exitCode,
    })) ?? [];

  const reviewFindings = events
    .filter(
      (event) =>
        event.type.includes("review") ||
        event.type === "phase.needs_review" ||
        (executor?.evidenceNotes ?? []).length > 0,
    )
    .map((event) => event.message)
    .slice(0, 20);

  if ((executor?.evidenceNotes ?? []).length > 0) {
    for (const note of executor!.evidenceNotes) {
      if (!reviewFindings.includes(note)) reviewFindings.push(note);
    }
  }

  const cleanupConfirmed = events.some(
    (event) =>
      event.type === "cleanup.succeeded" ||
      event.message.toLowerCase().includes("cleanup proof"),
  );

  const outputPath =
    workspace?.managedRunRoot ??
    workspace?.projects?.[0]?.managedRoot ??
    workspace?.promotion.exportPath ??
    null;
  const branch = workspace?.projects?.[0]?.baselineBranch ?? null;

  const docsPath = outputPath
    ? `${outputPath.replace(/[\\/]$/, "")}/.tiamat/docs`
    : null;
  const testBenchPath = outputPath
    ? `${outputPath.replace(/[\\/]$/, "")}/.tiamat/testbench`
    : null;

  const promotionInstructions = [
    "Review the isolated managed output before promotion.",
    "Use Export / Open output to inspect the worktree or copy.",
    "Promote or merge only after fingerprints confirm the source inputs are unchanged.",
  ];

  const terminal =
    runStatus === "completed" ||
    runStatus === "failed" ||
    runStatus === "cancelled";
  const complete =
    terminal &&
    input.processRegistryEmpty &&
    (cleanupConfirmed || runStatus === "cancelled");

  return {
    runId,
    title: plan?.title ?? "Tiamat run",
    runStatus,
    tallies,
    counts,
    outputPath,
    branch,
    testResults,
    reviewFindings,
    docsPath,
    testBenchPath,
    promotionInstructions,
    processRegistryEmpty: input.processRegistryEmpty,
    cleanupConfirmed,
    complete,
  };
}
