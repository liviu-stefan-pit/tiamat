# Install

Windows 10/11 only. WebView2 is required (preinstalled on most Windows 11 systems).

## Packaged install (recommended)

1. Obtain a release package from `artifacts/packages/` (or the published release):
   - NSIS current-user: `Tiamat_0.1.0_x64-setup.exe`
   - MSI: `Tiamat_0.1.0_x64_en-US.msi`
2. Verify SHA-256 against [../release/PACKAGE-HASHES.md](../release/PACKAGE-HASHES.md).
3. Run the installer.
   - NSIS installs for the current user (no elevation).
   - MSI may require elevation for per-machine install.
4. Launch **Tiamat** from the Start menu.

Signing: development packages are **unsigned**. See [../release/SIGNING.md](../release/SIGNING.md). Windows SmartScreen may warn; that is expected for unsigned-dev builds.

## Uninstall

Uninstall from Windows Settings → Apps.

**Retention policy:** managed workspaces with unpromoted work under `%APPDATA%\com.tiamat.desktop\tiamat\workspaces\` must not be deleted silently. Promote or export first if you need a portable copy; see [promotion-and-export.md](promotion-and-export.md).

**Unsigned-dev note:** default Tauri uninstall may still remove AppData when custom retain hooks are not wired. Treat retention as a product requirement verified by policy planners and the VM matrix fixture path — not as a guarantee of the current unsigned installer. See [../contributor/packaging.md](../contributor/packaging.md).

## Upgrade

Upgrade preserves the app database, settings (including configured Cursor CLI path), and managed workspaces. After upgrade, reopen Tiamat and confirm Settings still shows your Cursor CLI path.

## From source (contributors)

```powershell
git clone <repository-url> tiamat
cd tiamat
npm run setup
npm run tauri:dev
```

Prerequisites: Node.js 20+, Rust stable with `cargo`, WebView2.

## Next

[First run](first-run.md)
