<#
.SYNOPSIS
Extracts the game's built-in English global.ini from Data.p4k with the StarBreaker CLI.

.DESCRIPTION
The game keeps its English texts inside Data.p4k; a language pack on disk replaces them. Readers need these texts
(ADR 0007), so players without a pack need this file. It is written as global-english-<version>.ini into the
output folder, the version from build_manifest.id next to Data.p4k, so a stale file is obvious after a patch.

Needs the StarBreaker CLI (https://github.com/w00zla/starbreaker-qm, `starbreaker p4k extract`). Untested while
the game runs and holds Data.p4k open.

.EXAMPLE
./scripts/extract-global-ini.ps1 -StarBreaker C:\tools\starbreaker.exe -P4k "F:\Games\StarCitizen\LIVE\Data.p4k" -Out .\local
#>
param(
    [Parameter(Mandatory)] [string] $StarBreaker,
    [string] $P4k = $env:SC_DATA_P4K,
    [string] $Out = "."
)
$ErrorActionPreference = 'Stop'
if (-not $P4k) { throw "Data.p4k: pass -P4k or set SC_DATA_P4K" }
if (-not (Test-Path $P4k)) { throw "Data.p4k not found: $P4k" }

$manifest = Join-Path (Split-Path $P4k) 'build_manifest.id'
$version = 'unknown'
if (Test-Path $manifest) { $version = (Get-Content $manifest -Raw | ConvertFrom-Json).Data.Version }

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("global-ini-" + [guid]::NewGuid())
try {
    & $StarBreaker p4k extract --p4k $P4k --filter 'Data/Localization/english/global.ini' -o $tmp
    if ($LASTEXITCODE -ne 0) { throw "StarBreaker failed ($LASTEXITCODE)" }
    $file = Get-ChildItem $tmp -Recurse -Filter global.ini | Select-Object -First 1
    if (-not $file) { throw "global.ini not found in Data.p4k" }
    New-Item -ItemType Directory -Force $Out | Out-Null
    $dest = Join-Path $Out "global-english-$version.ini"
    Copy-Item $file.FullName $dest -Force
    Write-Output $dest
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
