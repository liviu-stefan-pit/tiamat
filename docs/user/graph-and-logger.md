# Graph and logger

## Phase graph

The center canvas is a **read-only** projection of the canonical plan. You cannot edit dependencies by dragging nodes.

Node states: `draft`, `ready`, `queued`, `running`, `verifying`, `passed`, `failed`, `blocked`, `cancelled`, `skipped`, `needs_review`.

Selecting a node shows objective, acceptance criteria, model/attempt history, write roots, current command/test, artifacts/diffs, timestamps/cost when available, and failure/recovery action.

Controls: zoom, pan, fit, minimap. Running edges animate.

## Activity logger

- Events appear within ~250 ms under normal load.
- Logs are append-only and persisted before UI delivery.
- Reopening the app replays the same ordered events.
- Secrets and credential-like strings are redacted before disk persistence.
- Filters: run, project, phase, attempt, agent, test, stdout, stderr, system.
- Export a redacted run report from the shell when available.

Raw unredacted subprocess streams exist only in bounded memory when necessary and are never shown by default.

## Next

[Pause, cancel, abort](pause-cancel-abort.md)
