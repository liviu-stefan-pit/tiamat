# Known limits (end-user)

v1 intentionally does **not**:

- Run on macOS (Windows and Linux only).
- Execute in the cloud or on remote worker farms.
- Expose a phase dependency graph editor in the UI (orchestration remains DAG-backed in the core).
- Automatically push, open PRs, deploy, purchase services, or change external systems.
- Run multiple agents concurrently against the same writable project root.
- Guarantee that arbitrary third-party project tests are deterministic.
- Replace source control, CI, or human product approval.
- Contain deliberately hostile native build scripts (Normal mode is not a hostile-code sandbox).
- On Windows: claim it can reopen a destroyed Job Object after crash; unverifiable leftovers are hard failures.
- On Linux: contain descendants that call `setsid()` (process-group kill is weaker than Windows Job Objects).

Also note:

- Development installers may be unsigned (SmartScreen warnings on Windows).
- Paid Cursor usage requires spending consent and is never part of the deterministic demo path.
- Long paths and junctions are supported only within tested Windows configurations (long-path registry recommended).

Full release list: [../release/KNOWN-LIMITATIONS.md](../release/KNOWN-LIMITATIONS.md).
