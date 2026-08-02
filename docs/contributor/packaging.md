# Packaging

## Targets

Configured in `src-tauri/tauri.conf.json`: NSIS + MSI, product version `0.1.0`, identifier `com.tiamat.desktop`.

```powershell
npm run package
```

Stages installers and SHA-256 sums under `artifacts/packages/` via `scripts/package.ps1`. Signing disposition defaults to `unsigned-dev`.

## Install / upgrade / uninstall policy

Rust module `src-tauri/src/packaging`:

- Upgrade preserves DB, settings, workspaces under `%APPDATA%\com.tiamat.desktop\` (app data).
- **Retention policy:** unmanaged deletion of unpromoted managed workspaces is forbidden. Managed runs live under `%APPDATA%\com.tiamat.desktop\tiamat\workspaces\` (same root `materialize_workspace` uses). `plan_uninstall_retention` lists paths that must be kept.
- **Installer hooks (honest status):** unsigned-dev NSIS/MSI builds do **not** yet ship fully wired custom actions that exclude `tiamat\workspaces\` from AppData wipe. Product policy and the VM matrix fixture use the real workspaces path; `install-matrix-result.json` records `unpromotedRetained` / `installerRetainHooksWired` without planting KEEP under an unrelated `LOCALAPPDATA\tiamat-managed-runs` path. Do not claim installer-level retention is proven until hooks are wired and the matrix reports `installerRetainHooksWired: true`.
- Cleanup proof helpers write `artifacts/cleanup-proof/`.

## Disposable VM matrix

Do **not** run full install matrix against a contributor profile. Use a snapshotted disposable Windows VM:

```powershell
# On the VM after restoring tiamat-p11-base:
powershell -ExecutionPolicy Bypass -File scripts\vm\run-install-matrix.ps1 `
  -PackageDir \\host\share\artifacts\packages `
  -ArtifactOut C:\tiamat-artifacts
```

Details: `scripts/vm/README.md`.

## Local subset without MSI

```powershell
npm run test:packaged
npm run test:cleanup-proof
```

Uses isolated APPDATA redirect (`-IsolatedProfile`).
