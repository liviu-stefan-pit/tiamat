# Install

Supported: **Windows 10/11** and **Linux** (Ubuntu 24.04–class with WebKitGTK 4.1).

## Packaged install (recommended)

1. Obtain a release package from GitHub Releases (or `artifacts/packages/`):
   - Windows NSIS current-user: `Tiamat_*_x64-setup.exe`
   - Windows MSI: `Tiamat_*_x64_en-US.msi`
   - Linux AppImage / `.deb` when published for that tag
2. Verify SHA-256 against [../release/PACKAGE-HASHES.md](../release/PACKAGE-HASHES.md) or the release `SHA256SUMS.txt`.
3. Run the installer / AppImage.
   - NSIS installs for the current user (no elevation).
   - MSI may require elevation for per-machine install.
4. Launch **Tiamat**.

Signing: development packages are **unsigned**. See [../release/SIGNING.md](../release/SIGNING.md). Windows SmartScreen may warn; that is expected for unsigned-dev builds.

## Uninstall

- Windows: Settings → Apps.
- Linux: remove the AppImage or uninstall the `.deb` package.

**Retention policy:** managed workspaces with unpromoted work must not be deleted silently. Choose an explicit output folder when running; promote or export first if you need a portable copy. See [promotion-and-export.md](promotion-and-export.md).

## From source (contributors)

```bash
git clone <repository-url> tiamat
cd tiamat
npm run setup
npm run tauri:dev
```

Prerequisites: Node.js 20+, Rust stable with `cargo`, plus platform WebView dependencies.

## Next

[First run](first-run.md)
