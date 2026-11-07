# Build the installer locally: .\installer\build.ps1 [-Version 0.2.0]
# Needs Inno Setup 6 (ISCC on PATH or in Program Files) and a release build.
param([string]$Version = "0.0.0-dev")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    cargo build --release
    & .\target\release\winsploit.exe --catalog .\tools.yaml packs --iss | Set-Content -Encoding ascii installer\packs.iss
    $iscc = (Get-Command ISCC -ErrorAction SilentlyContinue).Source
    if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
    & $iscc "/DAppVersion=$Version" installer\winsploit.iss
} finally { Pop-Location }
