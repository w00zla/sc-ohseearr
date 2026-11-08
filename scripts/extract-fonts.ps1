<#
.SYNOPSIS
Extracts the game's UI fonts from Data.p4k with the StarBreaker CLI and converts them to TrueType for training.

.DESCRIPTION
Star Citizen keeps its UI fonts in the Scaleform font library Data/UI/fonts/Shared/fonts_en.swf inside Data.p4k
(29 fonts, among them Blender Pro, a commercial font). The recognizer recipes draw synthetic lines in them
(synth.fonts.dir: fonts-sc/ttf in the data root). This extracts the SWF and runs training/tools/swf2ttf.py over
it: one <Family>-<Style>.ttf per font in the output folder. The fonts are for local training only: never put them
into git or a build.

Needs a StarBreaker CLI with `p4k extract`: upstream (https://github.com/diogotr7/StarBreaker) or a fork such as
starbreaker-ps (https://github.com/w00zla/starbreaker-ps), and the training venv's Python with fontTools
(training/README.md).

.EXAMPLE
./scripts/extract-fonts.ps1 -StarBreaker C:\tools\starbreaker.exe -P4k "F:\Games\StarCitizen\LIVE\Data.p4k"
#>
param(
    [Parameter(Mandatory)] [string] $StarBreaker,
    [string] $P4k = $env:SC_DATA_P4K,
    [string] $Out,
    [string] $Python
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot
if (-not $P4k) { throw "Data.p4k: pass -P4k or set SC_DATA_P4K" }
if (-not (Test-Path $P4k)) { throw "Data.p4k not found: $P4k" }
if (-not $Out) {
    $root = if ($env:SC_OHSEEARR_DATA) { $env:SC_OHSEEARR_DATA } else { Join-Path $repo 'local' }
    $Out = Join-Path $root 'fonts-sc/ttf'
}
if (-not $Python) { $Python = Join-Path $repo 'local/detector/.venv/Scripts/python.exe' }
if (-not (Test-Path $Python)) { throw "Python not found: $Python (pass -Python, see training/README.md)" }

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("fonts-" + [guid]::NewGuid())
try {
    & $StarBreaker p4k extract --p4k $P4k --filter 'Data/UI/fonts/Shared/fonts_en.swf' -o $tmp
    if ($LASTEXITCODE -ne 0) { throw "StarBreaker failed ($LASTEXITCODE)" }
    $swf = Get-ChildItem $tmp -Recurse -Filter fonts_en.swf | Select-Object -First 1
    if (-not $swf) { throw "fonts_en.swf not found in Data.p4k" }
    & $Python -I (Join-Path $repo 'training/tools/swf2ttf.py') $swf.FullName $Out
    if ($LASTEXITCODE -ne 0) { throw "swf2ttf failed ($LASTEXITCODE)" }
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
