# Release

## Version source of truth

Keep these aligned at **0.1.0** for the P11/P12 candidate:

- `package.json` → `version`
- `src-tauri/tauri.conf.json` → `version`
- `Cargo.toml` workspace package version
- `CHANGELOG.md`
- `docs/release/PACKAGE-HASHES.md`

## Prep checklist

Follow [../release/CHECKLIST.md](../release/CHECKLIST.md). Produce:

1. Changelog entry for the version.
2. Dependency license report.
3. Vulnerability scan disposition.
4. Signing disposition.
5. Package hashes matching staged installers.
6. Docs validation (`npm run test:docs`) and new-user E2E.
7. P12 handoff checkpoint (`P12-RELEASE-PREP.md`).

## Commands

```powershell
npm run package
npm run test:docs
npm run test:all
npm run test:packaged
npm run test:cleanup-proof
# Local-live release gate only:
npm run test:canary
```

## Traceability

Release-prep artifacts must cite the candidate package hashes from P11 (`P11-RELEASE-CANDIDATE.md`) unless packages are rebuilt; if rebuilt, regenerate hashes and update both the release docs and MASTER-PLAN evidence.

P13 owns independent review and final release handoff after this prep is exact.
