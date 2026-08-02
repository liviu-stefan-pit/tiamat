# Troubleshooting

| Symptom | What to do |
|---|---|
| CLI absent | Open Settings → set Cursor CLI path → Save/probe. Start stays disabled until available. |
| Authentication unavailable | Run `agent status` / sign in to Cursor. Planning will not start. |
| Model absent | Tiamat applies recorded allowed fallback. It never substitutes an unapproved model. Check Settings model list. |
| Architect output invalid | One repair resume is attempted; then planning fails with retained evidence. |
| Attempt timeout | Evidence preserved; tree killed/reaped; resume with next tier. At Grok High, one same-tier resume if progress exists, then fail visibly. |
| Tests fail | Workspace and evidence retained; phase does not checkpoint/pass. |
| Out-of-root write | Stop + quarantine + show diff; retry only from clean checkpoint. |
| App restarts mid-run | Use recovery offer: Resume or Cancel. Nothing auto-starts. |
| DB corruption | Preserve copy; use supported recovery; do not guess completion. |
| Disk low | Pause/stop; retain unpromoted work; free space and reopen. |
| Cleanup failure | Critical UI state; retry emergency stop; do not treat run as terminal until cleanup proof exists. |
| Global shortcut conflict | Rebind in Settings or acknowledge degraded abort; tray/UI stop still works. |
| SmartScreen on installer | Expected for unsigned-dev packages; verify hashes in [../release/PACKAGE-HASHES.md](../release/PACKAGE-HASHES.md). |
| Source fingerprint CHANGED | Stop. Investigate accidental mutation. Tiamat should never alter sources; treat as a hard failure. |

## Operator depth

Contributors: see [../contributor/operator-runbook.md](../contributor/operator-runbook.md).
