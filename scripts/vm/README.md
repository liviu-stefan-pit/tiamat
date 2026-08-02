# Disposable Windows VM runner (P11)

Tiamat install/upgrade/uninstall acceptance runs in a **disposable snapshotted Windows VM**, never by mutating a contributor workstation.

## Declared base image

| Field | Value |
|---|---|
| OS | Windows 11 Pro 23H2 (or Windows 10 22H2) |
| vCPU / RAM | 4 vCPU / 8 GB (same perf reference as P09) |
| Disk | 80 GB fixed VHDX |
| WebView2 | Evergreen Runtime preinstalled |
| Long paths | `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\LongPathsEnabled=1` |
| Snapshot | `tiamat-p11-base` taken after toolchain-free clean desktop |

## Privileges

| Step | Account | Elevation |
|---|---|---|
| Fresh NSIS current-user install | Standard user | Not required |
| MSI per-machine install | Local admin | UAC elevation required |
| Uninstall retention check | Same user that installed | Match installer mode |
| Global shortcut / Job Object cleanup | Standard user | Not required |
| Snapshot restore | Hypervisor admin | Outside guest |

## Snapshot / reboot policy

1. Restore `tiamat-p11-base` before every matrix run.
2. Reboot only when the installer explicitly requests it (MSI `REBOOT=ReallySuppress` preferred; if reboot is forced, restore snapshot afterward and treat as failure unless documented).
3. Never leave the VM dirty between scenarios; always revert to snapshot.
4. Captured crash dumps under `C:\tiamat-artifacts\dumps` survive only until the next restore — copy them out before reverting.

## Retained artifacts (copied out of the guest)

Staging host directory: `artifacts/vm/<run-id>/`

- Installer logs (`%TEMP%\Tiamat*.log`, MSI `/l*v`)
- Package copies + `SHA256SUMS.txt`
- App DB after upgrade (`%APPDATA%\com.tiamat.desktop\tiamat\`)
- Managed workspace roots listing (prove unpromoted retention)
- Cleanup proof JSON (`cleanup-proof-*.json`)
- Process snapshot (`Get-CimInstance Win32_Process` filtered)
- Global shortcut smoke transcript
- Unicode / long-path intake transcript

## Matrix

```powershell
# On the disposable VM after restoring tiamat-p11-base:
powershell -ExecutionPolicy Bypass -File scripts\vm\run-install-matrix.ps1 `
  -PackageDir \\host\share\artifacts\packages `
  -ArtifactOut C:\tiamat-artifacts
```

Scenarios:

1. Clean-profile smoke (isolated `%APPDATA%` / `%LOCALAPPDATA%`)
2. Configured CLI discovery via Settings path
3. Unicode + long-path intake
4. Global shortcut abort while window unfocused
5. Packaged stop/exit → zero owned processes
6. Upgrade preserve DB/settings/workspaces
7. Uninstall retention fixture under real `%APPDATA%\com.tiamat.desktop\tiamat\workspaces\` (records honest `unpromotedRetained` / `installerRetainHooksWired`; see packaging.md)

## Contributor machines

Do **not** run `run-install-matrix.ps1` against a developer profile. Use `scripts\vm\clean-profile-smoke.ps1` locally only with `-IsolatedProfile` (temp APPDATA redirect) for a subset of checks without MSI install.
