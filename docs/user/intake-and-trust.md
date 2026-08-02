# Intake and trust

## Selecting inputs

- Drag/drop files or folders, or use **Pick files** / **Pick folder**, or paste a path and press Enter.
- Supported: single files, one folder, a folder of multiple projects, notes, mixed brainstorm material.
- Paths are canonicalized; unsupported UNC/device forms and unresolved reparse escapes are rejected.

## What preflight shows

- Detected projects (git / folder / notes), languages, build systems, likely test commands.
- Blockers and warnings (limits, nested repos, secret-risk markers by path/hash only).
- Cursor CLI probe result when available.

## Trust confirmation

**Run** stays disabled until you acknowledge that sources will be read and agents will run commands in the chosen output folder.

## Isolation promise

Tiamat never writes into your source folders during an unattended run. It clones/copies into owned managed workspaces under the output folder you choose (`run-{uuid}/`). Pre/post source fingerprints prove the sources stayed unchanged.

## Next

[Start (Run)](start-implementation.md) · [Containment limits](containment-limits.md)
