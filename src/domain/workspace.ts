export type ManagedProjectKind = "gitClone" | "nonGitCopy" | "notesSnapshot";
export type PromotionStatus = "unpromoted" | "exported" | "promoted";

export interface SourceFingerprint {
  path: string;
  kind: string;
  head?: string;
  branch?: string;
  statusPorcelain: string;
  statusHash: string;
  treeHash: string;
  capturedAtUtc: string;
}

export interface FingerprintPair {
  before: SourceFingerprint;
  after: SourceFingerprint;
  unchanged: boolean;
}

export interface DirtyOverlayMetadata {
  sourceHead: string;
  hadStaged: boolean;
  hadUnstaged: boolean;
  hadUntracked: boolean;
  stagedPatchBytes: number;
  unstagedPatchBytes: number;
  untrackedFiles: string[];
  overlayArtifact: string;
}

export interface ManagedProject {
  projectId: string;
  sourceRoot: string;
  managedRoot: string;
  kind: ManagedProjectKind;
  baselineCommit?: string;
  baselineBranch: string;
  worktreePath?: string;
  writeRoot: string;
  readRoots: string[];
  dirtyOverlay?: DirtyOverlayMetadata;
  sourceFingerprint: SourceFingerprint;
  lockName: string;
}

export interface CheckpointRecord {
  checkpointId: string;
  projectId: string;
  commit: string;
  branch: string;
  message: string;
  createdAtUtc: string;
  parentCheckpointId?: string;
}

export interface QuarantineRecord {
  quarantineId: string;
  projectId: string;
  reason: string;
  sourcePath: string;
  quarantinePath: string;
  createdAtUtc: string;
  fromCheckpointId?: string;
}

export interface PromotionMetadata {
  status: PromotionStatus;
  exportPath?: string;
  promotedAtUtc?: string;
  notes?: string;
}

export interface RetentionPolicy {
  retainUnpromoted: boolean;
  maxQuarantineEntries: number;
  allowDestructiveCleanup: boolean;
}

export interface RunWorkspaceManifest {
  schemaVersion: number;
  runId: string;
  intakeId: string;
  managedRunRoot: string;
  controlRoot: string;
  projects: ManagedProject[];
  notesRoots: string[];
  checkpoints: CheckpointRecord[];
  quarantines: QuarantineRecord[];
  promotion: PromotionMetadata;
  retention: RetentionPolicy;
  fingerprintPairs: FingerprintPair[];
  createdAtUtc: string;
  sourceUnchanged: boolean;
}

export interface RootValidationResult {
  ok: boolean;
  writeErrors: string[];
  readErrors: string[];
}

export function summarizeWorkspace(manifest: RunWorkspaceManifest): string {
  const status = manifest.sourceUnchanged
    ? "source unchanged"
    : "source mutated";
  return `${manifest.projects.length} project(s), ${status}, promotion=${manifest.promotion.status}`;
}
