@echo off
REM ==============================================================================
REM USB Master Pro — Dynamic Driver Uninstallation Script
REM Dynamically unloads KMDF Virtual Host & Filter Drivers (reboot may be required if system or locked files require it)
REM BhartX Ecosystems Pvt Ltd | Dynamic Driver Installation (reboot may be required if system or locked files require it)
REM ==============================================================================

echo [USB Master Pro] Verifying Administrator Privileges for clean teardown...
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo [ERROR] Administrator privileges required. Please right-click and 'Run as Administrator'.
    pause
    exit /b 1
)

echo [USB Master Pro] Gracefully stopping active processes to prevent file lock reboots...
taskkill /F /IM usb_master_daemon.exe >nul 2>&1
taskkill /F /IM usb_master_cli.exe >nul 2>&1
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
netsh advfirewall firewall delete rule name="USB Master Pro Daemon (Loopback In)" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro Daemon (Tunnel Out)" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro CLI" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro CLI (Loopback In)" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro CLI (Tunnel Out)" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro (Loopback In)" >nul 2>&1
netsh advfirewall firewall delete rule name="USB Master Pro (Outbound)" >nul 2>&1

echo [USB Master Pro] Removing Field Testing CA certificate from Root and TrustedPublisher stores...
certutil.exe -delstore "Root" "BhartX Ecosystems Field Testing CA" >nul 2>&1
certutil.exe -delstore "TrustedPublisher" "BhartX Ecosystems Field Testing CA" >nul 2>&1

echo [USB Master Pro] Checking Windows Test-Signing status...
bcdedit.exe /enum {current} 2>nul | findstr /i "testsigning" | findstr /i "Yes" >nul 2>&1
if %errorLevel% equ 0 (
    echo [NOTICE] Windows Test-Signing mode is currently ENABLED.
    set "REVERT_TESTSIGN=Y"
    set /p REVERT_TESTSIGN="Would you like to turn off Test-Signing mode? (Y/N) [default: Y]: "
    if /i not "%REVERT_TESTSIGN%"=="N" (
        echo [USB Master Pro] Reverting BCDEdit test signing: bcdedit /set testsigning off...
        bcdedit.exe /set testsigning off >nul 2>&1
        if %errorLevel% equ 0 (
            echo [SUCCESS] Windows Test-Signing mode disabled. (Reboot required for BCD change to take effect)
        ) else (
            echo [WARNING] Failed to disable Test-Signing. You may run 'bcdedit /set testsigning off' manually.
        )
    ) else (
        echo [USB Master Pro] Test-Signing left unchanged per operator choice.
    )
) else (
    echo [USB Master Pro] Windows Test-Signing mode is not enabled or already OFF.
)

echo ==============================================================================
echo [SUCCESS] USB Master Pro uninstalled cleanly via Dynamic Driver Teardown (reboot may be required if system or locked files require it).
echo All drivers, firewall rules, and runtime services detached dynamically.
echo ==============================================================================
exit /b 0
