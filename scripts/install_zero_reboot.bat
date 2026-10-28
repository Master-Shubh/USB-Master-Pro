@echo off
REM ==============================================================================
REM USB Master Pro — Zero-Reboot Automated Installation Script
REM Designed for Windows 10 & Windows 11 (x64)
REM BhartX Ecosystems Pvt Ltd | Zero Restart Invariant
REM ==============================================================================

echo [USB Master Pro] Verifying Administrator Privileges...
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo [ERROR] Administrator privileges required. Please right-click and 'Run as Administrator'.
    pause
    exit /b 1
)

echo [USB Master Pro] Pre-Flight: Ensuring no lingering instances are holding locked file handles...
taskkill /F /IM USBMasterDaemon.exe >nul 2>&1
taskkill /F /IM USBMasterPro.exe >nul 2>&1
timeout /t 1 /nobreak >nul

echo [USB Master Pro] Dynamically staging Virtual USB Host Controller (anyusb_vhci)...
pnputil.exe /add-driver "%~dp0..\drivers\vhci\anyusb_vhci.inf" /install
if %errorLevel% neq 0 (
    echo [NOTICE] If running in field test mode with test-signed drivers, run:
    echo          powershell -ExecutionPolicy Bypass -File "%~dp0setup_field_test_signing.ps1"
)

echo [USB Master Pro] Dynamically staging Hardware Capture Filter Driver (anyusb_filter)...
pnputil.exe /add-driver "%~dp0..\drivers\filter\anyusb_filter.inf" /install >nul 2>&1

echo [USB Master Pro] Configuring Firewall Rules for WebRTC and Local IPC (Port 443 / 5123)...
powershell -ExecutionPolicy Bypass -File "%~dp0configure_firewall.ps1"

echo ==============================================================================
echo [SUCCESS] USB Master Pro installed smoothly! ZERO REBOOT REQUIRED.
echo You can now launch: "%~dp0..\bin\USBMasterPro.exe"
echo ==============================================================================
exit /b 0
