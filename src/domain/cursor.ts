export type CursorCapabilityStatus =
  | "absent"
  | "available"
  | "unsupported_version"
  | "error";

export type CursorAuthStatus =
  | "unknown"
  | "ready"
  | "unauthenticated"
  | "error";

export interface CursorFeatureFlags {
  printMode: boolean;
  outputFormat: boolean;
  streamJson: boolean;
  workspace: boolean;
  force: boolean;
  model: boolean;
  listModels: boolean;
  trust: boolean;
  apiKey: boolean;
  streamPartialOutput: boolean;
  modePlan: boolean;
  resume: boolean;
  autoReview: boolean;
}

export interface CursorModelInfo {
  id: string;
  label: string;
}

export interface CursorCapabilityReport {
  status: CursorCapabilityStatus;
  message: string;
  executable?: string;
  version?: string;
  versionRaw?: string;
  minimumVersion: string;
  helpExcerpt?: string;
  features: CursorFeatureFlags;
  auth: CursorAuthStatus;
  authMessage?: string;
  models: CursorModelInfo[];
  probedAtUtc: string;
}

export interface CursorModelsReport {
  status: "available" | "absent" | "error";
  models: CursorModelInfo[];
  message?: string;
  executable?: string;
}

export interface CursorCommandPreview {
  argv: string[];
  commandDisplay: string;
  stdinPreview: string;
  timeoutMs: number;
  workspace: string;
  executable: string;
  spawned: boolean;
}

export const EMPTY_FEATURES: CursorFeatureFlags = {
  printMode: false,
  outputFormat: false,
  streamJson: false,
  workspace: false,
  force: false,
  model: false,
  listModels: false,
  trust: false,
  apiKey: false,
  streamPartialOutput: false,
  modePlan: false,
  resume: false,
  autoReview: false,
};

export function formatCursorStatus(report: CursorCapabilityReport | null): string {
  if (!report) return "unknown";
  const version = report.version ? ` v${report.version}` : "";
  const auth = report.auth !== "unknown" ? ` · auth ${report.auth}` : "";
  return `${report.status}${version}${auth}`;
}

export function hasNoninteractiveApproval(features: CursorFeatureFlags): boolean {
  return features.force || features.autoReview;
}
