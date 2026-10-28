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
taskkill /F /IM usb_master_daemon.exe >nul 2>&1
taskkill /F /IM usb_master_cli.exe >nul 2>&1
taskkill /F /IM USBMasterDaemon.exe >nul 2>&1
taskkill /F /IM USBMasterPro.exe >nul 2>&1
timeout /t 1 /nobreak >nul

REM Resolve drivers directory (supports release bundle root and scripts/ directory)
set "DRIVER_DIR="
if exist "%~dp0drivers\vhci\anyusb_vhci.inf" (
    set "DRIVER_DIR=%~dp0drivers"
) else if exist "%~dp0..\drivers\vhci\anyusb_vhci.inf" (
    set "DRIVER_DIR=%~dp0..\drivers"
)

if not defined DRIVER_DIR (
    echo [ERROR] Drivers directory could not be located!
    echo Ensure drivers/vhci/anyusb_vhci.inf exists.
    pause
    exit /b 1
)

echo [USB Master Pro] Checking for Field-Testing CA Certificate...
if exist "%DRIVER_DIR%\bhartx_test_ca.cer" (
    echo [USB Master Pro] Trusting BhartX Ecosystems Field Testing CA in Root ^& TrustedPublisher...
    certutil -addstore -f "Root" "%DRIVER_DIR%\bhartx_test_ca.cer" >nul 2>&1
    certutil -addstore -f "TrustedPublisher" "%DRIVER_DIR%\bhartx_test_ca.cer" >nul 2>&1
)

echo [USB Master Pro] Dynamically staging Virtual USB Host Controller (anyusb_vhci)...
pnputil.exe /add-driver "%DRIVER_DIR%\vhci\anyusb_vhci.inf" /install
if %errorLevel% neq 0 (
    echo [NOTICE] If running in field test mode with test-signed drivers, run:
    if exist "%~dp0setup_field_test_signing.ps1" (
        echo          powershell -ExecutionPolicy Bypass -File "%~dp0setup_field_test_signing.ps1"
    ) else (
        echo          powershell -ExecutionPolicy Bypass -File "%~dp0scripts\setup_field_test_signing.ps1"
    )
)

echo [USB Master Pro] Dynamically staging Hardware Capture Filter Driver (anyusb_filter)...
pnputil.exe /add-driver "%DRIVER_DIR%\filter\anyusb_filter.inf" /install >nul 2>&1

echo [USB Master Pro] Configuring Windows Firewall Rules (Zero PowerShell Dependency)...
netsh advfirewall firewall delete rule name="USB Master Pro Daemon" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro CLI" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro" >nul 2>&1

REM Locate daemon and CLI binaries
set "BIN_DIR="
if exist "%~dp0usb_master_daemon.exe" (
    set "BIN_DIR=%~dp0"
) else if exist "%~dp0bin\usb_master_daemon.exe" (
    set "BIN_DIR=%~dp0bin\"
) else if exist "%~dp0..\bin\usb_master_daemon.exe" (
    set "BIN_DIR=%~dp0..\bin\"
) else if exist "%~dp0..\dist\bin\USBMasterDaemon.exe" (
    set "BIN_DIR=%~dp0..\dist\bin\"
)

if defined BIN_DIR (
    if exist "%BIN_DIR%usb_master_daemon.exe" (
        netsh advfirewall firewall add rule name="USB Master Pro Daemon" dir=in action=allow program="%BIN_DIR%usb_master_daemon.exe" enable=yes >nul 2>&1
        netsh advfirewall firewall add rule name="USB Master Pro Daemon" dir=out action=allow program="%BIN_DIR%usb_master_daemon.exe" enable=yes >nul 2>&1
    )
    if exist "%BIN_DIR%usb_master_cli.exe" (
        netsh advfirewall firewall add rule name="USB Master Pro CLI" dir=in action=allow program="%BIN_DIR%usb_master_cli.exe" enable=yes >nul 2>&1
        netsh advfirewall firewall add rule name="USB Master Pro CLI" dir=out action=allow program="%BIN_DIR%usb_master_cli.exe" enable=yes >nul 2>&1
    )
    if exist "%BIN_DIR%USBMasterDaemon.exe" (
        netsh advfirewall firewall add rule name="USB Master Pro Daemon" dir=in action=allow program="%BIN_DIR%USBMasterDaemon.exe" enable=yes >nul 2>&1
        netsh advfirewall firewall add rule name="USB Master Pro Daemon" dir=out action=allow program="%BIN_DIR%USBMasterDaemon.exe" enable=yes >nul 2>&1
    )
    if exist "%BIN_DIR%USBMasterPro.exe" (
        netsh advfirewall firewall add rule name="USB Master Pro" dir=in action=allow program="%BIN_DIR%USBMasterPro.exe" enable=yes >nul 2>&1
        netsh advfirewall firewall add rule name="USB Master Pro" dir=out action=allow program="%BIN_DIR%USBMasterPro.exe" enable=yes >nul 2>&1
    )
)

echo ==============================================================================
echo [SUCCESS] USB Master Pro installed smoothly! ZERO REBOOT REQUIRED.
if defined BIN_DIR (
    if exist "%BIN_DIR%usb_master_daemon.exe" (
        echo You can now launch: "%BIN_DIR%usb_master_daemon.exe"
    ) else if exist "%BIN_DIR%USBMasterPro.exe" (
        echo You can now launch: "%BIN_DIR%USBMasterPro.exe"
    )
)
echo ==============================================================================
exit /b 0
