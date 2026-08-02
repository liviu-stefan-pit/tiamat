# Packaging

## Targets

Configured in `src-tauri/tauri.conf.json` with `"targets": "all"` (platform-native bundles). Product version `0.1.0`, identifier `com.tiamat.desktop`.

```bash
npm run package
```

Runs `scripts/package.mjs`: `tauri build`, then stages installers and SHA-256 sums under `artifacts/packages/`. Signing disposition defaults to `unsigned-dev`.

Windows: NSIS + MSI. Linux: AppImage / deb (and rpm when enabled by the host toolchain).

## GitHub Releases

Pushing a `v*` tag runs `.github/workflows/release.yml` on `windows-latest` and `ubuntu-24.04`, attaching both platforms' artifacts to one GitHub Release via `tauri-apps/tauri-action`.

Bump versions with:

```bash
npm run version -- 0.2.0
```

## Install / upgrade / uninstall policy

Rust module `src-tauri/src/packaging`:

- Upgrade preserves DB and settings.
- **Retention policy:** unmanaged deletion of unpromoted managed workspaces is forbidden. User-chosen output directories hold `run-*` trees; promote or export before wiping.
- Windows VM matrix scripts remain under `scripts/vm/` (PowerShell).

## Disposable VM matrix (Windows)

Do **not** run full install matrix against a contributor profile. Use a snapshotted disposable Windows VM — see `scripts/vm/README.md`.
