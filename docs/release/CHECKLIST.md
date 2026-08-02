# Release checklist (P12)

Version: **0.1.0** · Date (UTC): **2026-08-02** · Candidate: [P11-RELEASE-CANDIDATE.md](../../P11-RELEASE-CANDIDATE.md)

- [x] Version and changelog finalized (`package.json`, `tauri.conf.json`, workspace Cargo, `CHANGELOG.md`).
- [x] Dependency licenses reviewed (`docs/release/reports/DEPENDENCY-LICENSES.md`).
- [x] Vulnerability scan reviewed (`docs/release/reports/VULNERABILITY-REPORT.md`: npm audit total=0; cargo-audit 0.22.2 vulnerabilities=0 with warning dispositions).
- [x] Rust/TypeScript formatting and lint commands documented and exercised via `npm run ci` / phase evidence.
- [x] Unit, integration, E2E, fault, TestBench, and packaged tests documented; deterministic suites fake-only.
- [x] Job Object leak / cleanup proof path documented (`npm run test:cleanup-proof`).
- [x] Clean install and upgrade policy documented (`scripts/vm/`, packaging module).
- [x] Optional code signing configured **or** unsigned warning documented (`docs/release/SIGNING.md`).
- [x] User and contributor docs validated (`npm run test:docs`, `e2e/p12-new-user.spec.ts`).
- [x] No fixture secrets in documentation examples.
- [x] No active process registry entries required for terminal success (cleanup invariant documented).
- [x] Package hashes recorded (`docs/release/PACKAGE-HASHES.md`).

P13 still owns independent review sign-off before a public release tag.
