#Requires -Version 5.1
param(
  [switch]$IsolatedProfile,
  [string]$ArtifactOut,
  [string]$ConfiguredCli
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if ([string]::IsNullOrWhiteSpace($ArtifactOut)) {
  $ArtifactOut = Join-Path $RepoRoot "artifacts\clean-profile"
}
New-Item -ItemType Directory -Force -Path $ArtifactOut | Out-Null

if ($IsolatedProfile) {
  $iso = Join-Path $ArtifactOut "isolated-profile"
  New-Item -ItemType Directory -Force -Path $iso | Out-Null
  $env:APPDATA = Join-Path $iso "Roaming"
  $env:LOCALAPPDATA = Join-Path $iso "Local"
  New-Item -ItemType Directory -Force -Path $env:APPDATA, $env:LOCALAPPDATA | Out-Null
  Write-Host "Using isolated profile under $iso"
}

if ([string]::IsNullOrWhiteSpace($ConfiguredCli)) {
  $ConfiguredCli = Join-Path $RepoRoot "fixtures\cursor-cli\fake-agent.cmd"
}
$env:TIAMAT_CURSOR_CLI = $ConfiguredCli
$env:TIAMAT_FAKE_CLI_MODE = "success"
$env:TIAMAT_DEMO_FAKE_ONLY = "1"

Write-Host "Clean-profile smoke - configured CLI: $ConfiguredCli"

& $ConfiguredCli --version 2>&1 | Tee-Object -FilePath (Join-Path $ArtifactOut "cli-version.txt") | Out-Host
& $ConfiguredCli --help 2>&1 | Tee-Object -FilePath (Join-Path $ArtifactOut "cli-help.txt") | Out-Null

& (Join-Path $RepoRoot "scripts\materialize-testbench.ps1") | Out-Null

$unicode = Get-ChildItem (Join-Path $RepoRoot "fixtures\testbench") -Directory |
  Where-Object { $_.Name -like "unicode*" } |
  Select-Object -First 1
$longManifest = Join-Path $RepoRoot "fixtures\testbench\.generated\materialize-manifest.json"

$result = [ordered]@{
  isolatedProfile = [bool]$IsolatedProfile
  configuredCli = $ConfiguredCli
  unicodeCase = if ($unicode) { $unicode.FullName } else { $null }
  longPathManifest = if (Test-Path $longManifest) { Get-Content $longManifest -Raw | ConvertFrom-Json } else { $null }
  fakeOnly = $true
  completedAtUtc = [DateTime]::UtcNow.ToString("o")
}
$result | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $ArtifactOut "clean-profile-smoke.json")
Write-Host "Clean-profile smoke complete -> $ArtifactOut"
