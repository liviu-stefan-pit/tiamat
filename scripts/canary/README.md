# Real Cursor contract canary (P11 / P13 release gate)

This harness is **local-live only**. Deterministic CI remains fake-only and must never set the consent flags below.

## What it verifies

For the current Cursor executable version + account capability hash:

1. Stream schema (`stream-json` events parse)
2. Chat-ID extraction
3. Noninteractive approval (`--force` or proven `--auto-review`)
4. Plan mode
5. Prompt transport (stdin / argv)
6. Model-changing resume

It uses a disposable temp workspace and **never touches user project inputs**.

## Spending consent (required)

Both must be set:

```powershell
$env:TIAMAT_LIVE_CANARY = "1"
$env:TIAMAT_CANARY_SPENDING_CONSENT = "I_ACCEPT_CURSOR_SPEND"
```

Optional: `TIAMAT_CANARY_FORCE=1` to re-run even when the version/capability hash already succeeded.

## Run

```powershell
npm run test:canary
# or
powershell -ExecutionPolicy Bypass -File scripts\canary\run-contract-canary.ps1
```

Evidence lands in `artifacts/canary/`.
