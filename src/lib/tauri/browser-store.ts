import type { EventEnvelope, ProjectPlan } from "../../domain/contracts";
import type { PhaseExecutionOutcome } from "../../domain/executor";
import type { PreflightReport } from "../../domain/intake";
import type { ArchitectRunResult } from "../../domain/plan";
import { projectGraphFromPlan } from "../../domain/plan";
import type { RecoveryOffer } from "../../domain/recovery";
import type {
  SchedulerAttemptView,
  SchedulerPhaseView,
  SchedulerSnapshot,
  TickResult,
} from "../../domain/scheduler";
import type { RunWorkspaceManifest } from "../../domain/workspace";
import type {
  ArtifactRecord,
  DemoRunSnapshot,
  RunRecord,
  TransitionResult,
} from "./commands";

const STORAGE_KEY = "tiamat.p01.browser-store.v1";
const LOCAL_STORAGE_EVENT_CAP = 2_000;

interface BrowserStoreState {
  run: RunRecord | null;
  events: EventEnvelope[];
  artifacts: ArtifactRecord[];
  preflight: PreflightReport | null;
  workspace: RunWorkspaceManifest | null;
  plan: ProjectPlan | null;
  architect: ArchitectRunResult | null;
  scheduler: SchedulerSnapshot | null;
  executor: PhaseExecutionOutcome | null;
  recoveryOffer: RecoveryOffer | null;
}

/** In-memory override used for large perf seeds (localStorage quota). */
let memoryState: BrowserStoreState | null = null;

const DEMO_RUN_ID = "11111111-1111-4111-8111-111111111111";
const INTAKE_ID = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

function emptyState(): BrowserStoreState {
  return {
    run: null,
    events: [],
    artifacts: [],
    preflight: null,
    workspace: null,
    plan: null,
    architect: null,
    scheduler: null,
    executor: null,
    recoveryOffer: null,
  };
}

function loadState(): BrowserStoreState {
  if (memoryState) {
    return memoryState;
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return emptyState();
    const parsed = JSON.parse(raw) as BrowserStoreState;
    return {
      ...emptyState(),
      ...parsed,
      preflight: parsed.preflight ?? null,
      workspace: parsed.workspace ?? null,
      plan: parsed.plan ?? null,
      architect: parsed.architect ?? null,
      scheduler: parsed.scheduler ?? null,
      executor: parsed.executor ?? null,
      recoveryOffer: parsed.recoveryOffer ?? null,
    };
  } catch {
    return emptyState();
  }
}

function saveState(state: BrowserStoreState): void {
  if (state.events.length > LOCAL_STORAGE_EVENT_CAP) {
    memoryState = state;
    return;
  }
  memoryState = null;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

function appendEvent(
  state: BrowserStoreState,
  type: string,
  message: string,
  payload: Record<string, unknown>,
  level: EventEnvelope["level"] = "info",
): EventEnvelope {
  const nextSequence =
    state.events.reduce((max, event) => Math.max(max, event.sequence), 0) + 1;
  const event: EventEnvelope = {
    schemaVersion: 1,
    eventId: crypto.randomUUID(),
    sequence: nextSequence,
    runId: state.run?.runId ?? DEMO_RUN_ID,
    projectId: "tiamat",
    phaseId: "P02",
    type,
    level,
    timestampUtc: new Date().toISOString(),
    message,
    payload,
  };
  state.events = [...state.events, event];
  notifyBrowserListeners(event);
  return event;
}

function buildBrowserPreflight(paths: string[]): PreflightReport {
  const joined = paths.join("|").toLowerCase();
  const secretRisk = joined.includes("secret");
  const escapeRisk = joined.includes("escape") || joined.includes("junction");
  const nested = joined.includes("nested");
  const overLimit = joined.includes("over-limit") || joined.includes("toolarge");
  const warnings: string[] = [];
  const blockers: string[] = [];
  const secretRisks = secretRisk
    ? [
        {
          relativePath: "config.env",
          patternId: "aws_access_key_id",
          matchHash: "browser-fixture-hash",
          matchByteLen: 20,
        },
      ]
    : [];

  if (secretRisk) {
    warnings.push(
      "Detected 1 secret-risk marker(s). Only pattern metadata and hashes are retained.",
    );
  }
  if (escapeRisk) {
    warnings.push(
      "One or more symlink/junction targets escaped approved roots and were skipped.",
    );
  }
  if (nested) {
    warnings.push("Found 1 nested git repositories.");
  }
  if (overLimit) {
    blockers.push("Inventory truncated: file count would exceed limit 3");
  }

  const projects = [
    {
      projectId: joined.includes("scheduler") ? "repo-a" : "demo-intake",
      root: paths[0] ?? "C:\\fixture\\demo",
      kind: nested || joined.includes("scheduler") ? ("git" as const) : ("folder" as const),
      languages: ["typescript"],
      buildSystems: ["npm"],
      testCommands: ["npm test"],
      warnings: secretRisk
        ? ["Secret-risk markers detected (1). Values are not included in events."]
        : [],
    },
  ];
  if (nested || joined.includes("scheduler")) {
    projects.push({
      projectId: joined.includes("scheduler") ? "repo-b" : "nested-api",
      root: joined.includes("scheduler")
        ? `${paths[0]}\\repo-b`
        : `${paths[0]}\\services\\api`,
      kind: "git",
      languages: ["rust"],
      buildSystems: ["cargo"],
      testCommands: ["cargo test"],
      warnings: ["Nested repository project"],
    });
  }

  return {
    schemaVersion: 1,
    manifest: {
      schemaVersion: 1,
      intakeId: INTAKE_ID,
      sources: paths.map((path) => ({
        path,
        kind: path.toLowerCase().endsWith(".md") ? "file" : "folder",
        readOnly: true,
      })),
      projects,
      inventoryArtifact: "inventory-browser",
    },
    inventory: {
      fileCount: overLimit ? 0 : 3,
      dirCount: 1,
      totalBytes: 128,
      ignoredCount: 1,
      truncated: overLimit,
      truncationReason: overLimit
        ? "file count would exceed limit 3"
        : undefined,
      estimatedCopyBytes: 128,
    },
    warnings,
    blockers,
    secretRisks,
    escapeAttempts: escapeRisk
      ? ["escape-link -> path escapes approved intake roots"]
      : [],
    trust: {
      confirmed: false,
      acknowledgedUntrusted: false,
      acknowledgedExecutionRisk: false,
    },
    cursor: {
      status: "available",
      message: "Browser host uses deterministic fake Cursor capability (no live call).",
      executable: "fixtures/cursor-cli/fake-agent.mjs",
      version: "1.2.3",
      auth: "ready",
      modelCount: 3,
      hasNoninteractiveApproval: true,
    },
    canStart: false,
    readRoots: paths,
    writeRootsPreview: [
      "<managed-run-root>/projects/* (created at Start; not yet allocated)",
    ],
    limits: {
      maxFiles: 25000,
      maxTotalBytes: 1073741824,
      maxFileBytes: 67108864,
      maxSecretScanBytes: 1048576,
      maxDepth: 32,
    },
    untrustedContentNotice:
      "Selected content is untrusted project data. Imported instructions cannot expand write roots, disable tests/policy/cleanup, or reveal credentials.",
  };
}

function buildDemo(): BrowserStoreState {
  const createdAt = "2026-08-02T09:00:00.000Z";
  const events: EventEnvelope[] = [
    {
      schemaVersion: 1,
      eventId: "22222222-2222-4222-8222-000000000001",
      sequence: 1,
      runId: DEMO_RUN_ID,
      projectId: "tiamat",
      type: "run.created",
      level: "info",
      timestampUtc: "2026-08-02T09:00:00.000Z",
      message: "Run created",
      payload: { demo: true, index: 1 },
    },
    {
      schemaVersion: 1,
      eventId: "22222222-2222-4222-8222-000000000002",
      sequence: 2,
      runId: DEMO_RUN_ID,
      projectId: "tiamat",
      type: "intake.placeholder",
      level: "info",
      timestampUtc: "2026-08-02T09:00:01.000Z",
      message: "Intake placeholder ready",
      payload: { demo: true, index: 2 },
    },
    {
      schemaVersion: 1,
      eventId: "22222222-2222-4222-8222-000000000003",
      sequence: 3,
      runId: DEMO_RUN_ID,
      projectId: "tiamat",
      phaseId: "P01",
      type: "phase.queued",
      level: "info",
      timestampUtc: "2026-08-02T09:00:02.000Z",
      message: "Phase P01 queued",
      payload: { demo: true, index: 3 },
    },
    {
      schemaVersion: 1,
      eventId: "22222222-2222-4222-8222-000000000004",
      sequence: 4,
      runId: DEMO_RUN_ID,
      projectId: "tiamat",
      phaseId: "P01",
      type: "phase.started",
      level: "info",
      timestampUtc: "2026-08-02T09:00:03.000Z",
      message: "Phase P01 started",
      payload: { demo: true, index: 4 },
    },
    {
      schemaVersion: 1,
      eventId: "22222222-2222-4222-8222-000000000005",
      sequence: 5,
      runId: DEMO_RUN_ID,
      projectId: "tiamat",
      phaseId: "P01",
      type: "test.unit.passed",
      level: "info",
      timestampUtc: "2026-08-02T09:00:04.000Z",
      message: "Unit evidence recorded",
      payload: { demo: true, index: 5 },
    },
    {
      schemaVersion: 1,
      eventId: "22222222-2222-4222-8222-000000000006",
      sequence: 6,
      runId: DEMO_RUN_ID,
      projectId: "tiamat",
      type: "system.info",
      level: "info",
      timestampUtc: "2026-08-02T09:00:05.000Z",
      message: "Structured logger connected",
      payload: { demo: true, index: 6 },
    },
  ];

  return {
    run: {
      runId: DEMO_RUN_ID,
      status: "executing",
      title: "P01 demo run",
      createdAtUtc: createdAt,
      updatedAtUtc: "2026-08-02T09:00:05.000Z",
      metadata: {},
    },
    events,
    artifacts: [
      {
        artifactId: "demo-artifact",
        contentHash: "demo-artifact",
        byteSize: 17,
        mediaType: "text/plain",
        relativePath: "demo/p01.txt",
        createdAtUtc: createdAt,
        metadata: { kind: "demo" },
      },
    ],
    preflight: null,
    workspace: null,
    plan: null,
    architect: null,
    scheduler: null,
    executor: null,
    recoveryOffer: null,
  };
}

function ensureDemoState(): BrowserStoreState {
  const current = loadState();
  if (current.run && current.events.length > 0) {
    return current;
  }
  const demo = buildDemo();
  saveState(demo);
  return demo;
}

export async function browserInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  switch (command) {
    case "get_app_info":
      return {
        name: "Tiamat",
        version: "0.1.0",
        schemaVersion: 1,
        orchestratorMode: "dag-scheduler",
        storeSchemaVersion: 5,
      } as T;
    case "orchestrator_status": {
      const state = ensureDemoState();
      return {
        mode: "dag-scheduler",
        activeRuns: state.scheduler?.activeAttempts ?? 0,
        message: state.scheduler?.paused
          ? "DAG scheduler paused; active attempts retained."
          : "Durable DAG scheduler ready.",
      } as T;
    }
    case "validate_contract_json":
      return {
        valid: true,
        schemaName: String(args?.schemaName ?? ""),
      } as T;
    case "ensure_demo_run": {
      const state = ensureDemoState();
      return {
        run: state.run,
        events: state.events,
        artifacts: state.artifacts,
      } as T;
    }
    case "list_runs": {
      const state = ensureDemoState();
      return (state.run ? [state.run] : []) as T;
    }
    case "replay_events": {
      const state = ensureDemoState();
      const afterSequence = Number(args?.afterSequence ?? 0);
      const runId = String(args?.runId ?? "");
      return state.events.filter(
        (event) => event.runId === runId && event.sequence > afterSequence,
      ) as T;
    }
    case "list_artifacts": {
      const state = ensureDemoState();
      return state.artifacts as T;
    }
    case "transition_run_status": {
      const state = ensureDemoState();
      if (!state.run) {
        throw new Error("no run");
      }
      const newStatus = String(args?.newStatus ?? state.run.status);
      const message = String(args?.message ?? "status changed");
      const eventType = String(args?.eventType ?? "run.status_changed");
      const event = appendEvent(state, eventType, message, {
        status: newStatus,
      });
      state.run = {
        ...state.run,
        status: newStatus,
        updatedAtUtc: event.timestampUtc,
      };
      saveState(state);
      return { run: state.run, event } as TransitionResult as T;
    }
    case "pick_intake_paths": {
      return [] as T;
    }
    case "run_intake_preflight": {
      const state = ensureDemoState();
      const paths = (args?.paths as string[] | undefined) ?? [];
      if (paths.length === 0) {
        throw new Error("no paths selected");
      }
      const report = buildBrowserPreflight(paths);
      const serialized = JSON.stringify(report);
      if (
        serialized.includes("AKIAIOSFODNN7EXAMPLE") ||
        serialized.includes("fixture-secret-value")
      ) {
        throw new Error("secret leak in browser preflight");
      }
      state.preflight = report;
      appendEvent(
        state,
        "intake.preflight_completed",
        `Preflight scanned ${report.manifest.sources.length} source(s); ${report.warnings.length} warning(s); ${report.blockers.length} blocker(s); ${report.secretRisks.length} secret-risk marker(s)`,
        {
          intakeId: report.manifest.intakeId,
          sourceCount: report.manifest.sources.length,
          projectCount: report.manifest.projects.length,
          warningCount: report.warnings.length,
          blockerCount: report.blockers.length,
          secretRiskCount: report.secretRisks.length,
          canStart: report.canStart,
        },
        report.blockers.length > 0 ? "warning" : "info",
      );
      saveState(state);
      return report as T;
    }
    case "confirm_intake_trust": {
      const state = ensureDemoState();
      if (!state.preflight) {
        throw new Error("no preflight report to trust");
      }
      const intakeId = String(args?.intakeId ?? "");
      if (state.preflight.manifest.intakeId !== intakeId) {
        throw new Error("intakeId does not match the latest preflight");
      }
      const acknowledgedUntrusted = Boolean(args?.acknowledgedUntrusted);
      const acknowledgedExecutionRisk = Boolean(
        args?.acknowledgedExecutionRisk,
      );
      const confirmed = acknowledgedUntrusted && acknowledgedExecutionRisk;
      const report: PreflightReport = {
        ...state.preflight,
        trust: {
          confirmed,
          acknowledgedUntrusted,
          acknowledgedExecutionRisk,
        },
        canStart:
          confirmed &&
          state.preflight.blockers.length === 0 &&
          state.preflight.manifest.sources.length > 0,
      };
      state.preflight = report;
      appendEvent(state, "intake.trust_updated", `Trust confirmation ${confirmed ? "accepted" : "incomplete"} (canStart=${report.canStart})`, {
        intakeId: report.manifest.intakeId,
        confirmed: report.trust.confirmed,
        canStart: report.canStart,
      });
      saveState(state);
      return report as T;
    }
    case "get_intake_preflight": {
      const state = ensureDemoState();
      return state.preflight as T;
    }
    case "probe_cursor_capability":
    case "get_cursor_capability":
      return browserCursorCapability() as T;
    case "list_cursor_models":
      return {
        status: "available",
        models: [
          { id: "composer-2.5", label: "composer-2.5" },
          { id: "composer-2.5-fast", label: "composer-2.5-fast" },
          { id: "cursor-grok-4.5-medium", label: "cursor-grok-4.5-medium" },
        ],
        executable: "fixtures/cursor-cli/fake-agent.mjs",
      } as T;
    case "preview_cursor_command": {
      const nested = (args?.args as Record<string, unknown> | undefined) ?? args ?? {};
      const apiKey = String(nested.apiKey ?? "");
      const prompt = String(nested.prompt ?? "");
      const workspace = String(nested.workspace ?? "C:\\managed\\workspace");
      const model = String(nested.model ?? "composer-2.5");
      const argv = [
        "fixtures/cursor-cli/fake-agent.mjs",
        "--print",
        "--output-format",
        "stream-json",
        "--workspace",
        workspace,
        "--trust",
        "--force",
        "--model",
        model,
      ];
      if (apiKey) {
        argv.push("--api-key", "***");
      }
      const stdinPreview = prompt
        .split(apiKey || "___none___")
        .join("***")
        .split("fixture-secret-value")
        .join("***")
        .split("AKIAIOSFODNN7EXAMPLE")
        .join("[REDACTED_AWS_KEY]");
      return {
        argv,
        commandDisplay: argv.join(" "),
        stdinPreview,
        timeoutMs: Number(nested.timeoutMs ?? 120000),
        workspace,
        executable: "fixtures/cursor-cli/fake-agent.mjs",
        spawned: false,
      } as T;
    }
    case "materialize_workspace": {
      const state = ensureDemoState();
      if (!state.preflight?.canStart) {
        throw new Error("preflight cannot start; trust and blockers must clear first");
      }
      const runId = String(args?.runId ?? state.run?.runId ?? DEMO_RUN_ID);
      const sourcePath =
        state.preflight.manifest.sources[0]?.path ?? "C:\\fixture\\demo";
      const fingerprint = {
        path: sourcePath,
        kind: "git",
        head: "browser-fixture-head",
        branch: "main",
        statusPorcelain: "",
        statusHash: "browser-status-hash",
        treeHash: "browser-tree-hash",
        capturedAtUtc: new Date().toISOString(),
      };
      const managedRunRoot = `C:\\managed\\run-${runId.slice(0, 8)}`;
      const projects = state.preflight.manifest.projects.map((project) => ({
        projectId: project.projectId,
        sourceRoot: project.root,
        managedRoot: `${managedRunRoot}\\projects\\${project.projectId}`,
        kind: "gitClone" as const,
        baselineCommit: "browser-baseline",
        baselineBranch: `tiamat/intake-${project.projectId}`,
        worktreePath: `${managedRunRoot}\\projects\\${project.projectId}-worktree`,
        writeRoot: `${managedRunRoot}\\projects\\${project.projectId}-worktree`,
        readRoots: [managedRunRoot],
        sourceFingerprint: { ...fingerprint, path: project.root },
        lockName: `write:${project.projectId}`,
      }));
      const manifest: RunWorkspaceManifest = {
        schemaVersion: 1,
        runId,
        intakeId: state.preflight.manifest.intakeId,
        managedRunRoot,
        controlRoot: `${managedRunRoot}\\control`,
        projects,
        notesRoots: [],
        checkpoints: projects.map((project) => ({
          checkpointId: `cp-${project.projectId}`,
          projectId: project.projectId,
          commit: "browser-baseline",
          branch: project.baselineBranch,
          message: "intake-baseline",
          createdAtUtc: new Date().toISOString(),
        })),
        quarantines: [],
        promotion: { status: "unpromoted" },
        retention: {
          retainUnpromoted: true,
          maxQuarantineEntries: 32,
          allowDestructiveCleanup: false,
        },
        fingerprintPairs: projects.map((project) => ({
          before: project.sourceFingerprint,
          after: project.sourceFingerprint,
          unchanged: true,
        })),
        createdAtUtc: new Date().toISOString(),
        sourceUnchanged: true,
      };
      state.workspace = manifest;
      appendEvent(
        state,
        "workspace.materialized",
        `Materialized ${manifest.projects.length} managed project(s); source_unchanged=true`,
        {
          runId: manifest.runId,
          intakeId: manifest.intakeId,
          managedRunRoot: manifest.managedRunRoot,
          projectCount: manifest.projects.length,
          sourceUnchanged: true,
          promotionStatus: "unpromoted",
        },
      );
      if (state.run) {
        state.run = {
          ...state.run,
          status: "planning",
          updatedAtUtc: new Date().toISOString(),
        };
      }
      saveState(state);
      return manifest as T;
    }
    case "get_workspace_manifest": {
      const state = ensureDemoState();
      return state.workspace as T;
    }
    case "validate_workspace_roots": {
      const state = ensureDemoState();
      if (!state.workspace) {
        throw new Error("no workspace manifest");
      }
      const writeRoots = (args?.writeRoots as string[] | undefined) ?? [];
      const readRoots = (args?.readRoots as string[] | undefined) ?? [];
      const writeErrors = writeRoots.filter(
        (root) =>
          !state.workspace!.projects.some(
            (project) =>
              root.toLowerCase().startsWith(project.writeRoot.toLowerCase()) ||
              root
                .toLowerCase()
                .startsWith(project.managedRoot.toLowerCase()),
          ),
      );
      const readErrors = readRoots.filter(
        (root) =>
          !root
            .toLowerCase()
            .startsWith(state.workspace!.managedRunRoot.toLowerCase()),
      );
      return {
        ok: writeErrors.length === 0 && readErrors.length === 0,
        writeErrors: writeErrors.map((root) => `write root not in managed projects: ${root}`),
        readErrors: readErrors.map((root) => `read root not approved: ${root}`),
      } as T;
    }
    case "create_workspace_checkpoint": {
      const state = ensureDemoState();
      if (!state.workspace) {
        throw new Error("no workspace manifest");
      }
      const projectId = String(args?.projectId ?? "");
      const message = String(args?.message ?? "checkpoint");
      const checkpoint = {
        checkpointId: crypto.randomUUID(),
        projectId,
        commit: `browser-cp-${state.workspace.checkpoints.length + 1}`,
        branch: `tiamat/intake-${projectId}`,
        message,
        createdAtUtc: new Date().toISOString(),
      };
      state.workspace = {
        ...state.workspace,
        checkpoints: [...state.workspace.checkpoints, checkpoint],
      };
      saveState(state);
      return state.workspace as T;
    }
    case "export_workspace_project": {
      const state = ensureDemoState();
      if (!state.workspace) {
        throw new Error("no workspace manifest");
      }
      const projectId = String(args?.projectId ?? "");
      const project = state.workspace.projects.find((p) => p.projectId === projectId);
      if (!project) {
        throw new Error(`unknown project: ${projectId}`);
      }
      const exportDir =
        String(args?.exportDir ?? "").trim() ||
        `${state.workspace.managedRunRoot}\\exports`;
      const exportPath = `${exportDir}\\${projectId}`;
      state.workspace = {
        ...state.workspace,
        promotion: {
          ...state.workspace.promotion,
          status: "exported",
          exportPath,
          promotedAtUtc: new Date().toISOString(),
        },
      };
      saveState(state);
      return state.workspace as T;
    }
    case "promote_workspace": {
      const state = ensureDemoState();
      if (!state.workspace) {
        throw new Error("no workspace manifest");
      }
      const notes = args?.notes != null ? String(args.notes) : undefined;
      state.workspace = {
        ...state.workspace,
        promotion: {
          ...state.workspace.promotion,
          status: "promoted",
          promotedAtUtc: new Date().toISOString(),
          notes: notes ?? state.workspace.promotion.notes,
        },
      };
      saveState(state);
      return state.workspace as T;
    }
    case "run_architect_pipeline": {
      const state = ensureDemoState();
      if (!state.preflight?.canStart) {
        throw new Error("preflight cannot start");
      }
      if (!state.workspace) {
        throw new Error("workspace must be materialized before architect");
      }
      const runId = String(args?.runId ?? state.run?.runId ?? DEMO_RUN_ID);
      const sourcePath =
        state.preflight.manifest.sources[0]?.path.toLowerCase() ?? "";
      const schedulerDemo = sourcePath.includes("scheduler");
      const executorDemo = sourcePath.includes("executor");
      const writeRootA =
        state.workspace.projects[0]?.writeRoot ??
        `${state.workspace.managedRunRoot}\\projects\\repo-a`;
      const writeRootB =
        state.workspace.projects[1]?.writeRoot ??
        `${state.workspace.managedRunRoot}\\projects\\repo-b`;
      const projectA = state.workspace.projects[0]?.projectId ?? "repo-a";
      const projectB = state.workspace.projects[1]?.projectId ?? "repo-b";

      const mkPhase = (
        phaseId: string,
        title: string,
        deps: string[],
        writeRoot: string,
        projectId: string,
        modelTier: "composer" | "grok-low" | "grok-medium" | "grok-high",
        allGates = false,
      ) => ({
        phaseId,
        title,
        objective: `${title} objective`,
        dependencies: deps,
        projectIds: [projectId],
        readRoots: [writeRoot],
        writeRoots: [writeRoot],
        modelTier,
        estimatedMinutes: 10,
        acceptanceCriteria: [
          {
            criterionId: `AC-${phaseId}-01`,
            description: `${phaseId} passes`,
            requiredEvidenceKinds: allGates
              ? (["unit", "integration", "e2e"] as ("unit" | "integration" | "e2e")[])
              : (["unit"] as ("unit")[]),
          },
        ],
        unitTests: [
          {
            testId: `UT-${phaseId}-01`,
            command: allGates ? ["node", "tests/unit.mjs"] : ["npm", "test"],
            workingDirectory: ".",
            timeoutSeconds: 120,
            resourceLocks: [] as string[],
            expected: { exitCode: 0, artifacts: [] as string[] },
            covers: [`AC-${phaseId}-01`],
          },
        ],
        integrationTests: allGates
          ? [
              {
                testId: `IT-${phaseId}-01`,
                command: ["node", "tests/integration.mjs"],
                workingDirectory: ".",
                timeoutSeconds: 120,
                resourceLocks: [] as string[],
                expected: { exitCode: 0, artifacts: [] as string[] },
                covers: [`AC-${phaseId}-01`],
              },
            ]
          : [],
        e2eTests: allGates
          ? [
              {
                testId: `E2E-${phaseId}-01`,
                command: ["node", "tests/e2e.mjs"],
                workingDirectory: ".",
                timeoutSeconds: 120,
                resourceLocks: [] as string[],
                expected: { exitCode: 0, artifacts: [] as string[] },
                covers: [`AC-${phaseId}-01`],
              },
            ]
          : [],
        manualChecks: [],
        rollback: { checkpoint: "intake-baseline", strategy: "restore" as const },
        expectedArtifacts: allGates ? ["src/feature.ts"] : [],
        prompt: `Implement only ${phaseId}.`,
        status: "draft" as const,
        evidence: [],
      });

      const plan: ProjectPlan = schedulerDemo
        ? {
            schemaVersion: 1,
            runId,
            title: "Scheduler multi-repo demo",
            summary: "Parallel, blocked, paused, and escalated scheduling states.",
            assumptions: [],
            risks: [],
            phases: [
              mkPhase("P01", "Repo A slice", [], writeRootA, projectA, "composer"),
              mkPhase("P02", "Repo B slice", [], writeRootB, projectB, "composer"),
              mkPhase(
                "P03",
                "Depends on P01",
                ["P01"],
                writeRootA,
                projectA,
                "grok-low",
              ),
              mkPhase(
                "P04",
                "Escalation candidate",
                [],
                writeRootB,
                projectB,
                "composer",
              ),
            ],
            finalGates: [
              {
                gateId: "FG-01",
                description: "Independent architecture review",
                dependencies: ["P03"],
                requiredEvidenceKinds: ["review"],
              },
            ],
          }
        : {
            schemaVersion: 1,
            runId,
            title: executorDemo ? "Executor fixture" : "Rough-spec notes tool",
            summary: executorDemo
              ? "Fake project with unit/integration/e2e gates"
              : "Turn brainstorm notes into a small testable notes list app.",
            assumptions: ["Desktop-first MVP"],
            risks: ["Ambiguous scope"],
            phases: [
              mkPhase(
                "P01",
                executorDemo
                  ? "Feature vertical slice"
                  : "Notes list vertical slice",
                [],
                writeRootA,
                projectA,
                "composer",
                executorDemo,
              ),
            ],
            finalGates: [
              {
                gateId: "FG-01",
                description: "Independent architecture review",
                dependencies: ["P01"],
                requiredEvidenceKinds: ["review"],
              },
            ],
          };
      // Preserve rough-spec objective text for existing E2E.
      if (!schedulerDemo && !executorDemo && plan.phases[0]) {
        plan.phases[0].objective =
          "Render a notes list from fixture data. Integration tests inapplicable; e2e tests inapplicable until UI host exists.";
        plan.phases[0].title = "Notes list vertical slice";
      }

      const architect: ArchitectRunResult = {
        ok: true,
        runId,
        modelSelection: {
          requestedModel: "gpt-5.6-sol-high",
          selectedModel: "gpt-5.6-sol-high",
          degraded: false,
          reason: "preferred SOL architect model available",
          availableModels: [
            "gpt-5.6-sol-high",
            "cursor-grok-4.5-high",
            "composer-2.5",
          ],
        },
        plan,
        planJsonPath: `${state.workspace.controlRoot}\\.tiamat\\plan.json`,
        masterPlanMdPath: `${state.workspace.controlRoot}\\.tiamat\\MASTER-PLAN.md`,
        hashes: {
          planJsonSha256: "browser-plan-hash",
          masterPlanMdSha256: "browser-md-hash",
        },
        checkpoint: {
          checkpointId: "cp-control-plan",
          projectId: "control",
          commit: "browser-plan-commit",
          branch: "master",
          message: "initial-architect-plan",
          createdAtUtc: new Date().toISOString(),
        },
        attempts: [
          {
            attempt: 1,
            model: "gpt-5.6-sol-high",
            chatId: "chat-architect-valid",
            repaired: false,
            proof: {
              planMode: true,
              force: false,
              autoReview: false,
              workspace: state.workspace.controlRoot,
              argv: [
                "agent",
                "--print",
                "--mode",
                "plan",
                "--model",
                "gpt-5.6-sol-high",
              ],
              model: "gpt-5.6-sol-high",
            },
          },
        ],
        degradedMode: false,
        evidence: ["browser-fake-architect"],
      };
      state.plan = plan;
      state.architect = architect;
      appendEvent(
        state,
        "plan.compiled",
        `Architect plan compiled (phases=${plan.phases.length}; degraded=false)`,
        {
          runId,
          phaseCount: plan.phases.length,
          degradedMode: false,
          selectedModel: "gpt-5.6-sol-high",
        },
      );
      saveState(state);
      return architect as T;
    }
    case "get_project_plan": {
      const state = ensureDemoState();
      return state.plan as T;
    }
    case "get_graph_projection": {
      const state = ensureDemoState();
      if (!state.plan) return null as T;
      const graph = projectGraphFromPlan(state.plan);
      if (state.scheduler) {
        for (const node of graph.nodes) {
          const phase = state.scheduler.phases.find(
            (item) => item.phaseId === node.phaseId,
          );
          if (phase) {
            node.status = phase.status;
            if (phase.selectionReason) {
              node.objective = phase.selectionReason;
            }
          }
        }
      }
      return graph as T;
    }
    case "get_architect_result": {
      const state = ensureDemoState();
      return state.architect as T;
    }
    case "start_scheduler": {
      const state = ensureDemoState();
      if (!state.plan) throw new Error("no compiled plan");
      const runId = String(args?.runId ?? state.plan.runId);
      const maxConcurrent = Number(args?.maxConcurrent ?? 2);
      const phases: SchedulerPhaseView[] = state.plan.phases.map((phase) => ({
        phaseId: phase.phaseId,
        title: phase.title,
        status: phase.dependencies.length === 0 ? "ready" : "draft",
        modelTier: phase.modelTier,
        attemptCount: 0,
        writeRoots: phase.writeRoots,
      }));
      state.scheduler = {
        runId,
        mode: "dag-scheduler",
        paused: false,
        epoch: 1,
        maxConcurrent,
        activeAttempts: 0,
        phases,
        attempts: [],
        heldLocks: [],
      };
      if (state.run) {
        state.run = {
          ...state.run,
          status: "executing",
          updatedAtUtc: new Date().toISOString(),
        };
      }
      appendEvent(state, "scheduler.loaded", `Scheduler loaded ${phases.length} phase(s)`, {
        runId,
        phaseCount: phases.length,
        maxConcurrent,
      });
      saveState(state);
      return state.scheduler as T;
    }
    case "scheduler_tick": {
      const state = ensureDemoState();
      if (!state.scheduler) throw new Error("scheduler not started");
      const snap = state.scheduler;
      snap.epoch += 1;
      if (snap.paused) {
        const result: TickResult = {
          epoch: snap.epoch,
          started: [],
          blocked: snap.phases
            .filter((p) => p.status === "blocked")
            .map((p) => p.phaseId),
          skippedDueToPause: true,
          skippedDueToCapacity: false,
          message: "scheduling paused; active attempts retained",
        };
        saveState(state);
        return result as T;
      }

      const started: string[] = [];
      const heldRoots = new Set(
        snap.phases
          .filter((p) => p.status === "running")
          .flatMap((p) => p.writeRoots.map((r) => r.toLowerCase())),
      );

      for (const phase of snap.phases) {
        if (phase.status !== "ready") continue;
        if (snap.activeAttempts >= snap.maxConcurrent) break;
        const rootBusy = phase.writeRoots.some((r) =>
          heldRoots.has(r.toLowerCase()),
        );
        if (rootBusy) continue;

        const attemptNumber = phase.attemptCount + 1;
        const escalated = attemptNumber > 1;
        const selectedModel = escalated
          ? "cursor-grok-4.5-low"
          : phase.modelTier === "composer"
            ? "composer-2.5"
            : "cursor-grok-4.5-medium";
        const selectionReason = escalated
          ? "escalated to grok-low after timeout; selected cursor-grok-4.5-low"
          : `selected preferred ${selectedModel} for ${phase.modelTier}`;
        const attempt: SchedulerAttemptView = {
          attemptId: crypto.randomUUID(),
          phaseId: phase.phaseId,
          attemptNumber,
          status: "running",
          selectedModel,
          selectionReason,
        };
        phase.status = "running";
        phase.attemptCount = attemptNumber;
        phase.selectedModel = selectedModel;
        phase.selectionReason = selectionReason;
        snap.attempts.push(attempt);
        snap.activeAttempts += 1;
        for (const root of phase.writeRoots) {
          heldRoots.add(root.toLowerCase());
          snap.heldLocks.push(`write:${root.toLowerCase()}`);
        }
        started.push(phase.phaseId);
      }

      // Mark dependents blocked when deps failed.
      for (const phase of snap.phases) {
        if (phase.status === "draft" || phase.status === "ready") {
          const planPhase = state.plan?.phases.find(
            (p) => p.phaseId === phase.phaseId,
          );
          const deps = planPhase?.dependencies ?? [];
          const failedDep = deps.some((depId) =>
            snap.phases.some(
              (p) => p.phaseId === depId && p.status === "failed",
            ),
          );
          const waiting = deps.some((depId) =>
            snap.phases.some(
              (p) =>
                p.phaseId === depId &&
                p.status !== "passed" &&
                p.status !== "skipped",
            ),
          );
          if (failedDep) phase.status = "blocked";
          else if (!waiting && phase.status === "draft") phase.status = "ready";
        }
      }

      snap.heldLocks = [...new Set(snap.heldLocks)].sort();
      state.scheduler = snap;
      appendEvent(
        state,
        "scheduler.tick",
        `Scheduler tick epoch=${snap.epoch} started=${JSON.stringify(started)}`,
        { epoch: snap.epoch, started },
      );
      saveState(state);
      const result: TickResult = {
        epoch: snap.epoch,
        started,
        blocked: snap.phases
          .filter((p) => p.status === "blocked")
          .map((p) => p.phaseId),
        skippedDueToPause: false,
        skippedDueToCapacity: snap.activeAttempts >= snap.maxConcurrent,
        message: "scheduling epoch complete",
      };
      return result as T;
    }
    case "scheduler_complete_attempt": {
      const state = ensureDemoState();
      if (!state.scheduler) throw new Error("scheduler not started");
      const attemptId = String(args?.attemptId ?? "");
      const success = Boolean(args?.success);
      const failureKind = args?.failureKind
        ? String(args.failureKind)
        : undefined;
      const attempt = state.scheduler.attempts.find(
        (item) => item.attemptId === attemptId,
      );
      if (!attempt) throw new Error("attempt not found");
      const phase = state.scheduler.phases.find(
        (item) => item.phaseId === attempt.phaseId,
      );
      if (!phase) throw new Error("phase not found");
      attempt.status = "completed";
      attempt.terminalResult = success ? "succeeded" : "failed";
      state.scheduler.activeAttempts = Math.max(
        0,
        state.scheduler.activeAttempts - 1,
      );
      state.scheduler.heldLocks = state.scheduler.heldLocks.filter(
        (lock) =>
          !phase.writeRoots.some((root) =>
            lock.includes(root.toLowerCase()),
          ),
      );
      if (success) {
        phase.status = "passed";
      } else if (failureKind === "policy") {
        phase.status = "failed";
      } else {
        phase.status = "ready";
        phase.selectionReason = `escalated after ${failureKind ?? "failure"}`;
      }
      // Refresh blocked dependents.
      for (const other of state.scheduler.phases) {
        const planPhase = state.plan?.phases.find(
          (p) => p.phaseId === other.phaseId,
        );
        const deps = planPhase?.dependencies ?? [];
        if (
          deps.some((depId) =>
            state.scheduler!.phases.some(
              (p) => p.phaseId === depId && p.status === "failed",
            ),
          )
        ) {
          other.status = "blocked";
        }
      }
      saveState(state);
      return phase as T;
    }
    case "scheduler_pause": {
      const state = ensureDemoState();
      if (!state.scheduler) throw new Error("scheduler not started");
      state.scheduler.paused = true;
      if (state.run) {
        state.run = {
          ...state.run,
          status: "paused",
          updatedAtUtc: new Date().toISOString(),
        };
      }
      appendEvent(state, "scheduler.paused", "Scheduling paused; active attempts retained", {
        paused: true,
      });
      saveState(state);
      return state.scheduler as T;
    }
    case "scheduler_resume": {
      const state = ensureDemoState();
      if (!state.scheduler) throw new Error("scheduler not started");
      state.scheduler.paused = false;
      if (state.run) {
        state.run = {
          ...state.run,
          status: "executing",
          updatedAtUtc: new Date().toISOString(),
        };
      }
      appendEvent(state, "scheduler.resumed", "Scheduling resumed", {
        paused: false,
      });
      saveState(state);
      return state.scheduler as T;
    }
    case "get_scheduler_snapshot": {
      const state = ensureDemoState();
      return state.scheduler as T;
    }
    case "get_abort_settings": {
      return browserAbortSettings() as T;
    }
    case "acknowledge_degraded_abort": {
      const settings = browserAbortSettings();
      settings.degradedAcknowledged = true;
      settings.degraded = true;
      settings.registered = false;
      settings.collisionReason = settings.collisionReason ?? "browser simulated collision";
      localStorage.setItem("tiamat.p07.abort", JSON.stringify(settings));
      return settings as T;
    }
    case "rebind_abort_shortcut": {
      const shortcut = String(args?.shortcut ?? "Ctrl+Shift+F12");
      const settings = browserAbortSettings();
      settings.shortcut = shortcut;
      settings.registered = false;
      settings.degraded = true;
      settings.degradedAcknowledged = false;
      settings.collisionReason = "rebinding pending native registration";
      localStorage.setItem("tiamat.p07.abort", JSON.stringify(settings));
      return settings as T;
    }
    case "get_process_registry": {
      const state = ensureDemoState();
      const abort = browserAbortSettings();
      const processes = (state as BrowserStoreState & { processes?: unknown[] }).processes ?? [];
      return {
        activeCount: 0,
        processes,
        abort,
        canStart: !abort.degraded || abort.degradedAcknowledged,
        cleanupIncomplete: false,
      } as T;
    }
    case "emergency_abort": {
      const state = ensureDemoState();
      const force = Boolean(args?.force);
      const key = "tiamat.p07.lastAbortPress";
      const prev = Number(localStorage.getItem(key) || "0");
      const now = Date.now();
      const second = !force && prev > 0 && now - prev <= 3000;
      localStorage.setItem(key, String(now));
      const forced = force || second;
      const activeRun = Boolean(
        state.run &&
          !["completed", "failed", "cancelled", "created"].includes(state.run.status),
      );
      if (!activeRun && !forced) {
        return {
          action: "prompt_confirm",
          forced: false,
          activeRun: false,
          message: "No active run. Confirm emergency stop readiness.",
          processesStopped: 0,
          cleanupOk: true,
        } as T;
      }
      if (state.run) {
        state.run = {
          ...state.run,
          status: "cancelled",
          updatedAtUtc: new Date().toISOString(),
        };
      }
      appendEvent(
        state,
        forced ? "process.forced_abort" : "process.emergency_abort",
        forced
          ? "Second-press forced Job Object termination"
          : "Emergency cancellation started (Ctrl+Shift+F12 / UI)",
        { forced, browser: true },
        "warning",
      );
      saveState(state);
      return {
        action: forced ? "force_terminate" : "begin_emergency_cancel",
        forced,
        activeRun,
        message: forced
          ? "Forced abort signaled for 1 process(es)"
          : "Emergency cancel started for 1 process(es)",
        processesStopped: 1,
        cleanupOk: true,
      } as T;
    }
    case "apply_close_policy": {
      const choice = String(args?.choice ?? "keep_running");
      if (choice === "keep_running") {
        localStorage.setItem("tiamat.p07.keepRunning", "1");
        return {
          action: "acknowledged",
          forced: false,
          activeRun: true,
          message: "Keep Tiamat running — work continues in background.",
          processesStopped: 0,
          cleanupOk: true,
        } as T;
      }
      localStorage.removeItem("tiamat.p07.keepRunning");
      return (await browserInvoke("emergency_abort", {
        runId: args?.runId,
        force: true,
      })) as T;
    }
    case "reconcile_processes": {
      return {
        inspected: 0,
        terminated: 0,
        alreadyGone: 0,
        unverifiable: 0,
        interruptedAttempts: 0,
        hardFailure: false,
        messages: ["browser host: nothing to reconcile"],
      } as T;
    }
    case "run_process_fixture": {
      const state = ensureDemoState();
      const mode = String(args?.mode ?? "silent_hang");
      const timedOut = mode !== "resume_success" && mode !== "success";
      appendEvent(state, "watchdog.warning", "Attempt watchdog warning threshold reached", {
        mode,
      });
      if (timedOut) {
        appendEvent(
          state,
          "watchdog.timeout_resume",
          "Timeout resume metadata persisted for same-chat continuation",
          {
            chatId: "chat-timeout-fixture",
            nextModel: "cursor-grok-4.5-low",
            reason: "attempt_watchdog_timeout",
          },
        );
        appendEvent(state, "cleanup.succeeded", "Cleanup proof: zero active Job processes", {
          activeAfter: 0,
          success: true,
        });
      }
      saveState(state);
      return {
        processId: crypto.randomUUID(),
        timedOut,
        cancelled: false,
        killed: timedOut,
        stdout: mode === "partial_timeout" ? "partial edit started" : "",
        stderr: "",
        durationMs: 120,
        chatId: "chat-timeout-fixture",
        resume: timedOut
          ? {
              chatId: "chat-timeout-fixture",
              nextModel: "cursor-grok-4.5-low",
              nextTier: "grok-low",
              reason: "attempt_watchdog_timeout",
              progressUseful: true,
              recoveryPrompt: "Resume the same assigned phase",
            }
          : undefined,
        cleanupOk: true,
        zeroSurvivors: true,
        activeAfterCleanup: 0,
      } as T;
    }
    case "execute_phase_fixture": {
      const state = ensureDemoState();
      if (!state.plan || !state.workspace) {
        throw new Error("plan and workspace required for phase execution");
      }
      const phaseId = String(args?.phaseId ?? "P01");
      const mode = String(args?.mode ?? "impl_success");
      const fail = mode === "impl_fail_tests";
      const escape = mode === "impl_escape";
      const ok = !fail && !escape;
      const layers = [
        {
          kind: "unit",
          required: true,
          executed: 1,
          passed: fail ? 0 : 1,
          failed: fail ? 1 : 0,
          skipped: 0,
          inapplicable: false,
        },
        {
          kind: "integration",
          required: true,
          executed: fail || escape ? 0 : 1,
          passed: fail || escape ? 0 : 1,
          failed: 0,
          skipped: 0,
          inapplicable: false,
        },
        {
          kind: "e2e",
          required: true,
          executed: fail || escape ? 0 : 1,
          passed: fail || escape ? 0 : 1,
          failed: 0,
          skipped: 0,
          inapplicable: false,
        },
      ];
      const checkpoint =
        ok
          ? {
              checkpointId: `cp-${crypto.randomUUID()}`,
              commit: "browser-phase-commit",
              message: `phase ${phaseId} passed gates`,
            }
          : undefined;
      if (ok && state.plan.phases[0]) {
        state.plan = {
          ...state.plan,
          phases: state.plan.phases.map((p) =>
            p.phaseId === phaseId
              ? { ...p, status: "passed" as const, evidence: ["ev-unit", "ev-int", "ev-e2e"] }
              : p,
          ),
        };
        state.workspace = {
          ...state.workspace,
          checkpoints: [
            ...state.workspace.checkpoints,
            {
              checkpointId: checkpoint!.checkpointId,
              projectId: state.workspace.projects[0]?.projectId ?? "app",
              commit: checkpoint!.commit,
              branch: "tiamat/output",
              message: checkpoint!.message,
              createdAtUtc: new Date().toISOString(),
            },
          ],
        };
      }
      const outcome: PhaseExecutionOutcome = {
        ok,
        runId: String(args?.runId ?? state.run?.runId ?? DEMO_RUN_ID),
        phaseId,
        terminalStatus: ok ? "passed" : "failed",
        phaseResult: ok
          ? {
              schemaVersion: 1,
              phaseId,
              status: "passed",
              summary: "browser fake phase passed",
              changedFiles: ["src/feature.ts"],
              evidenceIds: ["ev-unit", "ev-int", "ev-e2e"],
              acceptanceSatisfied: [`AC-${phaseId}-01`],
              artifacts: ["src/feature.ts"],
              immutable: true,
            }
          : undefined,
        evidence: [],
        layers,
        changedFiles: escape ? ["ESCAPE_PROOF.txt"] : ["src/feature.ts"],
        boundaryOk: !escape,
        quarantined: escape
          ? { quarantineId: "q-browser", reason: "boundary escape" }
          : undefined,
        projectCheckpoint: checkpoint,
        controlCheckpoint: ok
          ? {
              checkpointId: "cp-control",
              commit: "browser-control",
              message: "plan projection",
            }
          : undefined,
        planProjected: true,
        message: ok
          ? "Phase passed: gates green, plan projected, checkpoints created"
          : escape
            ? "Out-of-bound edits quarantined; phase failed without checkpoint"
            : "Verification gates failed: unit gate failed",
        evidenceNotes: ok
          ? [
              "plan projected: status=running",
              "plan projected: status=verifying",
              "orchestrator accepted immutable phase-result; plan projected",
            ]
          : ["failed tests prevented pass/checkpoint"],
      };
      state.executor = outcome;
      appendEvent(
        state,
        ok ? "phase.passed" : "phase.failed",
        outcome.message,
        {
          phaseId,
          ok,
          checkpointed: Boolean(checkpoint),
          layers: layers.map((l) => l.kind),
        },
      );
      saveState(state);
      return outcome as T;
    }
    case "get_executor_outcome": {
      const state = ensureDemoState();
      return state.executor as T;
    }
    case "seed_perf_events": {
      const state = ensureDemoState();
      const count = Number(args?.count ?? 0);
      const runId = String(args?.runId ?? state.run?.runId ?? DEMO_RUN_ID);
      const start =
        state.events.reduce((max, event) => Math.max(max, event.sequence), 0) +
        1;
      const seeded: EventEnvelope[] = [];
      const base = Date.now();
      for (let i = 0; i < count; i += 1) {
        const sequence = start + i;
        const phaseId = `P${String((i % 8) + 1).padStart(2, "0")}`;
        const levels = ["debug", "info", "warning", "error"] as const;
        const event: EventEnvelope = {
          schemaVersion: 1,
          eventId: crypto.randomUUID(),
          sequence,
          runId,
          projectId: "tiamat",
          phaseId,
          type: `perf.seed.${(i % 5) + 1}`,
          level: levels[i % 4]!,
          timestampUtc: new Date(base + i).toISOString(),
          message: `perf event ${sequence} phase=${phaseId}`,
          payload: { perf: true, index: sequence, seeded: true },
        };
        seeded.push(event);
      }
      state.events = [...state.events, ...seeded];
      saveState(state);
      return {
        runId,
        seeded: seeded.length,
        totalEvents: state.events.length,
        firstSequence: seeded[0]?.sequence ?? 0,
        lastSequence: seeded.at(-1)?.sequence ?? 0,
      } as T;
    }
    case "emit_event_burst": {
      const state = ensureDemoState();
      const count = Number(args?.count ?? 0);
      const runId = String(args?.runId ?? state.run?.runId ?? DEMO_RUN_ID);
      const started = performance.now();
      const start =
        state.events.reduce((max, event) => Math.max(max, event.sequence), 0) +
        1;
      const emitted: EventEnvelope[] = [];
      const base = Date.now();
      for (let i = 0; i < count; i += 1) {
        const sequence = start + i;
        const phaseId = `P${String((i % 8) + 1).padStart(2, "0")}`;
        const event: EventEnvelope = {
          schemaVersion: 1,
          eventId: crypto.randomUUID(),
          sequence,
          runId,
          projectId: "tiamat",
          phaseId,
          type: `perf.burst.${(i % 5) + 1}`,
          level: "info",
          timestampUtc: new Date(base + i).toISOString(),
          message: `burst event ${sequence}`,
          payload: { burst: true, index: sequence },
        };
        emitted.push(event);
      }
      state.events = [...state.events, ...emitted];
      saveState(state);
      notifyBrowserListeners(emitted);
      return {
        runId,
        emitted: emitted.length,
        events: emitted,
        elapsedMs: Math.round(performance.now() - started),
      } as T;
    }
    case "export_run_report": {
      const state = ensureDemoState();
      const runId = String(args?.runId ?? state.run?.runId ?? DEMO_RUN_ID);
      const report = {
        schemaVersion: 1,
        runId,
        status: state.run?.status ?? "unknown",
        title: state.run?.title ?? "Tiamat run",
        exportedAtUtc: new Date().toISOString(),
        planTitle: state.plan?.title ?? null,
        phaseCount: state.plan?.phases.length ?? 0,
        scheduler: state.scheduler
          ? {
              mode: state.scheduler.mode,
              paused: state.scheduler.paused,
              epoch: state.scheduler.epoch,
              phases: state.scheduler.phases.map((phase) => ({
                phaseId: phase.phaseId,
                status: phase.status,
                attemptCount: phase.attemptCount,
              })),
            }
          : null,
        executorOk: state.executor?.ok ?? null,
        workspaceRoot: state.workspace?.managedRunRoot ?? null,
        processRegistryEmpty: true,
        events: state.events.map((event) => ({
          sequence: event.sequence,
          eventId: event.eventId,
          type: event.type,
          level: event.level,
          timestampUtc: event.timestampUtc,
          phaseId: event.phaseId ?? null,
          message: String(event.message)
            .split("AKIAIOSFODNN7EXAMPLE")
            .join("[REDACTED_AWS_KEY]")
            .split("fixture-secret-value")
            .join("[REDACTED]")
            .split("demo-api-key-should-redact")
            .join("[REDACTED]"),
        })),
      };
      let reportJson = JSON.stringify(report, null, 2);
      for (const secret of [
        "AKIAIOSFODNN7EXAMPLE",
        "fixture-secret-value",
        "fixture-secret-value-do-not-leak",
        "demo-api-key-should-redact",
      ]) {
        if (reportJson.includes(secret)) {
          throw new Error(`refusing export: fixture secret would leak (${secret})`);
        }
      }
      const artifactId = `report-${runId.slice(0, 8)}`;
      state.artifacts = [
        ...state.artifacts,
        {
          artifactId,
          contentHash: artifactId,
          byteSize: reportJson.length,
          mediaType: "application/json",
          relativePath: "reports/run-report.json",
          createdAtUtc: new Date().toISOString(),
          metadata: { kind: "run_report", runId },
        },
      ];
      saveState(state);
      return {
        runId,
        reportJson,
        artifactId,
        relativePath: "reports/run-report.json",
      } as T;
    }
    case "scheduler_retry_phase": {
      const state = ensureDemoState();
      if (!state.scheduler) throw new Error("scheduler not started");
      const requested = args?.phaseId ? String(args.phaseId) : null;
      const target =
        requested ??
        state.scheduler.phases.find((phase) =>
          ["failed", "blocked", "needs_review"].includes(phase.status),
        )?.phaseId;
      if (!target) throw new Error("no failed phase available to retry");
      const phase = state.scheduler.phases.find((item) => item.phaseId === target);
      if (!phase) throw new Error("phase not found");
      phase.status = "ready";
      phase.selectionReason = "manual retry from run controls";
      appendEvent(
        state,
        "phase.retry_requested",
        `Retry requested for ${target}`,
        { phaseId: target },
      );
      saveState(state);
      return state.scheduler as T;
    }
    case "open_run_output": {
      const state = ensureDemoState();
      if (!state.workspace) throw new Error("no managed workspace available");
      return {
        path: state.workspace.managedRunRoot,
        opened: false,
        message: "Output path resolved (open deferred in browser host)",
      } as T;
    }
    case "run_startup_recovery": {
      const state = ensureDemoState();
      const force =
        typeof localStorage !== "undefined" &&
        localStorage.getItem("tiamat.p10.forceRecovery") === "1";
      if (!force && !state.recoveryOffer?.requiresUserChoice) {
        return {
          schemaVersion: 1,
          scannedAtUtc: new Date().toISOString(),
          dbIntegrityOk: true,
          schemaVersionOk: true,
          processReconcile: {
            inspected: 0,
            terminated: 0,
            alreadyGone: 0,
            unverifiable: 0,
            interruptedAttempts: 0,
            hardFailure: false,
            messages: ["browser host: nothing to recover"],
          },
          interruptedAttempts: [],
          unreconciledSideEffects: [],
          lowDisk: false,
          freeDiskBytes: null,
          diskPath: null,
          offer: state.recoveryOffer,
          messages: ["browser host: idle"],
        } as T;
      }
      const offer: RecoveryOffer = {
        offerId: crypto.randomUUID(),
        runId: state.run?.runId ?? DEMO_RUN_ID,
        status: "pending",
        reason: "interrupted run detected — choose Resume or Cancel",
        dbIntegrityOk: true,
        processHardFailure: false,
        interruptedAttemptCount: 1,
        unreconciledSideEffects: 1,
        lowDisk: false,
        details: { browser: true },
        createdAtUtc: new Date().toISOString(),
        requiresUserChoice: true,
        resumeAllowed: true,
      };
      state.recoveryOffer = offer;
      if (state.run) {
        state.run = {
          ...state.run,
          status: "interrupted",
          updatedAtUtc: offer.createdAtUtc,
        };
      }
      appendEvent(
        state,
        "recovery.offer_created",
        "Startup recovery requires Resume or Cancel before new work",
        { offerId: offer.offerId },
      );
      saveState(state);
      return {
        schemaVersion: 1,
        scannedAtUtc: offer.createdAtUtc,
        dbIntegrityOk: true,
        schemaVersionOk: true,
        processReconcile: {
          inspected: 0,
          terminated: 0,
          alreadyGone: 0,
          unverifiable: 0,
          interruptedAttempts: 1,
          hardFailure: false,
          messages: ["browser host: synthetic recovery offer"],
        },
        interruptedAttempts: [
          {
            attemptId: crypto.randomUUID(),
            runId: offer.runId,
            phaseId: "P01",
            priorStatus: "running",
            terminalResult: "lost",
          },
        ],
        unreconciledSideEffects: [{ kind: "git_checkpoint", state: "prepared" }],
        lowDisk: false,
        freeDiskBytes: null,
        diskPath: null,
        offer,
        messages: ["browser host recovery offer ready"],
      } as T;
    }
    case "get_recovery_offer": {
      const state = ensureDemoState();
      return (state.recoveryOffer ?? null) as T;
    }
    case "recovery_resume": {
      const state = ensureDemoState();
      const offer = state.recoveryOffer;
      if (!offer?.requiresUserChoice) throw new Error("no recovery offer");
      if (!offer.resumeAllowed) throw new Error(offer.reason);
      const next: RecoveryOffer = {
        ...offer,
        status: "resumed",
        requiresUserChoice: false,
        resolvedAtUtc: new Date().toISOString(),
        resolution: "resume",
      };
      state.recoveryOffer = next;
      if (state.run) {
        state.run = {
          ...state.run,
          status: "executing",
          updatedAtUtc: next.resolvedAtUtc!,
        };
      }
      appendEvent(state, "recovery.resumed", "User resumed after startup recovery", {
        runId: next.runId,
      });
      saveState(state);
      return next as T;
    }
    case "recovery_cancel": {
      const state = ensureDemoState();
      const offer = state.recoveryOffer;
      if (!offer?.requiresUserChoice) throw new Error("no recovery offer");
      const next: RecoveryOffer = {
        ...offer,
        status: "cancelled",
        requiresUserChoice: false,
        resumeAllowed: false,
        resolvedAtUtc: new Date().toISOString(),
        resolution: "cancel",
      };
      state.recoveryOffer = next;
      if (state.run) {
        state.run = {
          ...state.run,
          status: "cancelled",
          updatedAtUtc: next.resolvedAtUtc!,
        };
      }
      appendEvent(
        state,
        "recovery.cancelled",
        "User cancelled after startup recovery; no new execution",
        { runId: next.runId },
      );
      saveState(state);
      return next as T;
    }
    case "redact_text": {
      const text = String(args?.text ?? "");
      const secrets = [
        "AKIAIOSFODNN7EXAMPLE",
        "fixture-secret-value",
        "fixture-secret-value-do-not-leak",
        "demo-api-key-should-redact",
      ];
      let out = text;
      for (const secret of secrets) {
        out = out.split(secret).join("[REDACTED]");
      }
      out = out.replace(/AKIA[0-9A-Z]{16}/g, "[REDACTED_AWS_KEY]");
      for (const secret of secrets) {
        if (out.includes(secret)) {
          throw new Error("redaction failed to remove fixture secret");
        }
      }
      return {
        text: out,
        originalBytes: text.length,
        redactedBytes: out.length,
        contentHash: "browser-hash",
        replacementCount: text === out ? 0 : 1,
      } as T;
    }
    case "scan_prompt_injection": {
      const text = String(args?.text ?? "").toLowerCase();
      const markers = [
        "ignore previous instructions",
        "expand write roots",
        "disable tests",
      ].filter((m) => text.includes(m));
      return {
        suspicious: markers.length > 0,
        markers,
        message:
          markers.length > 0
            ? `prompt-injection markers detected: ${markers.join(", ")}`
            : "no prompt-injection markers detected",
      } as T;
    }
    case "apply_output_limits_fixture": {
      const text = String(args?.text ?? "");
      const maxLine = Number(args?.maxLineBytes ?? 64 * 1024);
      const maxTotal = Number(args?.maxTotalBytes ?? 2 * 1024 * 1024);
      let kept = text;
      let truncated = false;
      let floodDetected = false;
      if (kept.length > maxLine) {
        kept = `${kept.slice(0, maxLine)}…[LINE_TRUNCATED]\n`;
        truncated = true;
        floodDetected = true;
      }
      if (kept.length > maxTotal) {
        kept = kept.slice(0, maxTotal);
        truncated = true;
        floodDetected = true;
      }
      return {
        text: kept,
        truncated,
        originalBytes: text.length,
        keptBytes: kept.length,
        linesDropped: truncated ? 1 : 0,
        floodDetected,
        message: floodDetected
          ? `output flood/oversized stream truncated: kept ${kept.length}/${text.length} bytes`
          : null,
      } as T;
    }
    case "get_app_settings": {
      return browserAppSettings() as T;
    }
    case "set_cursor_cli_path": {
      const settings = browserAppSettings();
      const path = args?.path;
      settings.cursorCliPath =
        typeof path === "string" && path.trim() ? path.trim() : null;
      settings.updatedAtUtc = new Date().toISOString();
      localStorage.setItem("tiamat.p11.app-settings", JSON.stringify(settings));
      return settings as T;
    }
    case "plan_uninstall_retention": {
      const state = ensureDemoState();
      const hasUnpromoted = Boolean(state.workspace);
      return {
        removeProgramFiles: true,
        removeStartMenuShortcuts: true,
        removeAppDataDb: true,
        removeManagedWorkspaces: !hasUnpromoted,
        retainUnpromotedWorkspaces: hasUnpromoted,
        retainedPaths: hasUnpromoted
          ? [state.workspace?.managedRunRoot ?? "C:\\managed\\unpromoted"]
          : [],
        warnings: hasUnpromoted
          ? ["retaining unpromoted workspace for uninstall safety"]
          : [],
      } as T;
    }
    case "simulate_upgrade_preserve": {
      return {
        dbPreserved: true,
        settingsPreserved: true,
        workspacesPreserved: true,
        previousVersion: String(args?.previousVersion ?? "0.1.0"),
        nextVersion: String(args?.nextVersion ?? "0.1.1"),
        messages: ["upgrade must not rewrite managed workspace roots"],
      } as T;
    }
    case "create_long_path_fixture": {
      const root = String(args?.root ?? "C:\\fixture\\long-path");
      return `${root}\\segment-00\\segment-01\\long-path-marker.txt` as T;
    }
    case "prove_packaged_cleanup": {
      return {
        runId: String(args?.runId ?? DEMO_RUN_ID),
        activeProcessCount: 0,
        zeroOwnedProcesses: true,
        proofs: [],
        artifactPath: String(args?.outDir ?? "C:\\artifacts") + "\\cleanup-proof.json",
      } as T;
    }
    case "materialize_testbench": {
      return {
        destination: String(args?.dest ?? "C:\\fixture\\testbench"),
        longPathMarker: "C:\\fixture\\testbench\\long-path\\.generated\\marker.txt",
        cases: [
          "notes-only",
          "web-app",
          "multi-project",
          "dirty-git",
          "nested-repo",
          "secret-risk",
          "junction-escape",
          "unicode-项目",
          "long-path",
          "executor-app",
        ],
      } as T;
    }
    default:
      throw new Error(`unknown browser command: ${command}`);
  }
}

function browserAppSettings() {
  try {
    const raw = localStorage.getItem("tiamat.p11.app-settings");
    if (raw) return JSON.parse(raw);
  } catch {
    // fall through
  }
  return {
    cursorCliPath: "fixtures/cursor-cli/fake-agent.mjs",
    canaryCapabilityHash: null,
    canaryConsentedAtUtc: null,
    canaryLastSuccessAtUtc: null,
    canaryLastVersion: null,
    updatedAtUtc: "2026-08-02T09:00:00Z",
  };
}

function browserAbortSettings() {
  try {
    const raw = localStorage.getItem("tiamat.p07.abort");
    if (raw) return JSON.parse(raw);
  } catch {
    // fall through
  }
  return {
    shortcut: "Ctrl+Shift+F12",
    registered: true,
    degraded: false,
    degradedAcknowledged: false,
    trayFallbackEnabled: true,
    secondPressForceMs: 3000,
    updatedAtUtc: "2026-08-02T09:00:00.000Z",
  };
}

function browserCursorCapability() {
  return {
    status: "available",
    message: "Browser host uses deterministic fake Cursor capability (no live call).",
    executable: "fixtures/cursor-cli/fake-agent.mjs",
    version: "1.2.3",
    versionRaw: "1.2.3",
    minimumVersion: "0.1.0",
    helpExcerpt:
      "--print --output-format stream-json --workspace --model --list-models --trust --force --resume --mode plan --auto-review",
    features: {
      printMode: true,
      outputFormat: true,
      streamJson: true,
      workspace: true,
      force: true,
      model: true,
      listModels: true,
      trust: true,
      apiKey: true,
      streamPartialOutput: false,
      modePlan: true,
      resume: true,
      autoReview: true,
    },
    auth: "ready",
    authMessage: "Fake CLI status reports ready.",
    models: [
      { id: "gpt-5.6-sol-high", label: "gpt-5.6-sol-high" },
      { id: "composer-2.5", label: "composer-2.5" },
      { id: "composer-2.5-fast", label: "composer-2.5-fast" },
      { id: "cursor-grok-4.5-medium", label: "cursor-grok-4.5-medium" },
      { id: "cursor-grok-4.5-high", label: "cursor-grok-4.5-high" },
    ],
    probedAtUtc: "2026-08-02T09:00:00.000Z",
  };
}

type BrowserListener = (events: EventEnvelope[]) => void;
const listeners = new Set<BrowserListener>();

export function subscribeBrowserEvents(listener: BrowserListener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function notifyBrowserListeners(events: EventEnvelope | EventEnvelope[]): void {
  const batch = Array.isArray(events) ? events : [events];
  if (batch.length === 0) return;
  for (const listener of listeners) {
    listener(batch);
  }
}

export function resetBrowserStoreForTests(): void {
  memoryState = null;
  localStorage.removeItem(STORAGE_KEY);
  localStorage.removeItem("tiamat.p07.abort");
  localStorage.removeItem("tiamat.p07.lastAbortPress");
  localStorage.removeItem("tiamat.p07.keepRunning");
  localStorage.removeItem("tiamat.p10.forceRecovery");
  localStorage.removeItem("tiamat.p11.app-settings");
}

export type { DemoRunSnapshot };
