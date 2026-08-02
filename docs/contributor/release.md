# Release

## Version source of truth

Keep these aligned (use `npm run version -- X.Y.Z`):

- `package.json` → `version`
- `src-tauri/tauri.conf.json` → `version`
- root `Cargo.toml` → `[workspace.package].version`
- `CHANGELOG.md`
- `docs/release/PACKAGE-HASHES.md` (after you stage packages)

## Automatic GitHub Release (Windows + Linux)

1. Bump version and commit on `main`.
2. Tag and push:

```bash
git tag v0.1.0
git push origin main --tags
```

3. Workflow [`.github/workflows/release.yml`](../../.github/workflows/release.yml):
   - Creates (or reuses) a GitHub Release for that tag
   - Builds on `windows-latest` and `ubuntu-24.04`
   - Uploads NSIS, MSI, deb, and AppImage assets to the same release

Re-run from **Actions → Release → Run workflow** with the tag name if a platform job failed.

Repo setting required: **Settings → Actions → General → Workflow permissions → Read and write permissions** (so `GITHUB_TOKEN` can create releases).

## Local packaging

```bash
npm run package
npm run test:docs
```

Stages installers under `artifacts/packages/` with SHA-256 sums. Windows VM matrix helpers remain under `scripts/vm/`.

## Prep checklist

Follow [../release/CHECKLIST.md](../release/CHECKLIST.md) for changelog, licenses, signing disposition, and hashes when cutting a formal candidate.
