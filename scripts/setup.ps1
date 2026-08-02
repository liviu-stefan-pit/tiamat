$ErrorActionPreference = "Stop"

Write-Host "Tiamat setup — verifying toolchain and installing dependencies"

function Require-Command($name) {
  if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
    throw "Required command '$name' is not available on PATH."
  }
}

Require-Command node
Require-Command npm

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Host "Rust not found. Install via https://rustup.rs/ then rerun setup."
  throw "cargo is required"
}

Require-Command cargo
rustup component add rustfmt clippy | Out-Host

$nodeVersion = node -p "process.versions.node"
Write-Host "Node.js $nodeVersion"
Write-Host "Rust $(rustc --version)"
Write-Host "Cargo $(cargo --version)"

npm ci
npx playwright install chromium | Out-Host

Write-Host "Running workspace verification"
npm run ci

Write-Host "Setup complete."
