#Requires -Version 5.1
# Deprecated wrapper — prefer `npm run setup` (scripts/setup.mjs).
$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $RepoRoot
node (Join-Path $PSScriptRoot "setup.mjs")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
