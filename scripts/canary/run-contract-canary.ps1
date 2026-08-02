#Requires -Version 5.1
$ErrorActionPreference = "Stop"

<#
.SYNOPSIS
  Spending-consented, version-gated real Cursor contract canary.
  Never run from deterministic CI.
#>

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$ArtifactOut = Join-Path $RepoRoot "artifacts\canary"
New-Item -ItemType Directory -Force -Path $ArtifactOut | Out-Null

function Fail([string]$msg) {
  @{ ok = $false; error = $msg; at = [DateTime]::UtcNow.ToString("o") } |
    ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $ArtifactOut "canary-result.json")
  throw $msg
}

if ($env:TIAMAT_LIVE_CANARY -ne "1") {
  Fail "Refusing canary: set TIAMAT_LIVE_CANARY=1 (local-live only; never CI)."
}
if ($env:TIAMAT_CANARY_SPENDING_CONSENT -ne "I_ACCEPT_CURSOR_SPEND") {
  Fail "Refusing canary: set TIAMAT_CANARY_SPENDING_CONSENT=I_ACCEPT_CURSOR_SPEND"
}

# Resolve real CLI (not fake). Prefer .cmd/.exe over .ps1 wrappers.
$agent = $null
$candidates = @(
  $env:TIAMAT_CURSOR_CLI,
  $env:CURSOR_CLI_PATH,
  (Join-Path $env:LOCALAPPDATA "cursor-agent\agent.cmd"),
  (Join-Path $env:LOCALAPPDATA "cursor-agent\agent.exe"),
  (Join-Path $env:LOCALAPPDATA "cursor-agent\cursor-agent.cmd"),
  (Get-Command agent.cmd -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source),
  (Get-Command agent -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source),
  (Get-Command cursor-agent -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
)
foreach ($cand in $candidates) {
  if (-not $cand) { continue }
  if ($cand -match "fake-agent") { continue }
  if (-not (Test-Path $cand)) { continue }
  if ($cand -match '\.ps1$') { continue }
  $agent = $cand
  break
}
if (-not $agent) { Fail "Real Cursor CLI not found (refusing fake-agent for live canary)." }

Write-Host "Canary agent: $agent"

$versionRaw = & $agent --version 2>&1 | Out-String
$version = ([regex]::Match($versionRaw, "\d+(?:\.\d+){1,3}")).Value
if (-not $version) { $version = $versionRaw.Trim() }
$help = & $agent --help 2>&1 | Out-String
$models = & $agent --list-models 2>&1 | Out-String

$helpPath = Join-Path $ArtifactOut "help.txt"
$modelsPath = Join-Path $ArtifactOut "models.txt"
$versionPath = Join-Path $ArtifactOut "version.txt"
$help | Set-Content -Encoding utf8 $helpPath
$models | Set-Content -Encoding utf8 $modelsPath
$versionRaw | Set-Content -Encoding utf8 $versionPath

$status = ""
try { $status = & $agent status 2>&1 | Out-String } catch { $status = "$_" }
$status | Set-Content -Encoding utf8 (Join-Path $ArtifactOut "status.txt")

$features = @{
  print = $help -match "--print"
  streamJson = $help -match "stream-json"
  workspace = $help -match "--workspace"
  force = $help -match "--force"
  autoReview = $help -match "--auto-review"
  trust = $help -match "--trust"
  model = $help -match "--model"
  resume = $help -match "--resume"
  plan = ($help -match "--mode") -or ($help -match "--plan")
  listModels = $help -match "--list-models"
}

if (-not $features.print -or -not $features.streamJson) {
  Fail "CLI missing required stream-json/--print contract surface"
}
if (-not ($features.force -or $features.autoReview)) {
  Fail "CLI missing noninteractive approval (--force or --auto-review)"
}
if (-not $features.plan) { Fail "CLI missing plan mode" }
if (-not $features.resume) { Fail "CLI missing --resume" }

# Capability hash: version + feature flags + truncated model list
$hashInput = "$version|$($features | ConvertTo-Json -Compress)|$($models.Substring(0, [Math]::Min(500, $models.Length)))"
$sha = [System.Security.Cryptography.SHA256]::Create()
$hashBytes = $sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($hashInput))
$capabilityHash = ($hashBytes | ForEach-Object { $_.ToString("x2") }) -join ""

$gateFile = Join-Path $ArtifactOut "canary-gate.json"
$prior = $null
if (Test-Path $gateFile) {
  $prior = Get-Content $gateFile -Raw | ConvertFrom-Json
}
if ($prior -and $prior.capabilityHash -eq $capabilityHash -and $prior.ok -eq $true -and $env:TIAMAT_CANARY_FORCE -ne "1") {
  Write-Host "Version/capability hash already canaried successfully: $capabilityHash"
  @{
    ok = $true
    skipped = $true
    reason = "version-gated cache hit"
    capabilityHash = $capabilityHash
    version = $version
    agent = $agent
    completedAtUtc = [DateTime]::UtcNow.ToString("o")
  } | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $ArtifactOut "canary-result.json")
  exit 0
}

# Disposable workspace — never user input
$work = Join-Path $ArtifactOut ("workspace-" + [guid]::NewGuid().ToString("n").Substring(0, 8))
New-Item -ItemType Directory -Force -Path $work | Out-Null
"# canary" | Set-Content -Encoding utf8 (Join-Path $work "README.md")

# Pick cheapest available model from preferred list
$preferred = @("composer-2.5", "composer-2", "auto")
$model = $null
foreach ($p in $preferred) {
  if ($models -match [regex]::Escape($p)) { $model = $p; break }
}
if (-not $model) {
  $first = ($models -split "`r?`n" | Where-Object { $_.Trim() -ne "" } | Select-Object -First 1)
  if ($first) { $model = ($first -split "\s+")[0] }
}
if (-not $model) { Fail "No models available from --list-models" }

Write-Host "Canary model: $model"

function Invoke-AgentCapture([string[]]$Argv, [string]$Stdin, [string]$OutName) {
  $outFile = Join-Path $ArtifactOut $OutName
  $stdoutPath = Join-Path $ArtifactOut ($OutName + ".stdout")
  $stderrPath = Join-Path $ArtifactOut ($OutName + ".stderr")

  Push-Location $work
  try {
    # Native .cmd invocation via call operator; prompt as final argv element.
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    if ($Stdin) {
      $Stdin | & $agent @Argv > $stdoutPath 2> $stderrPath
    } else {
      & $agent @Argv > $stdoutPath 2> $stderrPath
    }
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $prev
  } finally {
    Pop-Location
  }

  $stdout = if (Test-Path $stdoutPath) { Get-Content $stdoutPath -Raw -ErrorAction SilentlyContinue } else { "" }
  $stderr = if (Test-Path $stderrPath) { Get-Content $stderrPath -Raw -ErrorAction SilentlyContinue } else { "" }
  if ($null -eq $stdout) { $stdout = "" }
  if ($null -eq $stderr) { $stderr = "" }

  @"
EXIT=$exitCode
---STDOUT---
$stdout
---STDERR---
$stderr
"@ | Set-Content -Encoding utf8 $outFile
  return @{ exitCode = $exitCode; stdout = [string]$stdout; stderr = [string]$stderr; file = $outFile }
}

$approval = @()
if ($features.force) { $approval += @("--force") }
elseif ($features.autoReview) { $approval += @("--auto-review") }

# 1) Plan-mode probe (tiny disposable prompt)
$planArgs = @("--print", "--output-format", "stream-json", "--workspace", $work, "--model", $model)
if ($features.trust) { $planArgs += "--trust" }
if ($help -match "--mode") { $planArgs += @("--mode", "plan") }
elseif ($help -match "--plan") { $planArgs += "--plan" }
$planArgs += $approval
$planArgs += "Reply with a one-line JSON object {`"canary`":true} only. Do not modify files."

$plan = Invoke-AgentCapture -Argv $planArgs -Stdin "" -OutName "plan-stream.txt"
$chatId = $null
foreach ($line in ($plan.stdout -split "`r?`n")) {
  if ($line -match '"session_id"\s*:\s*"([^"]+)"') { $chatId = $Matches[1]; break }
  if ($line -match '"chat_id"\s*:\s*"([^"]+)"') { $chatId = $Matches[1]; break }
  if ($line -match '"conversation_id"\s*:\s*"([^"]+)"') { $chatId = $Matches[1]; break }
}
# Also accept tool events that embed ids
if (-not $chatId -and $plan.stdout -match '(chat|session|conversation)[_-]?id["\s:=]+([a-zA-Z0-9_-]{6,})') {
  $chatId = $Matches[2]
}

# 2) Model-changing resume if chat id available; otherwise second fresh call with alternate model
$resumeModel = $null
foreach ($p in @("composer-2.5-fast", "composer-2.5", $model)) {
  if ($p -ne $model -and $models -match [regex]::Escape($p)) { $resumeModel = $p; break }
}
if (-not $resumeModel) { $resumeModel = $model }

$resumeOut = $null
if ($chatId -and $features.resume) {
  $resumeArgs = @("--print", "--output-format", "stream-json", "--workspace", $work, "--resume", $chatId, "--model", $resumeModel)
  if ($features.trust) { $resumeArgs += "--trust" }
  $resumeArgs += $approval
  $resumeArgs += "Acknowledge resume with {`"resumed`":true}. Do not modify files."
  $resumeOut = Invoke-AgentCapture -Argv $resumeArgs -Stdin "" -OutName "resume-stream.txt"
} else {
  $resumeArgs = @("--print", "--output-format", "stream-json", "--workspace", $work, "--model", $resumeModel)
  if ($features.trust) { $resumeArgs += "--trust" }
  $resumeArgs += $approval
  $resumeArgs += "Acknowledge second call with {`"second`":true}. Do not modify files."
  $resumeOut = Invoke-AgentCapture -Argv $resumeArgs -Stdin "" -OutName "second-stream.txt"
}

# Stream schema: at least one JSON object line in either stream
function Test-StreamSchema([string]$text) {
  foreach ($line in ($text -split "`r?`n")) {
    $t = $line.Trim()
    if ($t.StartsWith("{") -and $t.EndsWith("}")) {
      try { $null = $t | ConvertFrom-Json; return $true } catch { }
    }
  }
  return $false
}

$streamOk = (Test-StreamSchema $plan.stdout) -or (Test-StreamSchema $resumeOut.stdout)
if (-not $streamOk) {
  Fail "Stream schema check failed (no parseable stream-json objects)"
}

$result = @{
  ok = $true
  skipped = $false
  agent = $agent
  version = $version
  capabilityHash = $capabilityHash
  model = $model
  resumeModel = $resumeModel
  chatIdExtracted = [bool]$chatId
  chatId = $chatId
  features = $features
  planExit = $plan.exitCode
  resumeExit = $resumeOut.exitCode
  streamSchemaOk = $streamOk
  spendingConsent = $true
  disposableWorkspace = $work
  completedAtUtc = [DateTime]::UtcNow.ToString("o")
}

$result | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $ArtifactOut "canary-result.json")
@{
  ok = $true
  capabilityHash = $capabilityHash
  version = $version
  consentedAtUtc = [DateTime]::UtcNow.ToString("o")
} | ConvertTo-Json | Set-Content -Encoding utf8 $gateFile

Write-Host "Canary PASSED. capabilityHash=$capabilityHash chatIdExtracted=$([bool]$chatId)"
exit 0
