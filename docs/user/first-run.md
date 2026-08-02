# First run

This guide takes a new user from a fresh install through a safe TestBench journey.

## 1. Confirm Cursor CLI

Tiamat probes for the Cursor agent CLI on PATH. For deterministic practice without paid models, set `TIAMAT_CURSOR_CLI` / configure the fake agent:

- `fixtures/cursor-cli/fake-agent.mjs` (cross-platform; wrap with `node|…` where required)
- Windows convenience wrapper: `fixtures/cursor-cli/fake-agent.cmd`

Real Cursor: install Cursor and ensure `agent` is on PATH. Authentication must succeed before live runs.

## 2. Materialize TestBench (dev / packaged acceptance)

From a repository checkout:

```bash
npm run testbench:materialize
```

Use `fixtures/testbench/executor-app` as the intake folder for the first journey.

## 3. Input → trust → output → Run

1. Drop or paste the absolute path to `executor-app` into **Input**.
2. Review preflight summary: projects, blockers, warnings.
3. Acknowledge the single trust checkbox when required.
4. Choose an **Output** folder (build root; Tiamat creates `run-{id}/` under it).
5. Click **Run**.

Watch the live **Log** for architect / phase / process events. Use **Stop** to cancel.

## 4. Deterministic full demo (no paid models)

```bash
npm run demo
```

Runs the fake-CLI full story: unit/integration/E2E gates and process-cleanup proof (Windows helpers where noted).

## 5. What success looks like

- Log shows architect plan compile then phase progress to terminal states.
- Output folder contains the managed `run-*` workspace.
- Process registry reports empty after stop.

## Next

[Intake and trust](intake-and-trust.md) · [Start implementation](start-implementation.md)
