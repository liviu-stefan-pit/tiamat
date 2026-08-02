# Tiamat P12 release-preparation handoff

- Date (UTC): 2026-08-02
- Phase: P12 User documentation and release preparation
- Version: 0.1.0
- Upstream candidate: `P11-RELEASE-CANDIDATE.md`
- Docs index: `docs/README.md`
- Manifest: `docs/config/docs-manifest.json` (28 guides, 20 documented commands)
- License: `LICENSE` (MIT)
- Changelog: `CHANGELOG.md` → `[0.1.0] — 2026-08-02`
- Signing disposition: `unsigned-dev` (`docs/release/SIGNING.md`)
- Package hashes (`docs/release/PACKAGE-HASHES.md`), matching P11:
  - `Tiamat_0.1.0_x64-setup.exe` sha256 `216bb9a8da1ca025e19f8d3ef19060a83e335f0427d404d56059d370c74d0ee7`
  - `Tiamat_0.1.0_x64_en-US.msi` sha256 `cdbee67986cf95ed9efe6401408db52cc3986bc34205fcb13132a15f6ed4d7b4`
- License report: `docs/release/reports/DEPENDENCY-LICENSES.md`
- Vulnerability report: `docs/release/reports/VULNERABILITY-REPORT.md` (npm audit total=0; cargo-audit deferred to P13)
- Release checklist: `docs/release/CHECKLIST.md`
- Known limitations: `docs/release/KNOWN-LIMITATIONS.md` / `docs/user/known-limits.md`
- Validation: `npm run test:docs` → `artifacts/docs/docs-validation.json` (`ok=true`)
- New-user TestBench E2E: `e2e/p12-new-user.spec.ts` (fake-only)
- Rust acceptance: `src-tauri/tests/docs_acceptance.rs`

This handoff is the input for P13 independent reviews. Do not mark a public release until P13 completes.
