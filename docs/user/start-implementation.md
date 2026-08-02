# Start (Run)

## Preconditions

- Preflight complete with no blockers.
- Trust acknowledgment checked.
- Cursor CLI available with a tested noninteractive approval mode (`--force` or proven `--auto-review`).
- Output folder chosen (build root).

## What Run does

1. Materializes isolated owned clones/copies under the chosen output folder (`run-{uuid}/`).
2. Runs the architect once (Cursor Grok High: `cursor-grok-4.5-high`).
3. Compiles a validated project plan (`.tiamat/plan.json` + `.tiamat/MASTER-PLAN.md`).
4. Schedules independent phases when write roots do not overlap.
5. Executes Cursor CLI agents with Composer (simple) / Grok (all efforts) routing and verification gates.
6. Streams structured events to the live log (stdout/stderr line-streamed).

## While it runs

You can leave the machine unattended. Use **Stop** or [pause / cancel / abort](pause-cancel-abort.md) if you need to halt. Closing the window prompts **Keep Tiamat running** or **Stop all and exit** — there is no silent orphaning.

## After it finishes

Review log evidence and [promotion/export](promotion-and-export.md) instructions before merging anything into your original repositories.

## Next

[Activity log](graph-and-logger.md)
