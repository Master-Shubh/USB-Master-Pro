@echo off
REM ==============================================================================
REM USB Master Pro — Automated Driver Staging & Silent Installation Script
REM Windows 10 & Windows 11 (x64 / ARM64)
REM ==============================================================================

echo [USB Master Pro] Checking Administrator Privileges...
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo [ERROR] Administrative permissions required. Please right click and 'Run as Administrator'.
    pause
    exit /b 1
)

echo [USB Master Pro] Installing Virtual USB Host Controller Driver (anyusb_vhci)...
pnputil.exe /add-driver "%~dp0..\drivers\vhci\anyusb_vhci.inf" /install
if %errorLevel% neq 0 (
    echo [WARNING] pnputil returned code %errorLevel%. If running in field test mode, run:
    echo            powershell -ExecutionPolicy Bypass -File "%~dp0setup_field_test_signing.ps1"
)

echo [USB Master Pro] Installing Hardware Capture Upper Filter Driver (anyusb_filter)...
pnputil.exe /add-driver "%~dp0..\drivers\filter\anyusb_filter.inf" /install

echo [USB Master Pro] Configuring Outbound Firewall Rules (Port 443 WebRTC/QUIC)...
powershell -ExecutionPolicy Bypass -File "%~dp0configure_firewall.ps1"

echo [USB Master Pro] Driver Installation and Firewall Setup Complete!
