# Package hashes

Traceable to architect PowerShell `-File` / lone-dash spawn bypass rebuild and staged files under `artifacts/packages/`.

| Artifact | Kind | SHA-256 |
|---|---|---|
| `Tiamat_0.1.0_x64-setup.exe` | NSIS | `e1a0d5cf11eb4d43028eebb4c31e038fd3e2e51e394b0f5e7e1aa872778b5d50` |
| `Tiamat_0.1.0_x64_en-US.msi` | MSI | `0a56e1a6abfa34852546483ef2c61a4848aaec812a6065b122d8b9b4cfa593b1` |

Source `SHA256SUMS.txt`:

```text
0a56e1a6abfa34852546483ef2c61a4848aaec812a6065b122d8b9b4cfa593b1  Tiamat_0.1.0_x64_en-US.msi
e1a0d5cf11eb4d43028eebb4c31e038fd3e2e51e394b0f5e7e1aa872778b5d50  Tiamat_0.1.0_x64-setup.exe
```

Signing: `unsigned-dev` (see [SIGNING.md](SIGNING.md)).

Rebuild timestamp (UTC): **2026-08-02T11:05:14Z** (hosted Cursor spawns unwind `agent.cmd`/`*.ps1` to `node.exe`+`index.js`; strip lone `-`; cmd `/c` payload not double-quoted).

Verify:

```powershell
Get-FileHash -Algorithm SHA256 artifacts\packages\Tiamat_0.1.0_x64-setup.exe
Get-FileHash -Algorithm SHA256 artifacts\packages\Tiamat_0.1.0_x64_en-US.msi
```
