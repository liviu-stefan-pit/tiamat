# Tiamat documentation

Version **0.1.0** · Release candidate handoff from [P11](../P11-RELEASE-CANDIDATE.md) · Prep artifacts in [release/](release/)

## End-user guides

| Guide | Path |
|---|---|
| Install | [user/install.md](user/install.md) |
| First run | [user/first-run.md](user/first-run.md) |
| Intake and trust | [user/intake-and-trust.md](user/intake-and-trust.md) |
| Start implementation | [user/start-implementation.md](user/start-implementation.md) |
| Graph and logger | [user/graph-and-logger.md](user/graph-and-logger.md) |
| Pause, cancel, global abort | [user/pause-cancel-abort.md](user/pause-cancel-abort.md) |
| Isolated output promotion | [user/promotion-and-export.md](user/promotion-and-export.md) |
| Normal-mode containment limits | [user/containment-limits.md](user/containment-limits.md) |
| Model and cost policy | [user/model-and-cost.md](user/model-and-cost.md) |
| Timeout and resume | [user/timeout-and-resume.md](user/timeout-and-resume.md) |
| Recovery | [user/recovery.md](user/recovery.md) |
| Privacy and security | [user/privacy-and-security.md](user/privacy-and-security.md) |
| Troubleshooting | [user/troubleshooting.md](user/troubleshooting.md) |
| Known limits | [user/known-limits.md](user/known-limits.md) |

## Contributor guides

| Guide | Path |
|---|---|
| Architecture | [contributor/architecture.md](contributor/architecture.md) |
| Contracts | [contributor/contracts.md](contributor/contracts.md) |
| Tests | [contributor/tests.md](contributor/tests.md) |
| Fake CLI | [contributor/fake-cli.md](contributor/fake-cli.md) |
| Packaging | [contributor/packaging.md](contributor/packaging.md) |
| Release | [contributor/release.md](contributor/release.md) |
| Operator runbook | [contributor/operator-runbook.md](contributor/operator-runbook.md) |

## Release preparation

| Artifact | Path |
|---|---|
| Checklist | [release/CHECKLIST.md](release/CHECKLIST.md) |
| Signing disposition | [release/SIGNING.md](release/SIGNING.md) |
| Package hashes | [release/PACKAGE-HASHES.md](release/PACKAGE-HASHES.md) |
| Dependency licenses | [release/reports/DEPENDENCY-LICENSES.md](release/reports/DEPENDENCY-LICENSES.md) |
| Vulnerability report | [release/reports/VULNERABILITY-REPORT.md](release/reports/VULNERABILITY-REPORT.md) |
| Known limitations (release) | [release/KNOWN-LIMITATIONS.md](release/KNOWN-LIMITATIONS.md) |

## New-user TestBench journey

Follow [user/first-run.md](user/first-run.md) using the `executor-app` TestBench case under `fixtures/testbench/executor-app`. Deterministic path uses the fake Cursor CLI; see [contributor/fake-cli.md](contributor/fake-cli.md).

## Docs tooling

Machine-readable inventory: [config/docs-manifest.json](config/docs-manifest.json).

```powershell
npm run test:docs
```

Validates links, documented commands against `package.json`, and config parsers.
