import type { ProjectPlan } from "./contracts";

export interface GraphNode {
  phaseId: string;
  title: string;
  status: string;
  modelTier: string;
  objective: string;
}

export interface GraphEdge {
  from: string;
  to: string;
}

export interface GraphProjection {
  runId: string;
  title: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface ArchitectModelSelection {
  requestedModel: string;
  selectedModel: string;
  degraded: boolean;
  reason: string;
  availableModels: string[];
}

export interface ArchitectInvocationProof {
  planMode: boolean;
  force: boolean;
  autoReview: boolean;
  workspace: string;
  argv: string[];
  model: string;
}

export interface ArchitectRunResult {
  ok: boolean;
  runId: string;
  modelSelection: ArchitectModelSelection;
  plan?: ProjectPlan;
  planJsonPath?: string;
  masterPlanMdPath?: string;
  hashes?: {
    planJsonSha256: string;
    masterPlanMdSha256: string;
  };
  checkpoint?: {
    checkpointId: string;
    projectId: string;
    commit: string;
    branch: string;
    message: string;
    createdAtUtc: string;
  };
  attempts: Array<{
    attempt: number;
    model: string;
    chatId?: string;
    repaired: boolean;
    proof: ArchitectInvocationProof;
  }>;
  degradedMode: boolean;
  error?: string;
  evidence: string[];
}

export function projectGraphFromPlan(plan: ProjectPlan): GraphProjection {
  const nodes: GraphNode[] = plan.phases.map((phase) => ({
    phaseId: phase.phaseId,
    title: phase.title,
    status: phase.status,
    modelTier: phase.modelTier,
    objective: phase.objective,
  }));
  const edges: GraphEdge[] = [];
  for (const phase of plan.phases) {
    for (const dep of phase.dependencies) {
      edges.push({ from: dep, to: phase.phaseId });
    }
  }
  return {
    runId: plan.runId,
    title: plan.title,
    nodes,
    edges,
  };
}
