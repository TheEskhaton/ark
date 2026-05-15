# test-large-project.ps1
#
# Clones ABP Framework (~500 projects) and runs every ark command against it
# to verify correctness and check performance at scale.
#
# NOTE: The ABP repo is large — initial clone may take several minutes.
#
# Usage:
#   .\test-large-project.ps1                  # build + clone + run
#   .\test-large-project.ps1 -SkipBuild       # use existing ark binary
#   .\test-large-project.ps1 -Keep            # don't delete temp dir after run
#   .\test-large-project.ps1 -SolutionPath D:\some\existing\solution  # skip clone

param(
    [string]$SolutionPath = "",
    [switch]$SkipBuild,
    [switch]$Keep
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ArkDir  = $PSScriptRoot
$ArkExe  = Join-Path $ArkDir "target\release\ark.exe"

# ── 1. Build ───────────────────────────────────────────────────────────────
if (-not $SkipBuild) {
    Write-Host "Building ark (release)..." -ForegroundColor Cyan
    Push-Location $ArkDir
    cargo build --release --quiet
    Pop-Location
}

if (-not (Test-Path $ArkExe)) {
    Write-Error "ark binary not found at $ArkExe. Run without -SkipBuild first."
}

# ── 2. Clone or use existing solution ─────────────────────────────────────
$Tmp = $null
if ($SolutionPath) {
    $Root = $SolutionPath
    Write-Host "Using existing solution at $Root" -ForegroundColor Cyan
} else {
    $Tmp = Join-Path $env:TEMP "ark-smoke-$(Get-Random)"
    Write-Host "Cloning ABP Framework → $Tmp  (large repo, may take a few minutes)" -ForegroundColor Cyan
    git clone --depth 1 --single-branch --no-tags --quiet https://github.com/abpframework/abp $Tmp
    $Root = $Tmp
}

# ── 3. Write architecture.json ─────────────────────────────────────────────
#
# ABP Framework layer mapping (simplified DDD):
#   *.Domain.Shared          → DomainShared  (shared kernel, no deps on other layers)
#   *.Domain                 → Domain        (entities, domain services)
#   *.Application.Contracts  → Application   (interfaces, DTOs)
#   *.Application            → Application
#   *.EntityFrameworkCore    → Infrastructure
#   *.MongoDB                → Infrastructure
#   *.Dapper                 → Infrastructure
#   *.HttpApi                → Presentation
#   *.HttpApi.Client         → Presentation
#   *.Web                    → Presentation
#   *.Blazor                 → Presentation
#
$ConfigJson = @"
{
  "layers": [
    {
      "name": "DomainShared",
      "patterns": ["*.Domain.Shared"]
    },
    {
      "name": "Domain",
      "patterns": ["*.Domain"]
    },
    {
      "name": "Application",
      "patterns": ["*.Application.Contracts", "*.Application"]
    },
    {
      "name": "Infrastructure",
      "patterns": ["*.EntityFrameworkCore", "*.MongoDB", "*.Dapper"]
    },
    {
      "name": "Presentation",
      "patterns": ["*.HttpApi", "*.HttpApi.Client", "*.Web", "*.Blazor"]
    }
  ],
  "dependencyRules": [
    { "from": "Presentation",   "to": "Application",   "allowed": true  },
    { "from": "Presentation",   "to": "Domain",        "allowed": false },
    { "from": "Presentation",   "to": "Infrastructure","allowed": false },
    { "from": "Infrastructure", "to": "Domain",        "allowed": true  },
    { "from": "Infrastructure", "to": "Application",   "allowed": false },
    { "from": "Application",    "to": "Domain",        "allowed": true  },
    { "from": "Application",    "to": "DomainShared",  "allowed": true  },
    { "from": "Application",    "to": "Infrastructure","allowed": false },
    { "from": "Domain",         "to": "DomainShared",  "allowed": true  },
    { "from": "Domain",         "to": "Infrastructure","allowed": false },
    { "from": "Domain",         "to": "Application",   "allowed": false },
    { "from": "DomainShared",   "to": "Domain",        "allowed": false },
    { "from": "DomainShared",   "to": "Application",   "allowed": false },
    { "from": "DomainShared",   "to": "Infrastructure","allowed": false }
  ],
  "packagePolicies": [
    {
      "layer": "Domain",
      "forbidden": ["Microsoft.EntityFrameworkCore", "MongoDB.Driver"]
    },
    {
      "layer": "DomainShared",
      "forbidden": ["Microsoft.EntityFrameworkCore", "MongoDB.Driver"]
    }
  ],
  "ignorePatterns": [
    "*.Tests", "*.Test", "*.Testing",
    "*.Samples", "*.Sample",
    "*.Demo", "*.Template"
  ]
}
"@

$ConfigPath = Join-Path $Root "architecture.json"
$ConfigJson | Set-Content $ConfigPath -Encoding UTF8
Write-Host "Config written to $ConfigPath" -ForegroundColor DarkGray

# Use a non-existent .pkl path so ark falls back to the .json sidecar
$PklPath = Join-Path $Root "architecture.pkl"

# ── 4. Helpers ─────────────────────────────────────────────────────────────
function Run-Step {
    param([string]$Label, [scriptblock]$Block)
    Write-Host ""
    Write-Host ("─" * 60) -ForegroundColor DarkGray
    Write-Host "  $Label" -ForegroundColor Yellow
    Write-Host ("─" * 60) -ForegroundColor DarkGray
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $exitCode = 0
    try {
        & $Block
        $exitCode = $LASTEXITCODE
    } catch {
        Write-Host "  ERROR: $_" -ForegroundColor Red
    }
    $sw.Stop()
    $color = if ($exitCode -eq 0) { "Green" } else { "Red" }
    Write-Host ""
    Write-Host "  exit=$exitCode  time=$($sw.ElapsedMilliseconds)ms" -ForegroundColor $color
}

# ── 5. Discover a matched project name for `ark explain` ───────────────────
$ExplainTarget = "Volo.Abp.Identity.Domain"

# ── 6. Run all commands ────────────────────────────────────────────────────
Run-Step "ark check (first run — expect violations if any)" {
    & $ArkExe --root $Root --config $PklPath check
}

Run-Step "ark graph --format mermaid (stdout, first 30 lines)" {
    & $ArkExe --root $Root --config $PklPath graph | Select-Object -First 30
    Write-Host "  ... (truncated)"
}

Run-Step "ark graph --format dot -o graph.dot" {
    $OutFile = Join-Path $Root "graph.dot"
    & $ArkExe --root $Root --config $PklPath graph --format dot --output $OutFile
    if (Test-Path $OutFile) {
        Write-Host "  Written: $OutFile ($(((Get-Item $OutFile).Length)) bytes)"
    }
}

Run-Step "ark explain $ExplainTarget" {
    & $ArkExe --root $Root --config $PklPath explain $ExplainTarget
}

Run-Step "ark baseline (snapshot current violations)" {
    & $ArkExe --root $Root --config $PklPath baseline
}

Run-Step "ark check (after baseline — should report 0 new violations)" {
    & $ArkExe --root $Root --config $PklPath check
}

Run-Step "ark check --no-baseline (ignore baseline — violations reappear)" {
    & $ArkExe --root $Root --config $PklPath check --no-baseline
}

# ── 7. Project count summary ───────────────────────────────────────────────
$CsprojCount = (Get-ChildItem $Root -Filter "*.csproj" -Recurse |
    Where-Object { $_.FullName -notmatch "\\bin\\" -and $_.FullName -notmatch "\\obj\\" }).Count
Write-Host ""
Write-Host ("═" * 60) -ForegroundColor Cyan
Write-Host "  Projects found: $CsprojCount" -ForegroundColor Cyan
Write-Host "  ark binary:     $ArkExe" -ForegroundColor Cyan
Write-Host "  Solution root:  $Root" -ForegroundColor Cyan
Write-Host ("═" * 60) -ForegroundColor Cyan

# ── 8. Cleanup ─────────────────────────────────────────────────────────────
if ($Tmp -and -not $Keep) {
    Write-Host ""
    Write-Host "Cleaning up $Tmp ..." -ForegroundColor DarkGray
    Remove-Item $Tmp -Recurse -Force
}
