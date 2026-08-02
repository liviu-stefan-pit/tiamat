#Requires -Version 5.1
param(
  [string]$ArtifactOut,
  [string]$ExePath
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if ([string]::IsNullOrWhiteSpace($ArtifactOut)) {
  $ArtifactOut = Join-Path $RepoRoot "artifacts\cleanup-proof"
}

New-Item -ItemType Directory -Force -Path $ArtifactOut | Out-Null
Set-Location $RepoRoot

$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$env:TIAMAT_CURSOR_CLI = Join-Path $RepoRoot "fixtures\cursor-cli\fake-agent.cmd"
$env:TIAMAT_FAKE_CLI_MODE = "child_tree"

$logPath = Join-Path $ArtifactOut "process_job_object.log"
Write-Host "Running Job Object cleanup fixture (child_tree / ignore_terminate)"
$prevEap = $ErrorActionPreference
$ErrorActionPreference = "Continue"
& cargo test -p tiamat --test process_job_object -- --nocapture *> $logPath
$cargoExit = $LASTEXITCODE
$ErrorActionPreference = $prevEap
if ($cargoExit -ne 0) {
  Get-Content $logPath -ErrorAction SilentlyContinue | Select-Object -Last 40
  throw "process_job_object failed with exit $cargoExit"
}

$suspects = @(Get-CimInstance Win32_Process | Where-Object {
  $_.Name -match "fake-agent|tiamat" -and $_.ProcessId -ne $PID
} | Select-Object ProcessId, Name, CommandLine)
$suspects | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $ArtifactOut "residual-processes.json")

$zero = ($suspects.Count -eq 0)
$summary = [ordered]@{
  zeroOwnedProcesses = $zero
  packagedExe = $ExePath
  completedAtUtc = [DateTime]::UtcNow.ToString("o")
  proofLog = $logPath
}
$summary | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $ArtifactOut "cleanup-proof-summary.json")

if (-not $zero) {
  throw "Residual Tiamat-related processes observed after cleanup fixture"
}
Write-Host "Zero-process cleanup proof OK"
