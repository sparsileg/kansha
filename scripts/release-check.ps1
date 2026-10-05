# Release checks (devdocs/release-checklist.md §3, §4). Runs every
# check, then prints one line per check: PASS, FAIL, or NOTE (look at
# it; not a failure by itself). Exit status is 1 if any check failed.
# Windows PowerShell 5.1 or later; the same as release-check.sh.
#
# Usage: scripts/release-check.ps1 [BASE]
#   BASE: the commit or tag of the last release. Defaults to the latest
#   tag; with no tag, the checks that compare with it are skipped.
param([string]$Base = '')
# Not 'Stop': Windows PowerShell 5.1 would stop on any line a program
# writes to stderr. Each check looks at exit codes instead.
$ErrorActionPreference = 'Continue'
Set-Location (Join-Path $PSScriptRoot '..')

$log = 'target/release-check'
New-Item -ItemType Directory -Force -Path $log | Out-Null
$results = New-Object System.Collections.Generic.List[string]
$script:failed = 0
function Pass([string]$m) { $results.Add("PASS  $m") }
function Fail([string]$m) { $results.Add("FAIL  $m"); $script:failed = 1 }
function Note([string]$m) { $results.Add("NOTE  $m") }
function Step([string]$m) { Write-Output ''; Write-Output "=== $m" }

# Runs a program, its output (stdout and stderr) to the file `$To` and,
# with -Show, to the screen too as it comes. Returns true when it
# exits 0.
function Run([string]$To, [switch]$Show, [string]$Exe) {
    if ($Show) {
        & $Exe @args 2>&1 | ForEach-Object { "$_" } | Tee-Object -FilePath $To | Out-Host
    } else {
        & $Exe @args 2>&1 | ForEach-Object { "$_" } | Set-Content -Encoding UTF8 -Path $To
    }
    return $LASTEXITCODE -eq 0
}

# Lines a program prints, or none when it fails.
function Lines([string]$Exe) {
    $out = & $Exe @args 2>$null
    if ($LASTEXITCODE -ne 0) { return @() }
    return @($out | Where-Object { $_ -ne '' })
}

if (-not $Base) {
    $Base = (Lines git describe --tags --abbrev=0) | Select-Object -First 1
    if (-not $Base) { $Base = '' }
}

# 1. Format, lints, type-checking, every test.
Step 'just check'
if (Run "$log/check.txt" -Show just check) {
    Pass 'just check'
} else {
    Fail "just check (see $log/check.txt)"
}

# 2. The version is the same in every place it is written.
Step 'version'
$vCargo = ((Get-Content Cargo.toml) -match '^version = "' | Select-Object -First 1) -replace '^version = "(.*)"', '$1'
# No double quotes in node's arguments: PowerShell 5.1 and 7 pass them
# differently.
$vPkg = (Lines node -p 'require(''./package.json'').version') -join ' '
$vLock = (Lines node -p 'const l=require(''./package-lock.json''); l.version+'' ''+l.packages[''''].version') -join ' '
$vTauri = (Lines node -p 'require(''./src-tauri/tauri.conf.json'').version') -join ' '
$vTest = ((Get-Content src/lib/version.test.ts) | Select-String 'toBe\("(.*)"\)' |
    ForEach-Object { $_.Matches[0].Groups[1].Value }) -join ' '
$lock = Get-Content Cargo.lock
$vClock = @()
for ($i = 0; $i -lt $lock.Count - 1; $i++) {
    if ($lock[$i] -match '^name = "kansha(-core)?"$' -and $lock[$i + 1] -match '^version = "(.*)"') {
        $vClock += $Matches[1]
    }
}
$vClock = $vClock -join ' '
$all = "$vCargo $vPkg $vLock $vTauri $vTest $vClock" -split '\s+' | Where-Object { $_ -ne '' }
if ($vCargo -and -not ($all | Where-Object { $_ -ne $vCargo })) {
    Pass "version $vCargo in all places"
} else {
    Fail "versions differ: Cargo.toml $vCargo, package.json $vPkg, package-lock $vLock, tauri.conf.json $vTauri, version.test.ts $vTest, Cargo.lock $vClock (fix with: just version X.Y.Z)"
}

# 3. Bindings match the command signatures.
Step 'bindings'
$f = 'src/lib/types/bindings.ts'
$before = Lines git hash-object $f
if (Run "$log/bindings.txt" just bindings) {
    if ("$before" -eq "$(Lines git hash-object $f)") {
        Pass 'bindings unchanged'
    } else {
        Fail "just bindings changed $f; review and commit the diff"
    }
} else {
    Fail "just bindings (see $log/bindings.txt)"
}

# 4. Snapshots: none pending; list those changed since the last release.
Step 'snapshots'
$pending = (Get-ChildItem -Recurse -File -Path crates -Filter '*.snap.new' |
    ForEach-Object { Resolve-Path -Relative $_.FullName }) -join ' '
if ($pending) {
    Fail "pending snapshots: $pending (run: just snap-review)"
} else {
    Pass 'no pending snapshots'
}
if ($Base) {
    $changed = (Lines git diff --name-only $Base -- '*.snap') -join ' '
    if ($changed) { Note "snapshots changed since ${Base}; name each in the release notes: $changed" }
}

# 5. Released migrations are never edited.
Step 'migrations'
$mig = 'crates/kansha-core/src/persistence/migrations'
if ($Base) {
    $edited = (Lines git diff --name-status $Base -- $mig | Where-Object { $_ -notmatch '^A' }) -join ' '
    if ($edited) {
        Fail "released migrations changed since ${Base}: $edited"
    } else {
        Pass "migrations: only new files since $Base"
    }
    $added = (Lines git diff --name-only --diff-filter=A $Base -- $mig) -join ' '
    if ($added) { Note "new migrations (Schema change; each needs a migration test): $added" }
} else {
    Note "migrations not compared: no base (pass the last release's commit)"
}

# 6. Traceability: the only uncited IDs are those spec §23 says are not built.
Step 'just trace'
Run "$log/trace.txt" just trace | Out-Null
$trace = Get-Content "$log/trace.txt"
$uncited = @($trace | Select-String '^ *not cited: (.*)$' |
    ForEach-Object { $_.Matches[0].Groups[1].Value -split '\s+' } |
    Where-Object { $_ -ne '' } | Sort-Object)
$spec = [IO.File]::ReadAllText((Join-Path (Get-Location) 'devdocs/kansha-spec.md'))
$s23 = ''
if ($spec -match '(?ms)^### 23\..*?^### 24\.') { $s23 = $Matches[0] }
$unexpected = ($uncited | Where-Object { -not $s23.Contains($_) }) -join ' '
$summary = $trace | Select-String 'engine requirements cited' | Select-Object -First 1
if (-not $summary) {
    Fail "just trace did not run (see $log/trace.txt)"
} elseif ($unexpected) {
    Fail "requirements not cited by a test and not listed in spec §23: $unexpected"
} else {
    Pass "trace: $($summary.Line.Trim()); uncited are all in §23: $($uncited -join ' ')"
}

# 7. Coverage: record it beside the last release's.
Step 'just cov'
if (Run "$log/cov.txt" just cov) {
    $total = Get-Content "$log/cov.txt" | Where-Object { $_ -match '^TOTAL' } | Select-Object -First 1
    $w = "$total" -split '\s+'
    Note "coverage regions $($w[3]), functions $($w[6]), lines $($w[9]); record it in the release notes (detail: target/llvm-cov/html/index.html)"
} else {
    Fail "just cov (see $log/cov.txt)"
}

# 8. Large-book timings.
Step 'just perf'
if (Run "$log/perf.txt" -Show just perf) {
    Note "timings in $log/perf.txt; record them in the release notes"
} else {
    Fail "just perf: a timing is over its limit (see $log/perf.txt)"
}

# 9. Builds on the minimum supported Rust version.
Step 'MSRV'
if (Lines rustup toolchain list | Where-Object { $_ -match '^1\.85' }) {
    if (Run "$log/msrv.txt" cargo +1.85 check --workspace) {
        Pass 'builds with Rust 1.85'
    } else {
        Fail "cargo +1.85 check (see $log/msrv.txt)"
    }
} else {
    Note 'MSRV not checked: run once: rustup toolchain install 1.85'
}

# 10. Everything is committed.
Step 'git status'
if (-not (Lines git status --porcelain)) {
    Pass 'working tree clean'
} else {
    Note 'uncommitted changes; commit them before tagging'
}

Write-Output ''
if ($Base) { Write-Output "=== Release check against $Base" } else { Write-Output '=== Release check' }
$results | Set-Content -Encoding UTF8 -Path "$log/summary.txt"
$results | Write-Output
exit $script:failed
