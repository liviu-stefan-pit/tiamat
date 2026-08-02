# Signing disposition

| Field | Value |
|---|---|
| Product | Tiamat 0.1.0 |
| Bundle targets | NSIS (`*-setup.exe`), MSI (`*_en-US.msi`) |
| Disposition | **unsigned-dev** |
| Timestamping | Disabled (`tsp: false` in `tauri.conf.json`) |
| Rationale | P11/P12 release-candidate packages are built for deterministic acceptance without a production code-signing certificate in the contributor environment. |

## User impact

- Windows SmartScreen / Defender SmartScreen may warn on first launch of the NSIS installer.
- Enterprise AppLocker / WDAC policies may block unsigned binaries.
- Hash verification in [PACKAGE-HASHES.md](PACKAGE-HASHES.md) is the integrity check for this candidate.

## Production follow-up (post-P13)

1. Configure Authenticode certificate in the release pipeline.
2. Enable timestamping.
3. Rebuild packages, re-hash, and replace this disposition with `signed` + certificate thumbprint.
4. Do not claim signed status until both NSIS and MSI verify with `Get-AuthenticodeSignature` → `Valid`.
