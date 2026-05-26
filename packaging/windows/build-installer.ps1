# Build FluxFS Windows installer (.exe setup)
# Requires: Rust, NSIS (https://nsis.sourceforge.io/)
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Set-Location $Root

$Version = (Select-String -Path Cargo.toml -Pattern '^version = ' | Select-Object -First 1).Line
$Version = $Version -replace '.*"(.*)".*', '$1'

if (-not $SkipBuild) {
    Write-Host "Building release binaries..."
    cargo build --release --bins
}

New-Item -ItemType Directory -Force -Path dist | Out-Null

$Nsis = Get-Command makensis -ErrorAction SilentlyContinue
if (-not $Nsis) {
    $NsisPath = "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
    if (Test-Path $NsisPath) { $Nsis = $NsisPath } else { throw "NSIS not found. Install from https://nsis.sourceforge.io/" }
} else {
    $Nsis = $Nsis.Source
}

Write-Host "Running NSIS..."
& $Nsis "packaging\windows\installer.nsi"

Write-Host "Created dist\FluxFS-${Version}-windows-x86_64-setup.exe"
