#Requires -Version 5.1
$ErrorActionPreference = "Stop"

<#
.SYNOPSIS
  One-command deterministic TestBench demo using the fake Cursor CLI only.
  Never performs paid/live Cursor model calls.
#>

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $RepoRoot

$ArtifactRoot = Join-Path $RepoRoot "artifacts\p11-demo"
New-Item -ItemType Directory -Force -Path $ArtifactRoot | Out-Null

Write-Host "=== Tiamat P11 deterministic demo (fake CLI only) ==="

# Materialize fixtures
& (Join-Path $PSScriptRoot "materialize-testbench.ps1") | Out-Host

$fakeCli = Join-Path $RepoRoot "fixtures\cursor-cli\fake-agent.cmd"
if (-not (Test-Path $fakeCli)) { throw "fake CLI missing: $fakeCli" }
$env:TIAMAT_CURSOR_CLI = $fakeCli
$env:TIAMAT_FAKE_CLI_MODE = "success"
$env:TIAMAT_DEMO_FAKE_ONLY = "1"

Write-Host "Fake CLI: $fakeCli"

# Unit + integration regression (deterministic)
Write-Host "Running cargo test --workspace"
cargo test --workspace --quiet
if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

Write-Host "Running frontend unit tests"
npm run test:frontend
if ($LASTEXITCODE -ne 0) { throw "frontend tests failed" }

Write-Host "Running deterministic E2E (fake-only)"
npm run test:e2e
if ($LASTEXITCODE -ne 0) { throw "e2e failed" }

# Process cleanup proof via integration fixture
Write-Host "Proving zero-owned-process cleanup via Job Object fixture"
cargo test -p tiamat --test process_job_object -- --nocapture 2>&1 | Tee-Object -FilePath (Join-Path $ArtifactRoot "process-cleanup.log")
if ($LASTEXITCODE -ne 0) { throw "process cleanup fixture failed" }

$summary = @{
  demo = "p11-deterministic-full-story"
  fakeOnly = $true
  paidModels = $false
  completedAtUtc = [DateTime]::UtcNow.ToString("o")
  artifacts = $ArtifactRoot
  gates = @("cargo test --workspace", "npm run test:frontend", "npm run test:e2e", "process_job_object")
} | ConvertTo-Json -Depth 4
$summary | Set-Content -Encoding utf8 (Join-Path $ArtifactRoot "demo-summary.json")

Write-Host "Demo complete. Summary: $(Join-Path $ArtifactRoot 'demo-summary.json')"
Write-Output $summary
