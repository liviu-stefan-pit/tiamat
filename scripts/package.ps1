#Requires -Version 5.1
$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $RepoRoot

$OutDir = Join-Path $RepoRoot "artifacts\packages"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

Write-Host "Building Tiamat Windows packages (NSIS + MSI)..."
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
npm run tauri:build
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

function Find-BundleRoot {
  $candidates = @()
  if ($env:CARGO_TARGET_DIR) {
    $candidates += (Join-Path $env:CARGO_TARGET_DIR "release\bundle")
  }
  $candidates += (Join-Path $RepoRoot "src-tauri\target\release\bundle")
  $candidates += (Join-Path $RepoRoot "target\release\bundle")
  # Sandbox / alternate cargo target dirs used by some agent hosts
  $tmp = $env:TEMP
  if ($tmp) {
    Get-ChildItem -Path $tmp -Directory -Filter "cursor-sandbox-cache" -ErrorAction SilentlyContinue |
      ForEach-Object {
        Get-ChildItem $_.FullName -Directory -ErrorAction SilentlyContinue |
          ForEach-Object {
            $candidates += (Join-Path $_.FullName "cargo-target\release\bundle")
          }
      }
  }
  foreach ($c in $candidates) {
    if ($c -and (Test-Path $c)) { return $c }
  }
  return $null
}

$bundleRoot = Find-BundleRoot
if (-not $bundleRoot) {
  throw "bundle output missing after tauri build"
}
Write-Host "Using bundle root: $bundleRoot"

# Clear prior checksums for a fresh stage
$sums = Join-Path $OutDir "SHA256SUMS.txt"
if (Test-Path $sums) { Remove-Item $sums -Force }

$staged = @()
Get-ChildItem -Path $bundleRoot -Recurse -Include *.msi,*.exe | ForEach-Object {
  $dest = Join-Path $OutDir $_.Name
  Copy-Item $_.FullName $dest -Force
  $hash = (Get-FileHash -Algorithm SHA256 $dest).Hash.ToLowerInvariant()
  $kind = if ($_.Extension -eq ".msi") { "msi" } elseif ($_.Name -match "setup") { "nsis" } else { "exe" }
  $staged += [pscustomobject]@{
    path = $dest
    name = $_.Name
    kind = $kind
    sha256 = $hash
    byteSize = $_.Length
  }
  "$hash  $($_.Name)" | Add-Content -Encoding ascii $sums
}

if ($staged.Count -eq 0) {
  throw "No MSI/NSIS artifacts found under $bundleRoot"
}

$manifest = [ordered]@{
  version = (Get-Content (Join-Path $RepoRoot "package.json") -Raw | ConvertFrom-Json).version
  productName = "Tiamat"
  createdAtUtc = [DateTime]::UtcNow.ToString("o")
  signing = "unsigned-dev"
  bundleRoot = $bundleRoot
  artifacts = $staged
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $OutDir "package-manifest.json")
Write-Host "Staged $($staged.Count) package(s) to $OutDir"
$manifest | ConvertTo-Json -Depth 6
