# Normal-mode containment limits

Tiamat v1 runs in **Normal mode**. Read this before trusting unattended runs against unfamiliar inputs.

## What Normal mode provides

| Control | Effect |
|---|---|
| Owned clones/copies | Agents write only to managed roots; sources are withheld |
| Minimal environment | Controlled env for spawned processes |
| Non-elevated token | No elevation; restricted user rights |
| Windows Job Objects | Kill-on-close, breakaway disabled, process limits |
| Command policy | Allow/deny lists for destructive and out-of-root actions |
| Post-run verification | Source fingerprints + git status must remain unchanged |

## What Normal mode is not

- Prompt/command policy is **advisory** for Cursor invocations that use `--force`, because Cursor's internal tools can still act inside the trusted workspace.
- These controls protect against mistakes and ordinary agent behavior.
- They are **not** a security boundary against deliberately hostile native code, elevated breakaways, or OS escape mechanisms outside the supported worker contract.
- Package lifecycle scripts and project tests are untrusted executable code.

## Hostile inputs

If you do not trust the intake, do not Start. Prefer analyzing hostile material in an external disposable VM. A brokered sandbox worker is a possible post-v1 feature, not a v1 guarantee.

## Related

[Privacy and security](privacy-and-security.md) · [Known limits](known-limits.md)
