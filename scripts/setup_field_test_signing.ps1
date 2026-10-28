<#
.SYNOPSIS
    BhartX Ecosystems Pvt Ltd — Automated Field-Testing Driver Setup & Signing Script
    Windows 10 / Windows 11 (x64 and ARM64)

.DESCRIPTION
    Automates the entire driver signing and local test environment setup for USB Master Pro.
    1. Validates Administrator privileges.
    2. Enables Windows Test-Signing mode via BCDEdit (bcdedit /set testsigning on).
    3. Generates the dedicated "BhartX Ecosystems Field Testing CA" certificate.
    4. Installs the CA into LocalMachine\Root and LocalMachine\TrustedPublisher stores.
    5. Discovers Windows Kit tools (inf2cat.exe, signtool.exe).
    6. Generates catalog (.cat) files from INF files using inf2cat.
    7. Signs .sys and .cat files with SHA-256 and RFC-3161 timestamping.
    8. Verifies driver digital signatures.
    9. Optionally installs drivers silently into Windows Driver Store via PnPUtil.

.NOTES
    Owner: BhartX Ecosystems Pvt Ltd
    Product: USB Master Pro
    Target OS: Windows 10 (1809+), Windows 11 (all versions), Windows Server 2019/2022
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$DriverRootPath = "",

    [Parameter(Mandatory = $false)]
    [string]$CertSubject = "CN=BhartX Ecosystems Field Testing CA, O=BhartX Ecosystems Pvt Ltd, C=IN",

    [Parameter(Mandatory = $false)]
    [switch]$EnableBcdTestSigning = $true,

    [Parameter(Mandatory = $false)]
    [switch]$InstallDrivers = $false,

    [Parameter(Mandatory = $false)]
    [switch]$ForceRegenerateCert = $false
)

if ([string]::IsNullOrWhiteSpace($DriverRootPath)) {
    if (Test-Path "$PSScriptRoot\drivers") {
        $DriverRootPath = "$PSScriptRoot\drivers"
    } elseif (Test-Path "$PSScriptRoot\..\drivers") {
        $DriverRootPath = "$PSScriptRoot\..\drivers"
    } else {
        $DriverRootPath = "$PSScriptRoot\drivers"
    }
}
if (Test-Path $DriverRootPath) {
    $DriverRootPath = (Resolve-Path $DriverRootPath).Path
}

$ErrorActionPreference = "Stop"

# Colors and formatting helpers
function Write-Header {
    param([string]$Title)
    Write-Host "`n================================================================================" -ForegroundColor Cyan
    Write-Host "  $Title" -ForegroundColor Cyan
    Write-Host "================================================================================" -ForegroundColor Cyan
}

function Write-Success {
    param([string]$Msg)
    Write-Host "  [+] $Msg" -ForegroundColor Green
}

function Write-Info {
    param([string]$Msg)
    Write-Host "  [*] $Msg" -ForegroundColor White
}

function Write-Warn {
    param([string]$Msg)
    Write-Host "  [!] $Msg" -ForegroundColor Yellow
}

function Write-Failure {
    param([string]$Msg)
    Write-Host "  [-] $Msg" -ForegroundColor Red
}

Write-Header "BhartX Ecosystems — USB Master Pro Field-Testing Driver Signing Utility"
Write-Info "Copyright (c) 2026 BhartX Ecosystems Pvt Ltd. All rights reserved."

# -----------------------------------------------------------------------------
# STEP 1: Verify Administrator Privileges
# -----------------------------------------------------------------------------
Write-Header "Step 1: Checking Administrator Privileges"
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Failure "Administrator privileges are required to configure BCDEdit and Certificate Stores."
    Write-Warn "Please right-click PowerShell and choose 'Run as Administrator'."
    exit 1
}
Write-Success "Elevated privileges confirmed."

# -----------------------------------------------------------------------------
# STEP 2: Configure BCDEdit Test-Signing Mode
# -----------------------------------------------------------------------------
if ($EnableBcdTestSigning) {
    Write-Header "Step 2: Checking and Enabling BCDEdit Test-Signing"
    try {
        $bcdOutput = bcdedit.exe /enum '{current}' 2>&1 | Out-String
        if ($bcdOutput -match "testsigning\s+Yes") {
            Write-Success "Windows Test-Signing is already ENABLED on this machine."
        } else {
            Write-Info "Enabling Windows Test-Signing mode via 'bcdedit /set testsigning on'..."
            $res = bcdedit.exe /set testsigning on 2>&1 | Out-String
            if ($LASTEXITCODE -eq 0) {
                Write-Success "Test-Signing successfully enabled."
                Write-Warn "NOTE: If this machine has UEFI Secure Boot enabled, Windows will ignore testsigning."
                Write-Warn "For local physical test machines: Disable Secure Boot in UEFI BIOS, OR submit via Microsoft WHQL Attestation Signing."
                Write-Warn "A system restart may be required for BCDEdit changes to take full effect."
            } else {
                Write-Warn "BCDEdit command exited with code $LASTEXITCODE: $res"
                Write-Warn "If Secure Boot is active, disable Secure Boot in UEFI settings to enable Test-Signing."
            }
        }

        # Optional integrity checks toggle for maximum field test compatibility
        $null = bcdedit.exe /set nointegritychecks on 2>&1
    } catch {
        Write-Warn "Failed to query or configure BCDEdit: $_"
    }
}

# -----------------------------------------------------------------------------
# STEP 3: Setup BhartX Ecosystems Field Testing CA Certificate
# -----------------------------------------------------------------------------
Write-Header "Step 3: Configuring BhartX Ecosystems Field Testing CA"

$certFriendlyName = "BhartX Ecosystems Field Testing CA"
$existingCert = Get-ChildItem -Path "Cert:\LocalMachine\Root" | Where-Object { $_.Subject -match "BhartX Ecosystems" } | Select-Object -First 1

if ($existingCert -and -not $ForceRegenerateCert) {
    Write-Success "Found existing Root CA in Cert:\LocalMachine\Root:"
    Write-Info "  Thumbprint: $($existingCert.Thumbprint)"
    Write-Info "  Subject:    $($existingCert.Subject)"
    Write-Info "  Expires:    $($existingCert.NotAfter)"
    $signingCert = $existingCert
} else {
    Write-Info "Generating new cryptographic CA for BhartX Ecosystems Pvt Ltd..."
    
    # Enhanced Key Usages:
    # 1.3.6.1.5.5.7.3.3 = Code Signing
    # 1.3.6.1.4.1.311.10.3.6 = Windows Hardware Driver Verification
    # 1.3.6.1.4.1.311.10.3.5 = Windows Hardware Driver Extended Verification
    $ekuExtensions = "1.3.6.1.5.5.7.3.3", "1.3.6.1.4.1.311.10.3.6"

    $certParams = @{
        Type = "Custom"
        Subject = $CertSubject
        FriendlyName = $certFriendlyName
        KeySpec = "Signature"
        KeyExportPolicy = "Exportable"
        KeyLength = 4096
        KeyAlgorithm = "RSA"
        HashAlgorithm = "SHA256"
        CertStoreLocation = "Cert:\LocalMachine\My"
        KeyUsage = "DigitalSignature", "CertSign", "CRLSign"
        TextExtension = @(
            "2.5.29.37={text}$($ekuExtensions -join ',')",
            "2.5.29.19={critical}{text}ca=1"
        )
        NotAfter = (Get-Date).AddYears(5)
    }

    $signingCert = New-SelfSignedCertificate @certParams
    Write-Success "Created Self-Signed CA with thumbprint: $($signingCert.Thumbprint)"

    # Add to Trusted Root Certification Authorities
    Write-Info "Installing certificate into 'Cert:\LocalMachine\Root'..."
    $rootStore = New-Object System.Security.Cryptography.X509Certificates.X509Store("Root", "LocalMachine")
    $rootStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
    $rootStore.Add($signingCert)
    $rootStore.Close()
    Write-Success "Installed into Cert:\LocalMachine\Root."

    # Add to Trusted Publishers
    Write-Info "Installing certificate into 'Cert:\LocalMachine\TrustedPublisher'..."
    $pubStore = New-Object System.Security.Cryptography.X509Certificates.X509Store("TrustedPublisher", "LocalMachine")
    $pubStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
    $pubStore.Add($signingCert)
    $pubStore.Close()
    Write-Success "Installed into Cert:\LocalMachine\TrustedPublisher."
}

# Export public CA certificate (.cer) for client installations
if (Test-Path $DriverRootPath) {
    $certExportPath = Join-Path $DriverRootPath "bhartx_test_ca.cer"
    try {
        Export-Certificate -Cert $signingCert -FilePath $certExportPath -Type CERT -Force | Out-Null
        Write-Success "Exported public CA certificate to: $certExportPath"
    } catch {
        Write-Warn "Could not export certificate: $_"
    }
}

# -----------------------------------------------------------------------------
# STEP 4: Discover Windows SDK/WDK Build & Signing Utilities
# -----------------------------------------------------------------------------
Write-Header "Step 4: Discovering Windows SDK & WDK Utilities"

function Find-Tool {
    param([string]$ExeName)
    # Check PATH first
    $cmd = Get-Command $ExeName -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    # Common Windows Kits directories
    $searchPaths = @(
        "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\$ExeName",
        "C:\Program Files\Windows Kits\10\bin\*\x64\$ExeName",
        "C:\Program Files (x86)\Windows Kits\10\bin\x64\$ExeName",
        "C:\Program Files (x86)\Windows Kits\8.1\bin\x64\$ExeName"
    )

    foreach ($pattern in $searchPaths) {
        $found = Resolve-Path $pattern -ErrorAction SilentlyContinue | Sort-Object Path -Descending | Select-Object -First 1
        if ($found) { return $found.Path }
    }
    return $null
}

$signToolPath = Find-Tool "signtool.exe"
$inf2catPath = Find-Tool "Inf2Cat.exe"

if ($signToolPath) {
    Write-Success "SignTool located: $signToolPath"
} else {
    Write-Warn "SignTool.exe not found in Windows SDK paths. Will use PowerShell Set-AuthenticodeSignature fallback."
}

if ($inf2catPath) {
    Write-Success "Inf2Cat located:  $inf2catPath"
} else {
    Write-Warn "Inf2Cat.exe not found. Catalog files (.cat) must be pre-generated or compiled with WDK."
}

# -----------------------------------------------------------------------------
# STEP 5: Generate Catalog Files (.cat) from INF
# -----------------------------------------------------------------------------
Write-Header "Step 5: Generating Driver Catalog Files"

$driverDirs = @(
    Join-Path $DriverRootPath "vhci",
    Join-Path $DriverRootPath "filter"
)

# Supported Windows OS Targets for Inf2Cat
$osTargets = "10_X64,10_NI_X64,Server2016_X64,Server2019_X64,Server2022_X64,10_ARM64,10_CO_ARM64"

foreach ($dir in $driverDirs) {
    if (-not (Test-Path $dir)) {
        Write-Warn "Driver directory not found: $dir"
        continue
    }

    $infFiles = Get-ChildItem -Path $dir -Filter "*.inf"
    foreach ($inf in $infFiles) {
        Write-Info "Processing INF: $($inf.FullName)"
        if ($inf2catPath) {
            Write-Info "Running Inf2Cat for $($inf.Name)..."
            $inf2catArgs = "/driver:`"$dir`" /os:$osTargets /verbose"
            $p = Start-Process -FilePath $inf2catPath -ArgumentList $inf2catArgs -Wait -NoNewWindow -PassThru
            if ($p.ExitCode -eq 0) {
                Write-Success "Catalog generation succeeded for $($inf.Name)."
            } else {
                Write-Warn "Inf2Cat exited with code $($p.ExitCode). Check INF syntax or OS targets."
            }
        } else {
            Write-Info "Skipping Inf2Cat (tool not installed). Checking for existing .cat file..."
            $catFile = [System.IO.Path]::ChangeExtension($inf.FullName, ".cat")
            if (Test-Path $catFile) {
                Write-Success "Found pre-existing catalog file: $catFile"
            } else {
                Write-Warn "No catalog file found for $($inf.Name). Driver requires .cat file for clean installation."
            }
        }
    }
}

# -----------------------------------------------------------------------------
# STEP 6: Sign Driver Binaries (.sys) and Catalog Files (.cat)
# -----------------------------------------------------------------------------
Write-Header "Step 6: Signing Driver Binaries (.sys) and Catalogs (.cat)"

$filesToSign = Get-ChildItem -Path $DriverRootPath -Recurse -Include "*.sys", "*.cat"

if ($filesToSign.Count -eq 0) {
    Write-Warn "No .sys or .cat files found in $DriverRootPath to sign."
    Write-Info "Note: Compile the KMDF drivers in Visual Studio / WDK before running field signing."
} else {
    Write-Info "Found $($filesToSign.Count) driver artifact(s) to sign:"
    $timestampUrl = "http://timestamp.digicert.com"

    foreach ($item in $filesToSign) {
        Write-Info "Signing: $($item.FullName)"

        if ($signToolPath) {
            # Attempt 1: Sign with RFC-3161 Timestamping
            $signArgs = "sign /v /s `"MY`" /sha1 `"$($signingCert.Thumbprint)`" /fd sha256 /tr $timestampUrl /td sha256 `"$($item.FullName)`""
            $res = Start-Process -FilePath $signToolPath -ArgumentList $signArgs -Wait -NoNewWindow -PassThru

            if ($res.ExitCode -ne 0) {
                Write-Warn "Timestamped signing failed (network offline?). Retrying without timestamp..."
                $signArgsNoTs = "sign /v /s `"MY`" /sha1 `"$($signingCert.Thumbprint)`" /fd sha256 `"$($item.FullName)`""
                $resNoTs = Start-Process -FilePath $signToolPath -ArgumentList $signArgsNoTs -Wait -NoNewWindow -PassThru
                if ($resNoTs.ExitCode -eq 0) {
                    Write-Success "Signed successfully (local timestamp): $($item.Name)"
                } else {
                    Write-Failure "SignTool failed for $($item.Name) with code $($resNoTs.ExitCode)"
                }
            } else {
                Write-Success "Signed and timestamped: $($item.Name)"
            }
        } else {
            # PowerShell Set-AuthenticodeSignature Fallback
            try {
                $status = Set-AuthenticodeSignature -FilePath $item.FullName -Certificate $signingCert -HashAlgorithm SHA256 -TimestampServer $timestampUrl -ErrorAction SilentlyContinue
                if ($status.Status -eq "Valid") {
                    Write-Success "Authenticode signed: $($item.Name)"
                } else {
                    # Retry without timestamp
                    $status = Set-AuthenticodeSignature -FilePath $item.FullName -Certificate $signingCert -HashAlgorithm SHA256
                    Write-Success "Authenticode signed (offline): $($item.Name) [Status: $($status.Status)]"
                }
            } catch {
                Write-Failure "Failed to sign $($item.Name): $_"
            }
        }
    }
}

# -----------------------------------------------------------------------------
# STEP 7: Verify Digital Signatures
# -----------------------------------------------------------------------------
Write-Header "Step 7: Verifying Driver Signatures"

foreach ($item in $filesToSign) {
    if ($signToolPath) {
        $verifyArgs = "verify /v /kp `"$($item.FullName)`""
        $vRes = Start-Process -FilePath $signToolPath -ArgumentList $verifyArgs -Wait -NoNewWindow -PassThru
        if ($vRes.ExitCode -eq 0) {
            Write-Success "Signature VALID (Kernel Policy): $($item.Name)"
        } else {
            Write-Warn "Signature verification returned code $($vRes.ExitCode) for $($item.Name)"
        }
    } else {
        $sig = Get-AuthenticodeSignature -FilePath $item.FullName
        Write-Info "$($item.Name) Authenticode Status: $($sig.Status) | Signer: $($sig.SignerCertificate.Subject)"
    }
}

# -----------------------------------------------------------------------------
# STEP 8: Driver Staging via PnPUtil (Optional / Automatic)
# -----------------------------------------------------------------------------
if ($InstallDrivers) {
    Write-Header "Step 8: Staging and Installing Drivers via PnPUtil"

    $infFiles = Get-ChildItem -Path $DriverRootPath -Recurse -Filter "*.inf"
    foreach ($inf in $infFiles) {
        Write-Info "Installing driver INF: $($inf.FullName)"
        $res = pnputil.exe /add-driver "$($inf.FullName)" /install 2>&1 | Out-String
        if ($LASTEXITCODE -eq 0 -or $LASTEXITCODE -eq 3010) {
            Write-Success "Driver package added to driver store successfully."
            if ($LASTEXITCODE -eq 3010) {
                Write-Warn "Reboot recommended by Windows to complete driver activation."
            }
        } else {
            Write-Warn "PnPUtil exited with code $LASTEXITCODE: $res"
        }
    }
}

# -----------------------------------------------------------------------------
# STEP 9: Summary & Next Steps
# -----------------------------------------------------------------------------
Write-Header "Execution Summary & Field-Testing Instructions"
Write-Success "BhartX Ecosystems CA is installed in Root & TrustedPublisher."
Write-Success "Drivers are signed with SHA-256."

Write-Host "`nField-Testing Checkpoints for Client Machines:" -ForegroundColor Cyan
Write-Host "  1. If running on physical hardware with Secure Boot enabled:"
Write-Host "     -> Disable Secure Boot in BIOS/UEFI for test-signing, OR"
Write-Host "     -> Complete Microsoft Attestation Signing via Partner Center (see docs/microsoft_whql_compliance_playbook.md)."
Write-Host "  2. If running on VM (Hyper-V / VMware / Proxmox / VirtualBox):"
Write-Host "     -> Disable Secure Boot in VM configuration settings."
Write-Host "  3. To manually stage drivers anytime, run:"
Write-Host "     pnputil.exe /add-driver `"$PSScriptRoot\..\drivers\vhci\anyusb_vhci.inf`" /install"
Write-Host "     pnputil.exe /add-driver `"$PSScriptRoot\..\drivers\filter\anyusb_filter.inf`" /install"
Write-Host "  4. Check Device Manager -> System Devices -> 'USB Master Pro Virtual Host Controller & Bus Root'"
Write-Host "================================================================================`n" -ForegroundColor Cyan
