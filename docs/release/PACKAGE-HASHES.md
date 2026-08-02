# Package hashes

Traceable to P13 final review handoff (`P13-RELEASE-HANDOFF.md`) and staged files under `artifacts/packages/`.

| Artifact | Kind | SHA-256 |
|---|---|---|
| `Tiamat_0.1.0_x64-setup.exe` | NSIS | `1a3b92779c381bf7d9bfa2d544c04255a28dda085f19b96328bd66768146648b` |
| `Tiamat_0.1.0_x64_en-US.msi` | MSI | `04299e8145f1750f49fef6c3a555cf47d783f5d6bf4dc2efa949f85c58f25b57` |

Source `SHA256SUMS.txt`:

```text
04299e8145f1750f49fef6c3a555cf47d783f5d6bf4dc2efa949f85c58f25b57  Tiamat_0.1.0_x64_en-US.msi
1a3b92779c381bf7d9bfa2d544c04255a28dda085f19b96328bd66768146648b  Tiamat_0.1.0_x64-setup.exe
```

Signing: `unsigned-dev` (see [SIGNING.md](SIGNING.md)).

Rebuild timestamp (UTC): **2026-08-02T09:40:41Z** (post-P13 remediation).

Verify:

```powershell
Get-FileHash -Algorithm SHA256 artifacts\packages\Tiamat_0.1.0_x64-setup.exe
Get-FileHash -Algorithm SHA256 artifacts\packages\Tiamat_0.1.0_x64_en-US.msi
```
