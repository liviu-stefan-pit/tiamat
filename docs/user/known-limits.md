# Known limits (end-user)

v1 intentionally does **not**:

- Run on macOS or Linux desktop.
- Execute in the cloud or on remote worker farms.
- Let you edit the dependency graph manually.
- Automatically push, open PRs, deploy, purchase services, or change external systems.
- Run multiple agents concurrently against the same writable project root.
- Guarantee that arbitrary third-party project tests are deterministic.
- Replace source control, CI, or human product approval.
- Contain deliberately hostile native build scripts (Normal mode is not a hostile-code sandbox).
- Claim it can reopen a destroyed Job Object after crash; unverifiable leftovers are hard failures.

Also note:

- Development installers may be unsigned (SmartScreen warnings).
- Paid Cursor usage requires spending consent and is never part of the deterministic demo path.
- Long paths and junctions are supported only within tested Windows configurations (long-path registry recommended).

Full release list: [../release/KNOWN-LIMITATIONS.md](../release/KNOWN-LIMITATIONS.md).
