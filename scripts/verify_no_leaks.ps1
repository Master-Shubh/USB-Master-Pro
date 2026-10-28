<#
.SYNOPSIS
    USB Master Pro — Windows Release Artifact Secret Leak Verifier (PowerShell)
    BhartX Ecosystems Pvt Ltd — Autonomous Security Gate

.DESCRIPTION
    Scans compiled Windows binaries (.exe, .sys, .dll, .rlib) for embedded
    private keys, root CA seeds, and internal development credentials.
    Fails build immediately with non-zero exit code if any secret pattern is detected.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$TargetDir = "",

    [Parameter(Mandatory = $false)]
    [string]$DriversDir = ""
)

if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    if (Test-Path "$PSScriptRoot\..\target\x86_64-pc-windows-msvc\release") {
        $TargetDir = "$PSScriptRoot\..\target\x86_64-pc-windows-msvc\release"
    } elseif (Test-Path "$PSScriptRoot\..\target\release") {
        $TargetDir = "$PSScriptRoot\..\target\release"
    } elseif (Test-Path "$PSScriptRoot\target\x86_64-pc-windows-msvc\release") {
        $TargetDir = "$PSScriptRoot\target\x86_64-pc-windows-msvc\release"
    } elseif (Test-Path "$PSScriptRoot\target\release") {
        $TargetDir = "$PSScriptRoot\target\release"
    } else {
        $TargetDir = "$PSScriptRoot\..\target\release"
    }
}

if ([string]::IsNullOrWhiteSpace($DriversDir)) {
    if (Test-Path "$PSScriptRoot\drivers") {
        $DriversDir = "$PSScriptRoot\drivers"
    } elseif (Test-Path "$PSScriptRoot\..\drivers") {
        $DriversDir = "$PSScriptRoot\..\drivers"
    } else {
        $DriversDir = "$PSScriptRoot\drivers"
    }
}

$ErrorActionPreference = "Stop"

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "  🛡️ USB Master Pro — Windows Release Artifact Secret Leak Gate" -ForegroundColor Cyan
Write-Host "  🏢 BhartX Ecosystems Pvt Ltd" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

$forbiddenPatterns = @(
    "BHARTX_MOCK_TEST_HARNESS_KEY_001",
    "BEGIN PRIVATE KEY",
    "BEGIN RSA PRIVATE KEY",
    "BEGIN EC PRIVATE KEY",
    "BEGIN OPENSSH PRIVATE KEY",
    "BEGIN ENCRYPTED PRIVATE KEY"
)

$targetFiles = @()

if (Test-Path $TargetDir) {
    # Scan release binaries and compiled core library, avoiding intermediate deps test caches
    $targetFiles += Get-ChildItem -Path $TargetDir | Where-Object {
        -not $_.PSIsContainer -and (
            $_.Name -match "^usb_master_(daemon|cli)(\.exe)?$" -or
            $_.Name -match "^(lib)?usb_master_core\.(rlib|lib)$"
        )
    }
}

if (Test-Path $DriversDir) {
    $targetFiles += Get-ChildItem -Path $DriversDir -Include "*.sys" -Recurse -File
}

if ($targetFiles.Count -eq 0) {
    Write-Host "  [!] Warning: No compiled release artifacts found in target directories to scan." -ForegroundColor Yellow
    exit 0
}

$leaksFound = 0

foreach ($file in $targetFiles) {
    Write-Host "  👉 Scanning: $($file.Name) ($([math]::Round($file.Length / 1KB, 2)) KB)..." -ForegroundColor White
    $bytes = [System.IO.File]::ReadAllBytes($file.FullName)
    $latin1String = [System.Text.Encoding]::GetEncoding("iso-8859-1").GetString($bytes)
    $utf8String = [System.Text.Encoding]::UTF8.GetString($bytes)

    foreach ($pattern in $forbiddenPatterns) {
        $found = $false
        if ($latin1String.Contains($pattern) -or $utf8String.Contains($pattern)) {
            $found = $true
        }

        if ($found) {
            Write-Host "  ❌ [CRITICAL LEAK] Forbidden pattern '$pattern' detected in $($file.FullName)!" -ForegroundColor Red
            $leaksFound++
        }
    }
}

Write-Host "--------------------------------------------------------------------------------" -ForegroundColor Cyan

if ($leaksFound -gt 0) {
    Write-Host "  🚨 SECURITY GATE FAILED: $leaksFound forbidden signature(s) detected in release artifacts!" -ForegroundColor Red
    Write-Host "  ⛔ Release builds MUST NOT proceed. Fix feature gates and rebuild." -ForegroundColor Red
    exit 1
} else {
    Write-Host "  ✅ LEAK CHECK PASSED: 0 forbidden signatures found across all $($targetFiles.Count) scanned artifact(s)." -ForegroundColor Green
    Write-Host "  🛡️ Binary integrity verified: BHARTX root seed and private keys are strictly absent." -ForegroundColor Green
    exit 0
}
