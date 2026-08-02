# Operator runbook

Deep companion to [../user/troubleshooting.md](../user/troubleshooting.md) and MASTER-PLAN §23.

## CLI / auth / models

1. Resolution order: configured path → `TIAMAT_CURSOR_CLI` → PATH (`agent` / `cursor-agent`) → known install paths.
2. Probe `--version`, `--help`, `--list-models` before unattended runs.
3. Auth failures stop before planning; show `agent status` guidance.
4. Missing preferred models → recorded Composer/Grok fallback only.

## Architect failures

One repair resume on invalid plan output, then fail with retained evidence. Never invent a plan.

## Attempt lifecycle

Prepared → executing → observed → reconciled with idempotency keys. After timeout: preserve evidence, kill/reap tree, escalate model per policy.

## Boundary violations

Out-of-root write → stop, quarantine, show diff, retry only from clean checkpoint.

## Startup

Integrity check → process reconcile → workspace inspect → mark interrupted → offer Resume/Cancel. Never auto-start.

## Cleanup

Terminal run states require persisted zero-active-process observation while Job handles remain open, then successful handle closure. Unverifiable cleanup blocks completion.

## Disk / DB

Pause on low disk; preserve DB copies on corruption; never guess phase completion.

## Artifacts to collect

- Redacted run export
- `artifacts/cleanup-proof/cleanup-proof-summary.json`
- Package `SHA256SUMS.txt`
- Canary `artifacts/canary/canary-result.json` (live gate only)
- VM matrix logs under `artifacts/vm/<run-id>/` when applicable
