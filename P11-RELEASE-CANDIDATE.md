# Tiamat P11 release-candidate checkpoint

- Date (UTC): 2026-08-02
- Phase: P11 Packaging, TestBench, and end-to-end acceptance
- Version: 0.1.0
- Packages (see also `artifacts/packages/` when built locally):
  - `Tiamat_0.1.0_x64-setup.exe` (NSIS) sha256 `216bb9a8da1ca025e19f8d3ef19060a83e335f0427d404d56059d370c74d0ee7`
  - `Tiamat_0.1.0_x64_en-US.msi` (MSI) sha256 `cdbee67986cf95ed9efe6401408db52cc3986bc34205fcb13132a15f6ed4d7b4`
- Zero-process cleanup proof: `artifacts/cleanup-proof/cleanup-proof-summary.json` (`zeroOwnedProcesses=true`)
- Clean-profile smoke: `artifacts/clean-profile/clean-profile-smoke.json`
- Real Cursor contract canary: `artifacts/canary/canary-result.json` (`ok=true`, spendingConsent=true, capabilityHash `5a6424eccc8f20ab51065fd58eb935ca066daf7499e042bf8c4f43edb4783049`)
- Deterministic CI remains fake-only (`.github/workflows/ci.yml` unchanged; canary not in CI)

This checkpoint is the handoff point for P12 (docs/release prep). Full phase evidence is recorded under P11 in `MASTER-PLAN.md`.
