# Activity log

The bottom pane is the live activity logger. There is no phase graph in the main UI; progress is reported as structured events.

## Behavior

- Events appear within ~250 ms under normal load.
- Logs are append-only and persisted before UI delivery.
- Reopening the app replays the same ordered events.
- Secrets and credential-like strings are redacted before disk persistence.
- Filter by level and free-text search (type, message, phase, attempt).
- Export a redacted JSON snapshot from the log toolbar.

Raw unredacted subprocess streams exist only in bounded memory when necessary and are never shown by default.

Status line above the log summarizes run state (idle / running / phase counts) from the orchestrator.

## Next

[Pause, cancel, abort](pause-cancel-abort.md)
