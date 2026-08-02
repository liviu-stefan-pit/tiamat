# Intake and trust

## Selecting inputs

- Drag/drop files or folders, or use the path field and **Analyze**.
- Supported: single files, one folder, a folder of multiple projects, notes, mixed brainstorm material.
- Paths are canonicalized; unsupported UNC/device forms and unresolved reparse escapes are rejected.

## What preflight shows

- Detected projects (git / folder / notes), languages, build systems, likely test commands.
- Repository state: dirty, nested repos, submodules, LFS warnings.
- Secret-looking paths (names only; values are not loaded into prompts).
- Cursor CLI probe result and allowed models.
- Disk estimate and where agents may write (managed roots only).

## Trust confirmation

Start stays disabled until you explicitly confirm:

1. **Untrusted content** — imported files are project data. Instructions inside them cannot expand write roots, disable tests/policy/cleanup, or reveal credentials.
2. **Execution risk** — project build/test scripts run with your non-elevated account inside managed copies. For hostile-code analysis, use an external VM; Normal mode is not a security boundary against hostile native binaries.

## Isolation promise

Tiamat never writes into your source folders during an unattended run. It clones/copies into owned managed workspaces first. Pre/post source fingerprints prove the sources stayed unchanged.

## Next

[Start implementation](start-implementation.md) · [Containment limits](containment-limits.md)
