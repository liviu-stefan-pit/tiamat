# Tiamat P13 release handoff

- Date (UTC): 2026-08-02
- Phase: P13 Independent final reviews, remediation, and release handoff
- Version: 0.1.0
- Base commit (pre-implementation tree): `50032dbee01a131e67423a2bab603f11d31cd675`
- Working tree: full P00–P13 implementation pending release commit (hashes below are of rebuilt packages after remediation)
- Signing: `unsigned-dev` (`docs/release/SIGNING.md`)

## Independent reviews

| Lane | Agent | Verdict | Evidence |
|---|---|---|---|
| Architecture / contracts / data / docs | `692ec0d4-607d-484b-a43f-1df58dfad23f` | FAIL → remediated | `artifacts/p13/reviews/ARCH-CODE-DATA-DOCS.md` |
| Reliability / security / Job / release | `e0c75b8b-f41f-490d-89c7-c23f08697b69` | FAIL → remediated | `artifacts/p13/reviews/REL-SEC-JOB-RELEASE.md` |

## Remediation + fresh verification

| Batch | Remediator | Verifier | Result |
|---|---|---|---|
| Contracts/Arch/Data | `3c691dd0-e97c-460f-94a4-d75fe2f46489` | `a54eae80-ac6b-4a79-b6a2-bed7e4d4c148` | PASS |
| Security/Docs/UI | `f5f5b44c-b615-4c27-b001-f80b12d60706` | `14d35db2-7cb0-44f4-9702-eec81bd2a106` | PASS |
| ProcessHost | `1b9c4592-f13b-459c-bded-56f567270465` | `5e21f193-8807-4ce1-92a4-8416d9a9a0e4` | FAIL (PARTIAL) → follow-up |
| REL-001 + DATA-002 | `a5a79201-d05f-4d83-a6be-1b417a0d4833` | `fdd75da1-ac49-405c-9fdf-2df890276580` | PASS |

**Critical/high findings remaining: none.**

## Advisories

- `cargo audit` 0.22.2: **0 vulnerabilities** (17 warning-class dispositions in `docs/release/reports/VULNERABILITY-REPORT.md`)
- `npm audit`: **0 vulnerabilities**

## Package hashes (post-remediation rebuild 2026-08-02T09:40:41Z)

- `Tiamat_0.1.0_x64-setup.exe` sha256 `e1a0d5cf11eb4d43028eebb4c31e038fd3e2e51e394b0f5e7e1aa872778b5d50`
- `Tiamat_0.1.0_x64_en-US.msi` sha256 `0a56e1a6abfa34852546483ef2c61a4848aaec812a6065b122d8b9b4cfa593b1`

Recorded in `docs/release/PACKAGE-HASHES.md`, `docs/config/docs-manifest.json`, `artifacts/packages/SHA256SUMS.txt`.

## Suite evidence (`artifacts/p13/suites/`)

| Suite | Result | Log / artifact |
|---|---|---|
| `npm run ci` (fmt/lint/unit/docs) | PASS | `ci.log` |
| `npm run test:e2e` | PASS (27) | `e2e.log` (Vite watch ignores fixtures junction loop) |
| Fault / recovery | PASS (10) | `fault.log` |
| Zero-owned cleanup | PASS (`zeroOwnedProcesses=true`) | `cleanup-proof.log` + `artifacts/cleanup-proof/` |
| Packaged clean-profile | PASS | `packaged.log` |
| Install/upgrade/uninstall matrix | PASS | `upgrade.log` + `install-matrix/install-matrix-result.json` |
| TestBench materialize | PASS | `testbench.log` |
| Docs validation | PASS | `docs.log` / `artifacts/docs/docs-validation.json` |
| Package rebuild | PASS | `package.log` |

## Upstream handoffs

- `P12-RELEASE-PREP.md` (docs prep; historical P11 package hashes)
- `P11-RELEASE-CANDIDATE.md` (historical candidate packages)

This handoff is the P13 completion gate for unsigned-dev release readiness. Public signed release still requires Authenticode + re-audit on the release commit.
