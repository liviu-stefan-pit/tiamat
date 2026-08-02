# Pause, cancel, and global abort

## Pause scheduling

**Pause scheduling** keeps active attempts running but starts no new phases. **Resume** continues the scheduler.

## Cancel run

**Cancel run** stops scheduling and begins cooperative cancellation of owned processes, then Job Object termination after the grace period.

## Emergency stop (UI)

**Emergency stop** uses the same native cancellation path as the global shortcut.

## Global abort — `Ctrl+Shift+F12`

Works even when the Tiamat window is unfocused.

| Action | Behavior |
|---|---|
| First press with active run | Immediate emergency cancellation + visible acknowledgment |
| First press with no active run | Confirmation / countdown only |
| Second press within 3 seconds | Force Job Object termination immediately |

If another app already owns the shortcut, Settings shows degraded status. Rebind the shortcut or acknowledge degraded abort before Start. Tray/UI emergency stop remains available.

## App close

With active work, choose **Keep Tiamat running** or **Stop all and exit**. On crash/OS shutdown, kill-on-close Job Objects terminate descendants.

## Cleanup invariant

A run is not terminal until owned processes are reaped, Job handles closed, and a zero-active-process observation is persisted. Unverifiable cleanup is a hard failure, never a false success.

## Next

[Promotion and export](promotion-and-export.md)
