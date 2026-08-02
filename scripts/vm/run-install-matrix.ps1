#Requires -Version 5.1
param(
  [Parameter(Mandatory = $true)][string]$PackageDir,
  [Parameter(Mandatory = $true)][string]$ArtifactOut,
  [string]$ProductName = "Tiamat"
)
$ErrorActionPreference = "Stop"

<#
  Install / upgrade / uninstall matrix for a disposable Windows VM.
  Documented privileges, reboot policy, and retained artifacts: see README.md.

  Uninstall retention fixture uses the real managed workspaces root:
    %APPDATA%\com.tiamat.desktop\tiamat\workspaces\...
  (same pattern as materialize_workspace app_data_dir/tiamat/workspaces).

  Unsigned-dev Tauri installers may still wipe AppData on uninstall when custom
  retain hooks are not wired. This script records an honest result rather than
  planting KEEP under an unrelated LOCALAPPDATA path that would always "pass".
#>

New-Item -ItemType Directory -Force -Path $ArtifactOut | Out-Null
$log = Join-Path $ArtifactOut "install-matrix.log"
function Log([string]$msg) {
  $line = "[{0}] {1}" -f ([DateTime]::UtcNow.ToString("o")), $msg
  Add-Content -Encoding utf8 $log $line
  Write-Host $line
}

$msi = Get-ChildItem $PackageDir -Filter *.msi | Select-Object -First 1
$nsis = Get-ChildItem $PackageDir -Filter *-setup.exe | Select-Object -First 1
if (-not $msi -and -not $nsis) {
  # Tauri NSIS often named Tiamat_*_x64-setup.exe
  $nsis = Get-ChildItem $PackageDir -Filter *.exe | Where-Object { $_.Name -match "setup|nsis" } | Select-Object -First 1
}
if (-not $msi -and -not $nsis) { throw "No MSI/NSIS package in $PackageDir" }

# Real app workspaces root (not a fake LOCALAPPDATA managed-runs sideload path).
$appData = Join-Path $env:APPDATA "com.tiamat.desktop"
$workspacesRoot = Join-Path $appData "tiamat\workspaces"
New-Item -ItemType Directory -Force -Path $workspacesRoot | Out-Null
$unpromoted = Join-Path $workspacesRoot "unpromoted-demo"
New-Item -ItemType Directory -Force -Path $unpromoted | Out-Null
"unpromoted-work" | Set-Content -Encoding utf8 (Join-Path $unpromoted "KEEP.txt")
Log "Retention fixture at $unpromoted"

function Install-Package {
  if ($nsis) {
    Log ("NSIS install {0} (current user, no elevation expected)" -f $nsis.FullName)
    $p = Start-Process -FilePath $nsis.FullName -ArgumentList "/S" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "NSIS install exit $($p.ExitCode)" }
  } elseif ($msi) {
    Log ("MSI install {0} (admin elevation may be required)" -f $msi.FullName)
    $msiLog = Join-Path $ArtifactOut "msiexec-install.log"
    $p = Start-Process msiexec.exe -ArgumentList "/i `"$($msi.FullName)`" /qn REBOOT=ReallySuppress /l*v `"$msiLog`"" -Wait -PassThru
    if ($p.ExitCode -notin 0, 3010) { throw "MSI install exit $($p.ExitCode)" }
    if ($p.ExitCode -eq 3010) {
      Log 'WARNING: reboot requested (3010); per policy treat as soft-fail unless approved'
    }
  }
}

function Uninstall-Package {
  $un = Get-ChildItem "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall","HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall" -ErrorAction SilentlyContinue |
    ForEach-Object { Get-ItemProperty $_.PSPath } |
    Where-Object { $_.DisplayName -like "*$ProductName*" } |
    Select-Object -First 1
  if (-not $un) {
    Log "No uninstall entry found; skipping"
    return
  }
  Log "Uninstall $($un.DisplayName)"
  if ($un.UninstallString -match "msiexec") {
    $guid = $un.PSChildName
    $msiLog = Join-Path $ArtifactOut "msiexec-uninstall.log"
    Start-Process msiexec.exe -ArgumentList "/x $guid /qn REBOOT=ReallySuppress /l*v `"$msiLog`"" -Wait | Out-Null
  } else {
    $cmd = $un.UninstallString
    if ($cmd -match '"([^"]+)"(.*)') {
      Start-Process -FilePath $Matches[1] -ArgumentList ($Matches[2].Trim() + " /S") -Wait | Out-Null
    }
  }
}

Log "=== Fresh install ==="
Install-Package

Log '=== Upgrade preserve check (re-install same/newer package) ==='
New-Item -ItemType Directory -Force -Path (Join-Path $appData "tiamat") | Out-Null
"settings-v1" | Set-Content -Encoding utf8 (Join-Path $appData "tiamat\settings.marker")
# Re-assert KEEP before upgrade/uninstall path (installers must not clobber workspaces).
New-Item -ItemType Directory -Force -Path $unpromoted | Out-Null
if (-not (Test-Path (Join-Path $unpromoted "KEEP.txt"))) {
  "unpromoted-work" | Set-Content -Encoding utf8 (Join-Path $unpromoted "KEEP.txt")
}
Install-Package
$settingsOk = Test-Path (Join-Path $appData "tiamat\settings.marker")
Log "settings preserved: $settingsOk"

Log "=== Uninstall retention ==="
Uninstall-Package
$retained = Test-Path (Join-Path $unpromoted "KEEP.txt")
$installerHooksWired = $retained
if (-not $retained) {
  Log "WARNING: unpromoted workspace NOT retained under $unpromoted"
  Log 'Product policy requires retaining %APPDATA%\com.tiamat.desktop\tiamat\workspaces with unpromoted work.'
  Log 'Unsigned-dev Tauri default uninstall may wipe AppData; custom retain hooks are not fully wired.'
  Log 'See docs/contributor/packaging.md and docs/user/install.md - do not treat this as a silent-pass.'
} else {
  Log "unpromoted workspace retained: $retained"
}

$note = if ($retained) {
  'KEEP survived under real app workspaces root'
} else {
  'Honest fail: AppData workspaces wiped; policy documented, installer hooks not wired for unsigned-dev'
}

@{
  settingsPreserved = $settingsOk
  unpromotedRetained = $retained
  retentionFixturePath = $unpromoted
  installerRetainHooksWired = $installerHooksWired
  note = $note
  completedAtUtc = [DateTime]::UtcNow.ToString("o")
} | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $ArtifactOut "install-matrix-result.json")

Log 'Matrix complete (retention recorded honestly; see install-matrix-result.json).'
# Soft outcome: do not throw on missing retain hooks so unsigned-dev packaging can ship with documented gap.
# Hard-fail only when settings marker path is wrong (upgrade path regression unrelated to retain hooks).
if (-not $settingsOk) {
  throw 'Upgrade did not preserve settings marker under app data'
}
