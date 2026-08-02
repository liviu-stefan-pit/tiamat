#Requires -Version 5.1
# Deprecated wrapper — prefer `npm run package` (scripts/package.mjs).
$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $RepoRoot
node (Join-Path $PSScriptRoot "package.mjs")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
