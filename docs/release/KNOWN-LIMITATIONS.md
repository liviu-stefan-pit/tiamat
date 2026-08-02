# Known limitations (release)

Tiamat **0.1.0** ships with these explicit limits:

1. **Platform:** Windows 10/11 desktop only.
2. **No cloud workers:** all orchestration is local.
3. **Unsigned packages:** development candidate uses `unsigned-dev` signing disposition.
4. **Normal-mode containment:** not a hostile-code sandbox; prompt/command policy is advisory under `--force`.
5. **No auto-publish:** never force-pushes, opens PRs, deploys, or mutates external systems.
6. **No multi-writer concurrency** on the same writable project root.
7. **Third-party tests** are not guaranteed deterministic.
8. **Graph is read-only;** dependencies cannot be edited in the UI.
9. **Job Object reopen:** after crash, Tiamat cannot reopen a destroyed Job; unverifiable leftovers fail hard.
10. **Paid Cursor canary** is a local spending-consented release gate, never part of deterministic CI.
11. **macOS/Linux** unsupported.
12. **Hostile native binaries** are deliberately not executed in security fixtures.

End-user summary: [../user/known-limits.md](../user/known-limits.md).
