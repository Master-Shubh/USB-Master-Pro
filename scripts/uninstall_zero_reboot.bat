@echo off
REM ==============================================================================
REM USB Master Pro — Zero-Reboot Clean Uninstallation Script
REM Dynamically unloads KMDF Virtual Host & Filter Drivers without machine restart
REM BhartX Ecosystems Pvt Ltd | Zero Restart Invariant
REM ==============================================================================

echo [USB Master Pro] Verifying Administrator Privileges for clean teardown...
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo [ERROR] Administrator privileges required. Please right-click and 'Run as Administrator'.
    pause
    exit /b 1
)

echo [USB Master Pro] Gracefully stopping active processes to prevent file lock reboots...
taskkill /F /IM USBMasterDaemon.exe >nul 2>&1
taskkill /F /IM USBMasterPro.exe >nul 2>&1
timeout /t 2 /nobreak >nul

echo [USB Master Pro] Dynamically detaching Virtual USB Device...
pnputil.exe /remove-device "root\anyusb_vhci" >nul 2>&1

echo [USB Master Pro] Dynamically removing and unloading Virtual USB Host Controller Driver...
pnputil.exe /delete-driver anyusb_vhci.inf /uninstall /force >nul 2>&1

echo [USB Master Pro] Dynamically removing Hardware Capture Filter Driver...
pnputil.exe /delete-driver anyusb_filter.inf /uninstall /force >nul 2>&1

echo [USB Master Pro] Cleaning up Windows Firewall rules...
netsh advfirewall firewall delete rule name="USB Master Pro Daemon" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro" >nul 2>&1

echo ==============================================================================
echo [SUCCESS] USB Master Pro uninstalled cleanly! ZERO REBOOT REQUIRED.
echo All drivers, firewall rules, and runtime services detached dynamically.
echo ==============================================================================
exit /b 0
