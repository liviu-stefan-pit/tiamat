# Timeout and resume

## Watchdog

| Time | Action |
|---|---|
| 8 minutes | `attempt.warning` |
| 10 minutes without completion | Request graceful stop |
| +15 seconds | Terminate the Job Object |
| After kill | Persist stdout/stderr/stream events, chat ID, changed files, git diff, test evidence |

## Resume

Tiamat resumes the **same Cursor chat** with the next allowed model and a recovery prompt when:

- the attempt timed out but made useful progress, or
- the run was interrupted and you choose Resume after startup recovery.

If output is corrupt or violates boundaries, the workspace is quarantined and the phase retries from the prior clean checkpoint.

## Phase failure

After bounded retries a phase fails; dependent phases become blocked; independent phases may continue. Use **Retry failed phase** when appropriate.

## Related

[Model and cost](model-and-cost.md) · [Recovery](recovery.md)
