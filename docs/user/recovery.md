# Recovery

## After app restart or crash

On startup Tiamat:

1. Verifies DB integrity and migrations.
2. Reconciles owned processes (PID + creation time + executable identity).
3. Inspects managed clones/worktrees and git state.
4. Marks lost active attempts `interrupted`.
5. Rebuilds phase readiness from durable facts.
6. Offers **Resume** or **Cancel** — no new execution until you choose.

Never infer success only from a commit or process exit; all gates are reconstructed.

## Your choices

| Choice | Effect |
|---|---|
| Resume | Continue interrupted work from reconciled state |
| Cancel | Mark run cancelled; no new scheduling |

## Disk / DB problems

- **Low disk:** scheduling pauses; critical shortage stops safely; unpromoted work is retained.
- **DB corruption:** a copy is preserved; supported recovery is attempted; completion state is never guessed.

## Cleanup failures

If processes cannot be verified reaped, Tiamat shows a critical state, retries identity-safe termination, and prevents the run from becoming a false terminal success.

## Related

[Troubleshooting](troubleshooting.md)
