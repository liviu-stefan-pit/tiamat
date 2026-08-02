export type SourceKind = "file" | "folder";
export type ProjectKind = "git" | "folder" | "notes";

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

export interface InventorySummary {
  fileCount: number;
  dirCount: number;
  totalBytes: number;
  ignoredCount: number;
  truncated: boolean;
  truncationReason?: string;
  estimatedCopyBytes: number;
}

export interface TrustState {
  confirmed: boolean;
  acknowledgedUntrusted: boolean;
  acknowledgedExecutionRisk: boolean;
}

export interface SecretRiskFinding {
  relativePath: string;
  patternId: string;
  matchHash: string;
  matchByteLen: number;
}

export interface CursorProbeStub {
  status: string;
  message: string;
  executable?: string;
  version?: string;
  auth?: string;
  modelCount?: number;
  hasNoninteractiveApproval?: boolean;
}

export interface IntakeLimits {
  maxFiles: number;
  maxTotalBytes: number;
  maxFileBytes: number;
  maxSecretScanBytes: number;
  maxDepth: number;
}

export interface PreflightReport {
  schemaVersion: number;
  manifest: IntakeManifest;
  inventory: InventorySummary;
  warnings: string[];
  blockers: string[];
  secretRisks: SecretRiskFinding[];
  escapeAttempts: string[];
  trust: TrustState;
  cursor: CursorProbeStub;
  canStart: boolean;
  readRoots: string[];
  writeRootsPreview: string[];
  limits: IntakeLimits;
  untrustedContentNotice: string;
}

export function canStartImplementation(report: PreflightReport | null): boolean {
  return Boolean(report?.canStart);
}
