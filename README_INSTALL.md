# USB Master Pro — Windows Release Installation & Operations Guide

**Product**: USB Master Pro (Turbo-URB USB-over-IP Engine)  
**Company**: BhartX Ecosystems Pvt Ltd ([https://bhartx.in](https://bhartx.in))  
**Target OS**: Windows 10 (1809+), Windows 11 (all versions), Windows Server 2019/2022/2025  
**Architecture**: x64 (AMD64 Native)  
**Classification**: Enterprise Dynamic Driver Virtual USB Distribution

---

## 1. Release Bundle Overview

This portable release bundle (`usb-master-pro-windows-x64.zip`) contains all necessary user-mode binaries, kernel drivers, installation automation, and security certificates for USB Master Pro.

### Bundle Contents
```text
usb-master-pro-windows-x64/
├── usb_master_daemon.exe          # High-performance Turbo-URB background server
├── usb_master_cli.exe             # Interactive command-line client & control utility
├── install_zero_reboot.bat        # 1-Click dynamic driver administrator installation
├── uninstall_zero_reboot.bat      # 1-Click clean dynamic driver teardown
├── setup_field_test_signing.ps1   # PowerShell field-testing & CA management utility
├── README_INSTALL.md              # This technical operations document
└── drivers/
    ├── bhartx_test_ca.cer         # BhartX Ecosystems Field Testing CA public certificate
    ├── vhci/
    │   ├── anyusb_vhci.sys        # Virtual USB Host Controller & Bus KMDF driver
    │   ├── anyusb_vhci.inf        # Plug and Play installation manifest
    │   └── anyusb_vhci.cat        # Cryptographic security catalog
    └── filter/
        ├── anyusb_filter.sys      # Hardware Capture Upper Filter KMDF driver
        ├── anyusb_filter.inf      # Filter installation manifest
        └── anyusb_filter.cat      # Cryptographic security catalog
```

---

## 2. Dynamic Driver Installation Architecture

Unlike legacy USB virtualization software that locks class filters into the Windows registry requiring unconditional restarts, **USB Master Pro** leverages **Dynamic Driver Installation** (reboot may be required if system or locked files require it):
- Drivers are dynamically staged into the Windows Driver Store via `pnputil.exe`.
- Virtual device nodes (`root\anyusb_vhci`) are created and detached dynamically at runtime.
- Inbound and outbound firewall rules are configured on-the-fly using native `netsh advfirewall`.
- Drivers are uninstalled cleanly via dynamic detachment (reboot required only if system or file locks persist).

---

## 3. Fast-Track Installation (1-Click)

### Step 1: Extract Release Archive
Extract `usb-master-pro-windows-x64.zip` to your target directory (e.g. `C:\Program Files\BhartX\USB Master Pro` or any local folder).

### Step 2: Run Automated Installer
1. Right-click `install_zero_reboot.bat`.
2. Select **"Run as Administrator"**.
3. The script will automatically:
   - Ensure clean process state without file handle locks.
   - Import `bhartx_test_ca.cer` into Windows `Root` and `TrustedPublisher` certificate stores.
   - Dynamically stage `anyusb_vhci.inf` and `anyusb_filter.inf`.
   - Register Windows Firewall rules for daemon and CLI communication.
   - Confirm successful installation via dynamic staging (reboot may be required if system or locked files require it).

---

## 4. Field-Testing & TestSigning Configuration

For development machines, testing rigs, and pilot client deployments using field-test-signed drivers:

### Prerequisites for Field-Testing Mode:
- **Physical PCs**: UEFI Secure Boot must be disabled in BIOS to allow test-signed kernel drivers, OR drivers must be submitted to Microsoft Partner Center for Attestation Signing (see `docs/microsoft_whql_compliance_playbook.md`).
- **Virtual Machines** (Hyper-V / VMware / Proxmox / VirtualBox): Disable Secure Boot in VM hardware settings.

### Automated Setup Script:
Open an elevated PowerShell console (Run as Administrator) in the extracted folder:
```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
.\setup_field_test_signing.ps1 -InstallDrivers
```

This script will:
1. Enable Windows Test-Signing mode: `bcdedit.exe /set testsigning on`.
2. Verify or generate the **BhartX Ecosystems Field Testing CA**.
3. Inject the CA into `LocalMachine\Root` and `LocalMachine\TrustedPublisher`.
4. Validate that `.sys` and `.cat` files pass kernel signature policy checks.
5. Dynamically stage driver packages into the Windows Driver Store.

---

## 5. Usage & Operations Guide

### Starting the Daemon Service
The daemon runs the Turbo-URB engine and provides the WebSocket/HTTP control plane:
```cmd
usb_master_daemon.exe
```
By default, the daemon listens on `127.0.0.1:8080` (or `0.0.0.0:8080` for networked operation).

### Discovering Physical USB Peripherals
To inspect connected USB devices on the host:
```cmd
usb_master_cli.exe devices
```

### Sharing a Local USB Peripheral Over IP
To share a connected hardware device (e.g., biometric scanner, printer, token):
```cmd
usb_master_cli.exe share --bus 1 --port 2
```

### Attaching to a Remote Shared USB Device
On the client workstation (where the virtual device should appear as physically plugged in):
```cmd
usb_master_cli.exe attach --server ws://<remote-ip>:8080 --device-id <device-uuid>
```

### Verifying Device Node in Windows Device Manager
1. Press `Win + X` -> select **Device Manager** (`devmgmt.msc`).
2. Expand **System Devices**.
3. Confirm the entry:  
   `USB Master Pro Virtual Host Controller & Bus Root`
4. Expand **Universal Serial Bus controllers** when a device is remotely attached to verify dynamic mounting.

---

## 6. Clean Dynamic Driver Uninstallation

To cleanly and completely remove USB Master Pro without unnecessary rebooting:
1. Right-click `uninstall_zero_reboot.bat`.
2. Select **"Run as Administrator"**.
3. The script will:
   - Terminate active `usb_master_daemon.exe` and `usb_master_cli.exe` tasks.
   - Dynamically detach synthetic PnP device `root\anyusb_vhci`.
   - Force-uninstall and delete driver packages from Driver Store.
   - Flush and remove all registered Windows Firewall rules.
   - Complete clean uninstallation (reboot required only if system or file locks persist).

---

## 7. Troubleshooting & Verification Matrix

| Symptom | Probable Cause | Immediate Remediation |
| :--- | :--- | :--- |
| **Windows Error Code 52 in Device Manager** | Kernel driver signature not recognized under active policy | Run `setup_field_test_signing.ps1`. If physical PC, ensure UEFI Secure Boot is disabled for test builds. |
| **Firewall blocking inbound connection** | Windows Firewall policy blocked port | Re-run `install_zero_reboot.bat` as Administrator, or run: `netsh advfirewall firewall add rule name="USB Master Pro" dir=in action=allow program="<path>\usb_master_daemon.exe"` |
| **Driver installation prompts for reboot** | An active process was holding a handle to `anyusb_vhci.sys` | Run `taskkill /F /IM usb_master_daemon.exe` and retry `install_zero_reboot.bat`. |
| **Remote device mounted but requires vendor driver** | Guest Windows does not have manufacturer driver installed | Install the vendor's Windows driver software (e.g. Mantra RD Service, Morpho, printer driver) on the client machine. |

---

## 8. Enterprise Support & Legal

**Company**: BhartX Ecosystems Pvt Ltd  
**Website**: [https://bhartx.in](https://bhartx.in)  
**Technical Support**: [admin@bhartx.in](mailto:admin@bhartx.in)  
**Engineering Guild**: Logos - AI OS Cloud & Systems Architecture Team  
**Copyright**: (c) 2026 BhartX Ecosystems Pvt Ltd. All rights reserved.
