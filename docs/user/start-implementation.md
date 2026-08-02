# Start implementation

## Preconditions

- Preflight complete with no blockers.
- Both trust acknowledgments checked.
- Cursor CLI available with a tested noninteractive approval mode (`--force` or proven `--auto-review`).
- Global abort available, or degraded abort explicitly acknowledged.

## What Start does

1. Materializes isolated owned clones/copies (never mutating sources).
2. Runs the architect once (preferred model: `gpt-5.6-sol-high`; falls back to Grok High when SOL is unavailable).
3. Compiles a validated project plan (`.tiamat/plan.json` + `.tiamat/MASTER-PLAN.md`).
4. Schedules independent phases when write roots do not overlap.
5. Executes Cursor CLI agents with cost-aware model routing and verification gates.
6. Streams structured events to the graph and logger.

## While it runs

You can leave the machine unattended. Use [pause / cancel / abort](pause-cancel-abort.md) if you need to stop. Closing the window prompts **Keep Tiamat running** or **Stop all and exit** — there is no silent orphaning.

## After it finishes

Review the completion summary, test evidence, and [promotion/export](promotion-and-export.md) instructions before merging anything into your original repositories.

## Next

[Graph and logger](graph-and-logger.md)
