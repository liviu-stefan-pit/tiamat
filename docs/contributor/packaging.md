# Packaging

## Targets

Configured in `src-tauri/tauri.conf.json`:

- Windows: `nsis`, `msi`
- Linux: `deb`, `appimage`

Product version from `tauri.conf.json` / workspace Cargo version; identifier `com.tiamat.desktop`.

```bash
npm run package
```

Runs `scripts/package.mjs`: `tauri build`, then stages installers and SHA-256 sums under `artifacts/packages/`. Signing disposition defaults to `unsigned-dev`.

## GitHub Releases

Pushing a `v*` tag (for example `v0.1.0`) runs `.github/workflows/release.yml`:

1. Creates a GitHub Release for the tag
2. Builds on Windows and Ubuntu 24.04 in parallel
3. Uploads all platform artifacts into that one release

Bump versions with:

```bash
npm run version -- 0.2.0
```

Then commit, tag `v0.2.0`, and push with tags.

## Install / upgrade / uninstall policy

Rust module `src-tauri/src/packaging`:

- Upgrade preserves DB and settings.
- **Retention policy:** unmanaged deletion of unpromoted managed workspaces is forbidden. User-chosen output directories hold `run-*` trees; promote or export before wiping.
- Windows VM matrix scripts remain under `scripts/vm/` (PowerShell).

## Disposable VM matrix (Windows)

Do **not** run full install matrix against a contributor profile. Use a snapshotted disposable Windows VM — see `scripts/vm/README.md`.
