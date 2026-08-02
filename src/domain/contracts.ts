export const CURRENT_SCHEMA_VERSION = 1;

export type SourceKind = "file" | "folder";
export type ProjectKind = "git" | "folder" | "notes";
export type EventLevel = "debug" | "info" | "warning" | "error";
export type ModelTier = "composer" | "grok-low" | "grok-medium" | "grok-high";
export type PhaseStatus =
  | "draft"
  | "ready"
  | "queued"
  | "running"
  | "verifying"
  | "passed"
  | "failed"
  | "blocked"
  | "cancelled"
  | "skipped"
  | "needs_review";
export type TestKind =
  | "unit"
  | "integration"
  | "e2e"
  | "manual"
  | "diff"
  | "review"
  | "artifact"
  | "cleanup";
export type RollbackStrategy = "restore" | "quarantine";

export interface IntakeSource {
  path: string;
  kind: SourceKind;
  readOnly: boolean;
}

export interface ProjectSummary {
  projectId: string;
  root: string;
  kind: ProjectKind;
  languages: string[];
  buildSystems: string[];
  testCommands: string[];
  warnings: string[];
}

export interface IntakeManifest {
  schemaVersion: number;
  intakeId: string;
  sources: IntakeSource[];
  projects: ProjectSummary[];
  inventoryArtifact: string;
}

export interface EventEnvelope {
  schemaVersion: number;
  eventId: string;
  sequence: number;
  runId: string;
  projectId?: string;
  phaseId?: string;
  attemptId?: string;
  processId?: string;
  type: string;
  level: EventLevel;
  timestampUtc: string;
  message: string;
  payload: Record<string, unknown>;
}

export interface AcceptanceCriterion {
  criterionId: string;
  description: string;
  requiredEvidenceKinds: TestKind[];
}

export interface TestExpected {
  exitCode: number;
  artifacts: string[];
}

export interface TestSpec {
  testId: string;
  command: string[];
  workingDirectory: string;
  timeoutSeconds: number;
  resourceLocks: string[];
  expected: TestExpected;
  covers: string[];
  inapplicableReason?: string;
}

export interface ManualCheck {
  description: string;
  blocking: boolean;
}

export interface RollbackSpec {
  checkpoint: string;
  strategy: RollbackStrategy;
}

export interface PhasePlan {
  phaseId: string;
  title: string;
  objective: string;
  dependencies: string[];
  projectIds: string[];
  readRoots: string[];
  writeRoots: string[];
  modelTier: ModelTier;
  estimatedMinutes: number;
  acceptanceCriteria: AcceptanceCriterion[];
  unitTests: TestSpec[];
  integrationTests: TestSpec[];
  e2eTests: TestSpec[];
  manualChecks: ManualCheck[];
  rollback: RollbackSpec;
  expectedArtifacts: string[];
  prompt: string;
  status: PhaseStatus;
  evidence: string[];
}

export interface FinalGate {
  gateId: string;
  description: string;
  dependencies: string[];
  requiredEvidenceKinds: TestKind[];
}

export interface ProjectPlan {
  schemaVersion: number;
  runId: string;
  title: string;
  summary: string;
  assumptions: string[];
  risks: string[];
  phases: PhasePlan[];
  finalGates: FinalGate[];
}

export interface AppInfo {
  name: string;
  version: string;
  schemaVersion: number;
  orchestratorMode: string;
  storeSchemaVersion?: number;
}

export interface ContractValidationResult {
  valid: boolean;
  schemaName: string;
  error?: string;
}

export interface OrchestratorStatus {
  mode: string;
  activeRuns: number;
  message: string;
}

export type ContractSchemaName =
  | "intake-manifest"
  | "event-envelope"
  | "project-plan"
  | "phase-result";

export function assertSchemaVersion(
  schemaVersion: number,
  expected = CURRENT_SCHEMA_VERSION,
): void {
  if (schemaVersion !== expected) {
    throw new Error(
      `incompatible schema version: expected ${expected}, found ${schemaVersion}`,
    );
  }
}
