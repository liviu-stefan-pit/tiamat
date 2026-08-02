#Requires -Version 5.1
$ErrorActionPreference = "Stop"

<#
.SYNOPSIS
  Materialize TestBench sample workspaces with git baselines, nested repos,
  junction escape attempt, and a generated long-path leaf.
#>

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$Bench = Join-Path $RepoRoot "fixtures\testbench"
$Generated = Join-Path $Bench ".generated"

function Init-GitRepo([string]$Path, [string]$Message) {
  if (-not (Test-Path $Path)) { throw "missing $Path" }
  Push-Location $Path
  try {
    if (-not (Test-Path ".git")) {
      git init -q
      git config user.email "testbench@tiamat.local"
      git config user.name "Tiamat TestBench"
      git add -A
      git commit -q -m $Message
    }
  } finally {
    Pop-Location
  }
}

Write-Host "Materializing TestBench under $Bench"

New-Item -ItemType Directory -Force -Path $Generated | Out-Null

Init-GitRepo (Join-Path $Bench "web-app") "testbench web-app baseline"
Init-GitRepo (Join-Path $Bench "multi-project\repo-a") "repo-a baseline"
Init-GitRepo (Join-Path $Bench "multi-project\repo-b") "repo-b baseline"
Init-GitRepo (Join-Path $Bench "dirty-git") "dirty-git clean baseline"
Init-GitRepo (Join-Path $Bench "nested-repo\outer") "outer baseline"
Init-GitRepo (Join-Path $Bench "nested-repo\outer\inner") "inner nested baseline"
Init-GitRepo (Join-Path $Bench "executor-app") "executor baseline"

# Leave dirty-git dirty after baseline
$dirtyExtra = Join-Path $Bench "dirty-git\src\untracked-extra.ts"
"export const extra = true;" | Set-Content -Encoding utf8 $dirtyExtra
Add-Content -Encoding utf8 (Join-Path $Bench "dirty-git\src\index.ts") "`nexport const dirty = true;"

# Junction escape (Windows)
$junction = Join-Path $Bench "junction-escape\safe\escape-link"
$target = Join-Path $Bench "junction-escape"
if (Test-Path $junction) {
  cmd /c "rmdir `"$junction`"" | Out-Null
}
cmd /c "mklink /J `"$junction`" `"$target`"" | Out-Host

# Long path via cargo/unit helper equivalent
$longRoot = Join-Path $Generated "long-path"
New-Item -ItemType Directory -Force -Path $longRoot | Out-Null
$current = $longRoot
for ($i = 0; $i -lt 20; $i++) {
  $current = Join-Path $current ("segment-{0:D2}-abcdefghijklmnopqrstuvwxyz" -f $i)
  New-Item -ItemType Directory -Force -Path $current | Out-Null
}
$marker = Join-Path $current "long-path-marker.txt"
"tiamat-long-path-ok" | Set-Content -Encoding utf8 $marker
$len = $marker.Length
if ($len -lt 240) {
  throw "expected long path >= 240 chars, got $len"
}

$manifest = @{
  materializedAtUtc = [DateTime]::UtcNow.ToString("o")
  cases = @(
    "notes-only","web-app","multi-project","dirty-git","nested-repo",
    "secret-risk","junction-escape","unicode-项目","long-path","executor-app"
  )
  longPathMarker = $marker
  longPathLength = $len
  junction = $junction
} | ConvertTo-Json -Depth 4
$manifest | Set-Content -Encoding utf8 (Join-Path $Generated "materialize-manifest.json")

Write-Host "TestBench materialize complete."
Write-Host "Long path length: $len"
Write-Output $manifest
