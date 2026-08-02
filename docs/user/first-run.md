# First run

This guide takes a new user from a fresh install through a safe TestBench journey.

## 1. Confirm Cursor CLI

1. Open **Settings** (header control).
2. Set **Cursor CLI path** if auto-discovery fails. For deterministic practice without paid models, point at the repo fake agent:
   - `fixtures/cursor-cli/fake-agent.cmd` (dev checkout)
3. Click **Save**, then confirm status shows **available**.
4. Note the emergency-stop shortcut (default `Ctrl+Shift+F12`).

Real Cursor: install Cursor and ensure `agent` is on PATH or configure the full executable path. Authentication must succeed before Start is enabled for live runs.

## 2. Materialize TestBench (dev / packaged acceptance)

From a repository checkout:

```powershell
npm run testbench:materialize
```

Use `fixtures/testbench/executor-app` as the intake folder for the first journey.

## 3. Intake → trust → Start

1. Drop or paste the absolute path to `executor-app` into the intake field.
2. Click **Analyze**.
3. Review the preflight card: projects, languages, warnings, disk estimate, Cursor status.
4. Check both trust boxes:
   - acknowledge untrusted content
   - acknowledge execution risk (build/test code runs with your non-elevated account)
5. Click **Start implementation**.

You should see the read-only phase graph and the activity log populate.

## 4. Deterministic full demo (no paid models)

```powershell
npm run demo
```

Runs the fake-CLI full story: unit/integration/E2E gates and process-cleanup proof.

## 5. What success looks like

- Graph shows phases progressing to `passed` / terminal states.
- Activity log shows redacted structured events.
- Isolated workspace panel shows managed roots and **Source fingerprints: unchanged**.
- Completion summary lists tests, promotion instructions, and cleanup confirmation.
- Process registry reports empty after stop.

## Next

[Intake and trust](intake-and-trust.md) · [Start implementation](start-implementation.md)
