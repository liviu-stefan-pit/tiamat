#!/usr/bin/env node
/**
 * Deterministic fake Cursor CLI for Tiamat P03+.
 * Selected via TIAMAT_CURSOR_CLI / wrappers. Mode via TIAMAT_FAKE_CLI_MODE.
 * Never contacts a live Cursor service or paid model.
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync, existsSync, writeSync } from "node:fs";
import { dirname, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const mode = (process.env.TIAMAT_FAKE_CLI_MODE || "success").toLowerCase();
const args = process.argv.slice(2);
const arg0 = (args[0] || "").toLowerCase();

/**
 * Write synchronously to the underlying fd.
 *
 * On Unix, `stream.write` to a pipe is asynchronous, so the `process.exit()`
 * calls throughout this fixture would discard buffered output — silently
 * truncating large payloads such as the `flood_oversized` mode. Writing to the
 * fd directly keeps every mode's output intact on all platforms.
 */
function write(stream, text) {
  const fd = stream === process.stderr ? 2 : 1;
  const buffer = Buffer.from(text, "utf8");
  let offset = 0;
  while (offset < buffer.length) {
    try {
      offset += writeSync(fd, buffer, offset, buffer.length - offset);
    } catch (err) {
      if (err.code === "EAGAIN") continue;
      if (err.code === "EPIPE") return;
      throw err;
    }
  }
}

function streamSuccess(chatId = "chat-fake-001") {
  write(
    process.stdout,
    JSON.stringify({
      type: "system",
      subtype: "init",
      session_id: chatId,
      model: "composer-2.5",
    }) + "\n",
  );
  write(
    process.stdout,
    JSON.stringify({
      type: "assistant",
      message: { content: [{ type: "text", text: "STUB_OK" }] },
    }) + "\n",
  );
  write(
    process.stdout,
    JSON.stringify({
      type: "result",
      subtype: "success",
      session_id: chatId,
      result: "STUB_OK",
      usage: { inputTokens: 12, outputTokens: 4, totalTokens: 16 },
    }) + "\n",
  );
}

function assertArchitectPlanMode(argv) {
  const modeIdx = argv.findIndex((a) => a === "--mode");
  const planMode =
    (modeIdx >= 0 && argv[modeIdx + 1] === "plan") || argv.includes("--plan");
  if (!planMode) {
    write(process.stderr, "architect fake requires --mode plan\n");
    process.exit(11);
  }
  if (argv.includes("--force") || argv.includes("--auto-review")) {
    write(
      process.stderr,
      "architect fake rejects implementation approval flags\n",
    );
    process.exit(12);
  }
}

function validArchitectMarkdown() {
  const projectId = process.env.TIAMAT_FAKE_PLAN_PROJECT_ID || "notes-app";
  const writeRoot =
    process.env.TIAMAT_FAKE_PLAN_WRITE_ROOT ||
    "C:\\\\managed\\\\run\\\\projects\\\\notes-app";
  const readRoot =
    process.env.TIAMAT_FAKE_PLAN_READ_ROOT || writeRoot;
  return `# Rough-spec notes tool

## Summary
Turn brainstorm notes into a small testable notes list app.

## Assumptions
- Desktop-first MVP
- No cloud sync in v1

## Risks
- Ambiguous scope in brainstorm notes

## Phase: P01 — Notes list vertical slice

Deep design: fixture-backed list first; defer sync.

- **phaseId**: P01
- **objective**: Render a notes list from fixture data. Integration tests inapplicable for notes-only MVP shell; e2e tests inapplicable until a UI host exists.
- **dependencies**: none
- **projectIds**: ${projectId}
- **readRoots**: ${readRoot}
- **writeRoots**: ${writeRoot}
- **modelTier**: composer
- **estimatedMinutes**: 10
- **rollbackCheckpoint**: intake-baseline
- **rollbackStrategy**: restore
- **expectedArtifacts**: src/notes.ts

### Acceptance criteria
- \`AC-P01-01\` — Notes list unit test passes against fixture data — evidence: unit

### Unit tests
- \`UT-P01-01\` — command: \`npm\` \`test\` — cwd: \`.\` — timeout: 120 — covers: AC-P01-01

### Integration tests
- (none) — reason: notes-only MVP

### E2E tests
- (none) — reason: no UI host yet

### Manual checks
- (none)

## Final gates
- \`FG-01\` — Independent architecture review — deps: P01 — evidence: review
`;
}

function invalidArchitectMarkdown() {
  return `# Broken plan

## Summary
intentionally invalid

## Phase: P01 — Broken

- **phaseId**: P01
- **objective**: TODO?
- **dependencies**: P01
- **projectIds**: missing-project
- **readRoots**: C:\\\\escape\\\\outside
- **writeRoots**: C:\\\\escape\\\\outside
- **modelTier**: composer
- **estimatedMinutes**: 10
- **rollbackCheckpoint**: x
- **rollbackStrategy**: restore
- **expectedArtifacts**: none

### Acceptance criteria
- (none)

### Unit tests
- (none) — reason: none

### Integration tests
- (none) — reason: none

### E2E tests
- (none) — reason: none

### Manual checks
- (none)

## Final gates
- (none)
`;
}

function streamArchitectPlan(markdown, chatId) {
  const text = "```markdown\n" + markdown + "\n```";
  write(
    process.stdout,
    JSON.stringify({
      type: "system",
      subtype: "init",
      session_id: chatId,
      model: process.env.TIAMAT_FAKE_PLAN_MODEL || "cursor-grok-4.5-high",
    }) + "\n",
  );
  // Control / thinking frames with session_id must be ignored by the extractor.
  write(
    process.stdout,
    JSON.stringify({
      type: "thinking",
      session_id: chatId,
      text: "planning…",
    }) + "\n",
  );
  write(
    process.stdout,
    JSON.stringify({
      type: "tool_call",
      session_id: chatId,
      name: "readFile",
    }) + "\n",
  );
  write(
    process.stdout,
    JSON.stringify({
      type: "assistant",
      message: { content: [{ type: "text", text }] },
    }) + "\n",
  );
  write(
    process.stdout,
    JSON.stringify({
      type: "result",
      subtype: "success",
      session_id: chatId,
      result: "architect-plan",
      usage: { inputTokens: 100, outputTokens: 200, totalTokens: 300 },
    }) + "\n",
  );
}

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

async function main() {
  if (arg0 === "--version") {
    write(process.stdout, "1.2.3\n");
    process.exit(0);
  }

  if (arg0 === "--help") {
    write(
      process.stdout,
      [
        "Usage: agent [options]",
        "  --print",
        "  --output-format <text|json|stream-json>",
        "  --workspace <path>",
        "  --model <id>",
        "  --list-models",
        "  --trust",
        "  --force",
        "  --auto-review",
        "  --resume <chat-id>",
        "  --mode plan",
        "  --plan",
        "  --api-key <key>",
        "  --stream-partial-output",
        "",
        "Commands:",
        "  status",
        "  whoami",
        "",
      ].join("\n"),
    );
    process.exit(0);
  }

  if (arg0 === "--list-models") {
    if (mode === "model_unavailable") {
      write(process.stderr, "model catalog unavailable\n");
      process.exit(2);
    }
    const models =
      mode === "architect_no_sol"
        ? [
            "composer-2.5",
            "composer-2.5-fast",
            "cursor-grok-4.5-low",
            "cursor-grok-4.5-medium",
            "cursor-grok-4.5-high",
            "auto",
          ]
        : [
            "gpt-5.6-sol-high",
            "composer-2.5",
            "composer-2.5-fast",
            "cursor-grok-4.5-low",
            "cursor-grok-4.5-medium",
            "cursor-grok-4.5-high",
            "auto",
          ];
    write(process.stdout, models.join("\n") + "\n");
    process.exit(0);
  }

  if (arg0 === "status" || arg0 === "whoami") {
    if (mode === "auth_failure") {
      write(process.stderr, "not logged in: authentication failure\n");
      process.exit(3);
    }
    write(process.stdout, "logged in as fake-user\nok\n");
    process.exit(0);
  }

  // Consume stdin so callers can pass prompts safely.
  const stdin = await readStdin();

  switch (mode) {
    case "success":
      streamSuccess();
      process.exit(0);
      break;

    case "nonzero_exit":
      write(process.stderr, "fake failure\n");
      write(
        process.stdout,
        JSON.stringify({
          type: "result",
          subtype: "error",
          is_error: true,
          result: "failed",
        }) + "\n",
      );
      process.exit(2);
      break;

    case "malformed_mixed":
      write(
        process.stdout,
        JSON.stringify({ type: "system", session_id: "chat-mixed" }) + "\n",
      );
      write(process.stdout, "NOT JSON <<< garbled\n");
      write(
        process.stdout,
        JSON.stringify({
          type: "assistant",
          message: { content: [{ type: "text", text: "partial" }] },
        }) + "\n",
      );
      write(process.stdout, "{broken\n");
      write(
        process.stdout,
        JSON.stringify({
          type: "result",
          subtype: "success",
          session_id: "chat-mixed",
          usage: { input_tokens: 1, output_tokens: 2 },
        }) + "\n",
      );
      process.exit(0);
      break;

    case "silent_hang":
      await delay(60 * 60 * 1000);
      process.exit(0);
      break;

    case "chatty_hang":
      for (let i = 0; i < 10_000; i += 1) {
        write(process.stdout, `chatty line ${i}\n`);
        await delay(50);
      }
      process.exit(0);
      break;

    case "child_tree": {
      const child = spawn(
        process.execPath,
        [
          "-e",
          "const {spawn}=require('child_process');const g=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{detached:false,stdio:'ignore'});g.unref();setInterval(()=>{},1000);",
        ],
        { stdio: "ignore" },
      );
      write(
        process.stdout,
        JSON.stringify({
          type: "system",
          session_id: "chat-child",
          childPid: child.pid,
        }) + "\n",
      );
      await delay(60 * 60 * 1000);
      process.exit(0);
      break;
    }

    case "ignore_terminate":
      try {
        process.on("SIGTERM", () => {
          write(process.stderr, "ignoring SIGTERM\n");
        });
        process.on("SIGINT", () => {
          write(process.stderr, "ignoring SIGINT\n");
        });
      } catch {
        // Windows may not deliver POSIX signals the same way.
      }
      await delay(60 * 60 * 1000);
      process.exit(0);
      break;

    case "partial_timeout":
      write(
        process.stdout,
        JSON.stringify({
          type: "assistant",
          message: {
            content: [{ type: "text", text: "partial edit started" }],
          },
        }) + "\n",
      );
      await delay(60 * 60 * 1000);
      process.exit(0);
      break;

    case "resume_success": {
      const resumeIdx = args.findIndex((a) => a === "--resume");
      const chatId =
        resumeIdx >= 0 && args[resumeIdx + 1]
          ? args[resumeIdx + 1]
          : "chat-resume";
      streamSuccess(chatId);
      process.exit(0);
      break;
    }

    case "model_unavailable": {
      const modelIdx = args.findIndex((a) => a === "--model");
      const model = modelIdx >= 0 ? args[modelIdx + 1] : "unknown";
      write(
        process.stderr,
        `Model '${model}' is unavailable for this account\n`,
      );
      write(
        process.stdout,
        JSON.stringify({
          type: "result",
          subtype: "error",
          is_error: true,
          result: "model_unavailable",
        }) + "\n",
      );
      process.exit(4);
      break;
    }

    case "auth_failure":
      write(process.stderr, "authentication failure: login required\n");
      write(
        process.stdout,
        JSON.stringify({
          type: "result",
          subtype: "error",
          is_error: true,
          result: "auth_failure",
        }) + "\n",
      );
      process.exit(3);
      break;

    case "flood_oversized": {
      const oversized = "X".repeat(256 * 1024);
      write(process.stdout, oversized + "\n");
      for (let i = 0; i < 200; i += 1) {
        write(process.stdout, `flood-line-${i}-${"y".repeat(200)}\n`);
      }
      write(
        process.stdout,
        JSON.stringify({
          type: "result",
          subtype: "success",
          session_id: "chat-flood",
          usage: { inputTokens: 1, outputTokens: 1, totalTokens: 2 },
        }) + "\n",
      );
      process.exit(0);
      break;
    }

    case "secret_echo": {
      const secret =
        process.env.TIAMAT_FAKE_SECRET || "AKIAIOSFODNN7EXAMPLE";
      write(
        process.stdout,
        JSON.stringify({
          type: "assistant",
          text: `echo secret ${secret} and fixture-secret-value`,
        }) + "\n",
      );
      write(
        process.stdout,
        JSON.stringify({
          type: "result",
          subtype: "success",
          session_id: "chat-secret",
          result: `leaked:${secret}`,
        }) + "\n",
      );
      process.exit(0);
      break;
    }

    case "architect_valid":
    case "architect_no_sol": {
      assertArchitectPlanMode(args);
      streamArchitectPlan(validArchitectMarkdown(), "chat-architect-valid");
      process.exit(0);
      break;
    }

    case "architect_invalid": {
      assertArchitectPlanMode(args);
      streamArchitectPlan(invalidArchitectMarkdown(), "chat-architect-invalid");
      process.exit(0);
      break;
    }

    case "architect_repairable": {
      assertArchitectPlanMode(args);
      const resumeIdx = args.findIndex((a) => a === "--resume");
      if (resumeIdx >= 0) {
        streamArchitectPlan(
          validArchitectMarkdown(),
          args[resumeIdx + 1] || "chat-architect-repair",
        );
      } else {
        streamArchitectPlan(invalidArchitectMarkdown(), "chat-architect-repair");
      }
      process.exit(0);
      break;
    }

    case "impl_success":
    case "impl_fail_tests":
    case "impl_escape":
    case "impl_timeout_partial":
    case "impl_resume": {
      await runImplementationMode(mode, args, stdin);
      break;
    }

    default:
      write(process.stderr, `unknown fake mode: ${mode}\n`);
      // Still acknowledge stdin length for debugging without echoing secrets.
      write(
        process.stdout,
        JSON.stringify({
          type: "system",
          stdinBytes: Buffer.byteLength(stdin),
          mode,
        }) + "\n",
      );
      process.exit(1);
  }
}

main().catch((err) => {
  write(process.stderr, String(err) + "\n");
  process.exit(1);
});

function workspaceFromArgs(argv) {
  const idx = argv.findIndex((a) => a === "--workspace");
  if (idx >= 0 && argv[idx + 1]) return argv[idx + 1];
  return process.env.TIAMAT_FAKE_WRITE_ROOT || process.cwd();
}

function phaseResultPayload(phaseId, changedFiles, status = "passed") {
  return {
    schemaVersion: 1,
    phaseId,
    status,
    summary:
      status === "passed"
        ? "Fixture implementation completed all assigned work"
        : "Fixture implementation reported failure",
    changedFiles,
    evidenceIds: ["ev-unit", "ev-int", "ev-e2e"],
    acceptanceSatisfied: ["AC-P01-01"],
    artifacts: changedFiles,
    notes: ["fake-cli"],
    immutable: true,
    progressUseful: true,
  };
}

function streamPhaseResult(result, chatId) {
  const text = "```json\n" + JSON.stringify(result, null, 2) + "\n```";
  write(
    process.stdout,
    JSON.stringify({
      type: "system",
      subtype: "init",
      session_id: chatId,
      model: "composer-2.5",
    }) + "\n",
  );
  write(
    process.stdout,
    JSON.stringify({
      type: "assistant",
      message: { content: [{ type: "text", text }] },
    }) + "\n",
  );
  // Also emit a raw phase-result object line for robust orchestrator extraction.
  write(process.stdout, JSON.stringify(result) + "\n");
  write(
    process.stdout,
    JSON.stringify({
      type: "result",
      subtype: "success",
      session_id: chatId,
      result: "phase-result",
      usage: { inputTokens: 20, outputTokens: 40, totalTokens: 60 },
    }) + "\n",
  );
}

async function runImplementationMode(mode, argv, _stdin) {
  const workspace = workspaceFromArgs(argv);
  const phaseId = process.env.TIAMAT_FAKE_PHASE_ID || "P01";
  const managedRun =
    process.env.TIAMAT_FAKE_MANAGED_RUN_ROOT || dirname(workspace);

  if (mode === "impl_escape") {
    const escapePath = join(managedRun, "ESCAPE_PROOF.txt");
    writeFileSync(escapePath, "out of bounds\n", "utf8");
    // Also claim the escape in the payload so boundary checks see it.
    const result = phaseResultPayload(phaseId, [escapePath], "passed");
    streamPhaseResult(result, "chat-impl-escape");
    process.exit(0);
    return;
  }

  if (mode === "impl_timeout_partial") {
    const partial = join(workspace, "src", "partial.ts");
    mkdirSync(dirname(partial), { recursive: true });
    writeFileSync(partial, "export const partial = true;\n", "utf8");
    write(
      process.stdout,
      JSON.stringify({
        type: "assistant",
        message: {
          content: [{ type: "text", text: "partial edit started" }],
        },
      }) + "\n",
    );
    write(
      process.stdout,
      JSON.stringify({
        type: "system",
        subtype: "init",
        session_id: "chat-impl-partial",
      }) + "\n",
    );
    await delay(60 * 60 * 1000);
    process.exit(0);
    return;
  }

  if (mode === "impl_resume") {
    const feature = join(workspace, "src", "feature.ts");
    mkdirSync(dirname(feature), { recursive: true });
    writeFileSync(
      feature,
      "export function greet(name) { return `hi ${name}`; }\n",
      "utf8",
    );
    // Ensure gate scripts exist for resume completion.
    ensureGateScripts(workspace, mode === "impl_fail_tests");
    streamPhaseResult(
      phaseResultPayload(phaseId, ["src/feature.ts", "src/partial.ts"]),
      "chat-impl-resume",
    );
    process.exit(0);
    return;
  }

  // impl_success / impl_fail_tests
  const feature = join(workspace, "src", "feature.ts");
  mkdirSync(dirname(feature), { recursive: true });
  writeFileSync(
    feature,
    "export function greet(name) { return `hi ${name}`; }\n",
    "utf8",
  );
  ensureGateScripts(workspace, mode === "impl_fail_tests");
  streamPhaseResult(
    phaseResultPayload(phaseId, ["src/feature.ts"]),
    mode === "impl_fail_tests" ? "chat-impl-fail" : "chat-impl-success",
  );
  process.exit(0);
}

function ensureGateScripts(workspace, failUnit) {
  const unit = join(workspace, "tests", "unit.mjs");
  const integration = join(workspace, "tests", "integration.mjs");
  const e2e = join(workspace, "tests", "e2e.mjs");
  mkdirSync(dirname(unit), { recursive: true });
  if (!existsSync(unit) || failUnit) {
    writeFileSync(
      unit,
      failUnit
        ? "console.error('unit fail'); process.exit(1);\n"
        : "import { readFileSync } from 'node:fs';\nconst src = readFileSync(new URL('../src/feature.ts', import.meta.url), 'utf8');\nif (!src.includes('export function greet')) { console.error('missing greet'); process.exit(1); }\nconsole.log('unit ok');\n",
      "utf8",
    );
  }
  if (!existsSync(integration)) {
    writeFileSync(
      integration,
      "import { readFileSync } from 'node:fs';\nconst src = readFileSync(new URL('../src/feature.ts', import.meta.url), 'utf8');\nif (!src.includes('greet')) { console.error('missing greet'); process.exit(1); }\nconsole.log('integration ok');\n",
      "utf8",
    );
  }
  if (!existsSync(e2e)) {
    writeFileSync(
      e2e,
      "import { existsSync } from 'node:fs';\nimport { join, dirname } from 'node:path';\nimport { fileURLToPath } from 'node:url';\nconst root = join(dirname(fileURLToPath(import.meta.url)), '..');\nif (!existsSync(join(root, 'src', 'feature.ts'))) { console.error('missing feature'); process.exit(1); }\nconsole.log('e2e ok');\n",
      "utf8",
    );
  }
}
