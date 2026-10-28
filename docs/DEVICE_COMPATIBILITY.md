# USB Master Pro — Device Compatibility Matrix & Certification Specification

**Document Version**: 1.0.0  
**Classification**: Hardware Engineering & Quality Assurance Specification  
**Author**: BhartX Hardware Engineering & Systems Integration Guild  
**Status**: Certified Production Matrix  

---

## 1. Hardware Compatibility Tiers

USB Master Pro classifies physical USB peripherals into four operational tiers based on virtualization stability, real-time latency tolerance, driver behavior, and security impact.

```
+----------------------------------------------------------------------------------------------------+
|                                    COMPATIBILITY TIERS                                             |
+----------------------------------------------------------------------------------------------------+
|                                                                                                    |
|  [ TIER 1: Certified & Hardware-Tested ]                                                           |
|  • Rigorously tested in BhartX hardware labs across Windows 10 and Windows 11.                     |
|  • Full Turbo-URB pipeline optimization (Anticipatory read ring, Zstd-1 compression).              |
|  • Guaranteed 100% pass on official vendor diagnostic tools and e-Governance RD Services.          |
|                                                                                                    |
|  [ TIER 2: Compatible Standard USB Class ]                                                         |
|  • Peripherals conforming to USB-IF standardized class specifications (UVC, USB Mass Storage).     |
|  • Operates out of the box using native Windows class drivers (usbstor.sys, usbvideo.sys).        |
|  • Supports standard bulk, interrupt, and isochronous transfers.                                  |
|                                                                                                    |
|  [ TIER 3: Vendor-Specific & Complex Composite ]                                                   |
|  • High-throughput multi-function hardware (Custom DSP, multi-interface composite tokens).         |
|  • May require vendor-specific drivers installed on the mounting client machine.                   |
|  • Requires manual verification of proprietary vendor timeouts.                                    |
|                                                                                                    |
|  [ RESTRICTED / BLOCKED: System Critical Devices ]                                                 |
|  • Primary host human interface devices (keyboards, system mice, internal hub controllers).        |
|  • Shielded by default to prevent operator lockout or accidental loopback recursion.              |
|                                                                                                    |
+----------------------------------------------------------------------------------------------------+
```

---

## 2. Certified Hardware Matrix (Tier 1 & Tier 2)

### 2.1 Biometric Fingerprint Scanners (Indian e-Governance & Aadhaar UIDAI Certified)

| Vendor | Model / Hardware Name | Vendor ID (VID) | Product ID (PID) | Transfer Type | Turbo-URB Optimization Flags | RD Service Status |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **Mantra Softech** | MFS100 Optical Fingerprint Scanner | `0x27D0` | `0x0041` | Control / Bulk IN | `FLAG_RD_SERVICE_OPTIMIZED` | **Certified (L0/L1)** |
| **Mantra Softech** | MFS100 Scanner (Alternate Firmware) | `0x27D0` | `0x0040` | Control / Bulk IN | `FLAG_RD_SERVICE_OPTIMIZED` | **Certified (L0)** |
| **Mantra Softech** | MFS500 Fingerprint Scanner | `0x27D0` | `0x0050` | Control / Bulk IN | `FLAG_RD_SERVICE_OPTIMIZED` | **Certified (L0/L1)** |
| **IDEMIA / Morpho** | Morpho MSO 1300 E2 Optical Sensor | `0x08B0` | `0x0031` | Control / Bulk IN | `FLAG_RD_SERVICE_OPTIMIZED` | **Certified (L0)** |
| **IDEMIA / Morpho** | Morpho MSO 1300 E3 Biometric Reader | `0x079B` | `0x0047` | Control / Bulk IN | `FLAG_RD_SERVICE_OPTIMIZED` | **Certified (L0/L1)** |
| **Startek** | FM220U Biometric Fingerprint Scanner | `0x1FBA` | `0x0013` | Control / Bulk IN | `FLAG_RD_SERVICE_OPTIMIZED` | **Certified (L0/L1)** |
| **SecuGen** | Hamster Pro 20 (HU20 Optical Sensor) | `0x1162` | `0x0320` | Control / Bulk IN | `FLAG_RD_SERVICE_OPTIMIZED` | **Certified (L0/L1)** |
| **SecuGen** | Hamster IV Optical Fingerprint Sensor | `0x1162` | `0x0330` | Control / Bulk IN | `FLAG_RD_SERVICE_OPTIMIZED` | **Certified (L0)** |

### 2.2 Dual Iris Scanners (e-KYC & Ayushman Bharat Certified)

| Vendor | Model / Hardware Name | Vendor ID (VID) | Product ID (PID) | Transfer Type | Turbo-URB Optimization Flags | Certification Level |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **Mantra Softech** | MIS100V2 Dual Iris Scanner | `0x27D0` | `0x0042` | Composite (Bulk + Isoch) | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI L0 Iris** |
| **Mantra Softech** | MIS100V2 Dual Iris Scanner (L1 Edition) | `0x27D0` | `0x0045` | Composite (Bulk + Isoch) | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI L1 Iris** |
| **IriTech** | IriShield MK2120U Dual Iris Camera | `0x2655` | `0x0110` | Composite (Bulk + Isoch) | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI L0 Iris** |
| **Iris ID** | iCAM7000 Series Iris Imager | `0x1E58` | `0x0010` | Composite Bulk/Isoch | `RD_SERVICE` \| `ISOCH_JITTER` | **Tier 1 Certified** |

### 2.3 Point-of-Sale (POS) Thermal Receipt & Barcode Printers

| Vendor | Model / Hardware Name | Vendor ID (VID) | Product ID (PID) | USB Class | Turbo-URB Flags | Tested Print Speed |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **TVS Electronics** | RP3200 Plus Thermal POS Receipt Printer | `0x0FE6` | `0x811E` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 250 mm/sec |
| **Epson** | TM-T88V High-Speed Receipt Printer | `0x04B8` | `0x0202` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 300 mm/sec |
| **Epson** | TM-T88VI Cloud-Enabled POS Printer | `0x04B8` | `0x0E15` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 350 mm/sec |
| **TSC Auto ID** | TTP-244 Pro Barcode Label Printer | `0x1203` | `0x0111` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 127 mm/sec |
| **HP Inc.** | LaserJet Pro M404dn Monochrome Printer | `0x03F0` | `0x5D11` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 40 ppm |

### 2.4 Industrial Serial & COM Port Adapters

| Chipset Vendor | Chipset Model | Vendor ID (VID) | Product ID (PID) | Driver Model | Turbo-URB Flags | Max Verified Baud |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **FTDI Chip** | FT232R USB-to-UART Bridge | `0x0403` | `0x6001` | FTDIBUS / VCP | `LOW_LATENCY_SERIAL` | 3,000,000 baud |
| **FTDI Chip** | FT2232H Dual High-Speed UART/FIFO | `0x0403` | `0x6010` | FTDIBUS / D2XX | `LOW_LATENCY_SERIAL` | 12,000,000 baud |
| **WCH (WinChipHead)** | CH340 / CH340G USB-to-Serial | `0x1A86` | `0x7523` | CH341S64.SYS | `LOW_LATENCY_SERIAL` | 2,000,000 baud |
| **Silicon Labs** | CP2102 USB-to-UART Bridge | `0x10C4` | `0xEA60` | silabser.sys | `LOW_LATENCY_SERIAL` | 1,000,000 baud |
| **Prolific** | PL2303 USB-to-Serial Controller | `0x067B` | `0x2303` | ser2pl64.sys | `LOW_LATENCY_SERIAL` | 921,600 baud |

### 2.5 Hardware Security Tokens & SmartCards (CCID)

| Vendor | Product Family | Vendor ID (VID) | Product ID (PID) | Protocol | Compliant Standards | Status |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **Yubico** | YubiKey 5 Series (5 NFC / 5C) | `0x1050` | `0x0407` | USB CCID (0x0B) | FIDO2 / U2F / PIV / OpenPGP | **Certified** |
| **Feitian Technologies** | ePass2003 PKI Token | `0x096E` | `0x0807` | USB CCID (0x0B) | CCA India DSC / PKCS#11 | **Certified** |
| **Watchdata Technologies** | PROXKey USB Token | `0x1B8F` | `0x0029` | USB CCID (0x0B) | CCA India DSC / CSP / PKCS#11 | **Certified** |
| **HID Global** | Omnikey 3121 USB SmartCard Reader | `0x076B` | `0x3021` | USB CCID (0x0B) | ISO 7816 Class A, B, C | **Certified** |

### 2.6 UVC Webcams & Imaging Peripherals

| Manufacturer | Hardware Model | Vendor ID (VID) | Product ID (PID) | Video Format | Turbo-URB Flags | Resolution / FPS |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **Logitech** | HD Pro Webcam C920 | `0x046D` | `0x082D` | MJPEG / H.264 / YUY2 | `ISOCH_JITTER_BUFFER` | 1080p @ 30fps |
| **Logitech** | C922 Pro Stream Webcam | `0x046D` | `0x085C` | MJPEG / YUY2 | `ISOCH_JITTER_BUFFER` | 1080p @ 30fps / 720p @ 60fps |
| **Realtek** | Integrated HD UVC Camera | `0x0BDA` | `0x58F4` | YUY2 / MJPEG | `ISOCH_JITTER_BUFFER` | 720p @ 30fps |
| **OBSBOT** | Tiny 4K AI-Powered Webcam | `0x2E1A` | `0x0001` | MJPEG / NV12 | `ISOCH_JITTER_BUFFER` | 4K @ 30fps |

---

## 3. Restricted & Blocked System Devices

To protect system integrity, prevent recursive blue-screen loopbacks, and avoid operator lockout, the following hardware classes are shielded from automated or unauthenticated sharing:

```
+----------------------------------------------------------------------------------------------------+
|                               BLOCKED / RESTRICTED HARDWARE RULES                                  |
+----------------------------------------------------------------------------------------------------+
|                                                                                                    |
|  1. Primary System Keyboards (USB HID Class 0x03, SubClass 0x01, Protocol 0x01):                  |
|     • Threat: Sharing the primary keyboard immediately locks the host operator out of typing.      |
|     • Policy: EXCLUDED by default. Requires explicit double-confirmation with admin elevation.     |
|                                                                                                    |
|  2. Primary System Mice & Pointing Devices (USB HID Class 0x03, SubClass 0x01, Protocol 0x02):     |
|     • Threat: Sharing the primary mouse strips cursor control from the sharing PC.                 |
|     • Policy: EXCLUDED by default.                                                                 |
|                                                                                                    |
|  3. Host USB Root Hubs & xHCI Controller Bus Devices (USB\ROOT_HUB* / PCI\CC_0C0330):              |
|     • Threat: Attempting to virtualize the root controller crashes the Windows USB bus subsystem.  |
|     • Policy: HARD BLOCKED in kernel filter driver (EvtDeviceAdd returns STATUS_ACCESS_DENIED).    |
|                                                                                                    |
|  4. Synthetic Bus Loopback Nodes (DEVPKEY_Device_EnumeratorName == 'LOGOS_VUSB'):                  |
|     • Threat: Filter driver intercepting its own synthetic VHCI UPDO causes an infinite loop/BSOD. |
|     • Policy: HARD BLOCKED in anyusb_filter.sys (Self-Exclusion Invariant).                        |
|                                                                                                    |
+----------------------------------------------------------------------------------------------------+
```
