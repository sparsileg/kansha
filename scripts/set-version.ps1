# Sets the app version everywhere it is written: Cargo.toml,
# package.json, package-lock.json, src-tauri/tauri.conf.json,
# src/lib/version.test.ts, and Cargo.lock. Windows PowerShell 5.1 or
# later; the same as set-version.sh.
#
# Usage: scripts/set-version.ps1 X.Y.Z
param([Parameter(Mandatory = $true)][string]$V)
# Not 'Stop': Windows PowerShell 5.1 would stop on any line a program
# writes to stderr. Exit codes are checked instead; anything else
# that fails stops the script.
trap { [Console]::Error.WriteLine($_); exit 1 }
Set-Location (Join-Path $PSScriptRoot '..')

if ($V -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
    [Console]::Error.WriteLine("not X.Y.Z: $V")
    exit 1
}

# Files are read and written whole, as UTF-8 without a BOM, so line
# endings are kept.
function Edit-File([string]$Path, [scriptblock]$Change) {
    $full = Join-Path (Get-Location) $Path
    $text = [IO.File]::ReadAllText($full)
    [IO.File]::WriteAllText($full, (& $Change $text), (New-Object Text.UTF8Encoding $false))
}

# The first `version = "..."` only: the workspace package's.
Edit-File 'Cargo.toml' {
    param($t)
    ([regex]'(?m)^version = ".*"').Replace($t, "version = `"$V`"", 1)
}
npm version $V --no-git-tag-version --allow-same-version | Out-Null
if ($LASTEXITCODE -ne 0) { exit 1 }
Edit-File 'src-tauri/tauri.conf.json' {
    param($t)
    ([regex]'"version": ".*"').Replace($t, "`"version`": `"$V`"", 1)
}
Edit-File 'src/lib/version.test.ts' {
    param($t)
    $t = $t -replace 'it\("is [0-9.]*"', "it(`"is $V`""
    $t -replace 'toBe\("[0-9.]*"\)', "toBe(`"$V`")"
}
cargo update --workspace --offline 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    cargo update --workspace 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { exit 1 }
}
Write-Output "version set to ${V}:"
git diff --stat -- Cargo.toml Cargo.lock package.json package-lock.json src-tauri/tauri.conf.json src/lib/version.test.ts
