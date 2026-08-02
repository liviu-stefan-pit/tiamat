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

/** Compact connection state for the shell status light. */
export type CliConnectionKind =
  | "checking"
  | "connected"
  | "auth_needed"
  | "disconnected";

export interface CliConnectionState {
  kind: CliConnectionKind;
  label: string;
  detail: string;
}

export function cliConnectionState(
  report: CursorCapabilityReport | null,
  probing = false,
): CliConnectionState {
  if (probing && !report) {
    return {
      kind: "checking",
      label: "Checking CLI…",
      detail: "Probing Cursor agent CLI",
    };
  }
  if (!report) {
    return {
      kind: "disconnected",
      label: "CLI unknown",
      detail: "Cursor CLI has not been probed yet",
    };
  }
  if (report.status !== "available") {
    return {
      kind: "disconnected",
      label: "CLI offline",
      detail: report.message || `Cursor CLI status: ${report.status}`,
    };
  }
  if (!report.features.modePlan) {
    return {
      kind: "disconnected",
      label: "CLI incomplete",
      detail:
        report.executable
          ? `${report.executable} does not advertise plan mode`
          : "Cursor CLI does not advertise plan mode",
    };
  }
  if (report.auth === "unauthenticated" || report.auth === "error") {
    return {
      kind: "auth_needed",
      label: "CLI needs login",
      detail:
        report.authMessage ||
        "Run `agent login`, then click the status light to re-check",
    };
  }
  return {
    kind: "connected",
    label: "CLI connected",
    detail: [
      report.executable,
      report.version ? `v${report.version}` : null,
      report.auth === "ready" ? "auth ready" : null,
    ]
      .filter(Boolean)
      .join(" · "),
  };
}

export function hasNoninteractiveApproval(features: CursorFeatureFlags): boolean {
  return features.force || features.autoReview;
}
