#!/usr/bin/env pwsh
<#
.SYNOPSIS
Builds a Windows release installer for Art Window.

This script builds the Windows release binary and creates an installer using Inno Setup.
#>

$ErrorActionPreference = 'Stop'

# Navigate to repository root
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $scriptDir

# Read version from Cargo.toml (first match only)
$versionMatch = Select-String -Path "$root\Cargo.toml" -Pattern '^version = "([^"]+)"' | Select-Object -First 1
if (-not $versionMatch) {
    Write-Error "could not read version from $root\Cargo.toml"
    exit 1
}
$version = $versionMatch.Matches[0].Groups[1].Value

Write-Host "Building Art Window v$version for Windows..." -ForegroundColor Green

# Resolve build target directory
$buildTarget = $env:CARGO_TARGET_DIR ?? "$root\target"
if (-not [System.IO.Path]::IsPathRooted($buildTarget)) {
    $buildTarget = Join-Path $root $buildTarget
}

# Build the release binary
Write-Host "Running cargo build..." -ForegroundColor Cyan
& cargo build --release --locked

$exePath = Join-Path $buildTarget "release\art-window.exe"
if (-not (Test-Path $exePath)) {
    Write-Error "Build failed: $exePath not found"
    exit 1
}

# Find Inno Setup compiler
$isccPath = $null
if (Get-Command ISCC.exe -ErrorAction SilentlyContinue) {
    $isccPath = (Get-Command ISCC.exe).Source
}
else {
    $defaultPath = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
    if (Test-Path $defaultPath) {
        $isccPath = $defaultPath
    }
}

if (-not $isccPath) {
    Write-Error "Inno Setup 6 not found. Install it or add it to PATH."
    exit 1
}

Write-Host "Using Inno Setup: $isccPath" -ForegroundColor Cyan

# Create output directory
$distDir = Join-Path $buildTarget "dist"
New-Item -ItemType Directory -Path $distDir -Force | Out-Null

# Run Inno Setup compiler
Write-Host "Running Inno Setup compiler..." -ForegroundColor Cyan
$issScript = Join-Path $scriptDir "art-window.iss"
& $isccPath "/DVersion=$version" "/DExe=$exePath" "/DOutputDir=$distDir" $issScript

if ($LASTEXITCODE -ne 0) {
    Write-Error "Inno Setup compiler failed with exit code $LASTEXITCODE"
    exit 1
}

$installerPath = Join-Path $distDir "Art-Window-$version-windows-x64-setup.exe"
if (Test-Path $installerPath) {
    Write-Host "Created installer: $installerPath" -ForegroundColor Green
    Write-Host "Size: $([Math]::Round((Get-Item $installerPath).Length / 1MB, 2)) MB" -ForegroundColor Green
}
else {
    Write-Error "Installer creation failed: $installerPath not found"
    exit 1
}
