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

### 2.1 Single Biometric Fingerprint Scanners (UIDAI Aadhaar L0 & L1 Certified)

Optimized for instant fingerprint minutiae/bitmap extraction across UIDAI Aadhaar, Ayushman Bharat PMJAY, DigiSeva, CSC, and Banking Kiosks (AePS).

| Vendor | Model / Hardware Name | Vendor ID (VID) | Product ID (PID) | Transfer Profile | Turbo-URB Flags | RD Service Status & SLA |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **Mantra Softech** | MFS100 Optical Fingerprint Scanner | `0x27D0` | `0x0041` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0 Certified** (< 150ms) |
| **Mantra Softech** | MFS100 Scanner (Alternate Firmware) | `0x27D0` | `0x0040` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0 Certified** (< 150ms) |
| **Mantra Softech** | MFS110 L1 Biometric Fingerprint Scanner | `0x27D0` | `0x0110` | Bulk IN (64B/512B) / Interrupt | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L1 Certified** (< 120ms) |
| **Mantra Softech** | MFS110 L1 Optical Scanner (Alt PID) | `0x27D0` | `0x0043` | Bulk IN (64B/512B) / Interrupt | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L1 Certified** (< 120ms) |
| **Mantra Softech** | MFS500 Optical Fingerprint Scanner | `0x27D0` | `0x0050` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0/L1 Certified** (< 140ms) |
| **IDEMIA / Morpho** | Morpho MSO 1300 E2 Optical Sensor | `0x08B0` | `0x0031` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0 Certified** (< 150ms) |
| **IDEMIA / Morpho** | Morpho MSO 1300 E3 Smart Card + Bio | `0x079B` | `0x0031` | Composite Bulk IN / CCID | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0 Certified** (< 150ms) |
| **IDEMIA / Morpho** | MorphoSmart MSO 1300 E2 | `0x079B` | `0x0045` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0 Certified** (< 150ms) |
| **IDEMIA / Morpho** | MorphoSmart MSO 1300 E3 (L0/L1) | `0x079B` | `0x0047` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0/L1 Certified** (< 130ms) |
| **IDEMIA / Morpho** | MorphoSmart MSO 1300 E3 L1 Secure | `0x079B` | `0x0048` | Bulk IN (512B) Secure Crypto | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L1 Certified** (< 120ms) |
| **Startek** | FM220U Optical Fingerprint Scanner | `0x1FBA` | `0x0013` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0 Certified** (< 150ms) |
| **Startek** | FM220U L1 Biometric Scanner | `0x1FBA` | `0x0014` | Bulk IN (512B) Hardware PKI | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L1 Certified** (< 120ms) |
| **Startek** | FM220 Fingerprint Scanner | `0x0BCA` | `0x8220` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0 Certified** (< 160ms) |
| **SecuGen** | Hamster Pro 20 (HU20 Optical Sensor) | `0x1162` | `0x0320` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0/L1 Certified** (< 140ms) |
| **SecuGen** | Hamster Pro 10 (HU10 Compact Reader) | `0x1162` | `0x0330` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0/L1 Certified** (< 140ms) |
| **SecuGen** | Hamster IV Biometric Reader | `0x1162` | `0x2200` | Control / Bulk IN (512B) | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI L0 Certified** (< 160ms) |

### 2.2 Dual Iris Scanners (e-KYC & Ayushman Bharat Certified)

Dual-eye simultaneous infrared capture and live streaming with microsecond-paced isochronous jitter dampening.

| Vendor | Model / Hardware Name | Vendor ID (VID) | Product ID (PID) | Transfer Profile | Turbo-URB Optimization Flags | RD Service SLA & Status |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **Mantra Softech** | MIS100V2 Dual Iris Scanner | `0x27D0` | `0x0042` | Composite (Bulk IN + Isoch) | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI L0 Iris** (< 180ms frame) |
| **Mantra Softech** | MATISX Dual Iris Scanner (L1 Edition) | `0x27D0` | `0x0045` | Composite (Bulk IN + Isoch) | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI L1 Iris** (< 150ms frame) |
| **CMITECH** | BMT-20 Dual Iris Scanner | `0x2B89` | `0x0020` | High-Speed Bulk IN + Isoch | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI Certified** (< 175ms frame) |
| **CMITECH** | EMX-30 Dual Iris Scanner | `0x2B89` | `0x0030` | High-Speed Bulk IN + Isoch | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI Certified** (< 160ms frame) |
| **IriTech** | IriShield MK2120U Dual Iris Camera | `0x2655` | `0x0110` | Composite (Bulk IN + Isoch) | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI L0 Iris** (< 180ms frame) |
| **IriTech** | IriUniverse / BK2121U Dual Iris Scanner | `0x2655` | `0x0120` | Composite (Bulk IN + Isoch) | `RD_SERVICE` \| `ISOCH_JITTER` | **UIDAI L1 Iris** (< 150ms frame) |
| **Iris ID** | iCAM7000 Series Iris Imager | `0x1E58` | `0x0010` | Composite Bulk/Isoch | `RD_SERVICE` \| `ISOCH_JITTER` | **Tier 1 Certified** (< 180ms frame) |

### 2.3 Fingerprint Slap Scanners (4:4:2 Ten-Print for Aadhaar & Ayushman Bharat)

High-throughput 500 DPI optical slap scanners streaming uncompressed simultaneous 4-finger and two-thumb biometric frames (~12 MB/s burst throughput).

| Vendor | Model / Hardware Name | Vendor ID (VID) | Product ID (PID) | Transfer Profile | Turbo-URB Flags | Capture SLA & Status |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **Mantra Softech** | MFS600 4:4:2 Ten-Print Slap Scanner | `0x27D0` | `0x0060` | High-Speed Bulk IN Burst | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI Slap Certified** (< 300ms) |
| **Mantra Softech** | MFS600 Slap Scanner (Optical Alt) | `0x27D0` | `0x0061` | High-Speed Bulk IN Burst | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI Slap Certified** (< 300ms) |
| **IDEMIA / Morpho** | MorphoTop 100 Ten-Print Slap Scanner | `0x079B` | `0x0033` | High-Speed Bulk IN Burst | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI Slap Certified** (< 280ms) |
| **IDEMIA / Morpho** | MorphoTop MORPHS Ten-Print Scanner | `0x079B` | `0x0034` | High-Speed Bulk IN Burst | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI Slap Certified** (< 280ms) |
| **IDEMIA / Morpho** | MorphoTop 100 Live Scanner (Alt VID) | `0x08B0` | `0x0033` | High-Speed Bulk IN Burst | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI Slap Certified** (< 280ms) |
| **Thales / 3M Cogent** | Cogent CS500e Ten-Print Slap Scanner | `0x05BA` | `0x000A` | High-Speed Bulk IN Burst | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI Slap Certified** (< 300ms) |
| **Thales / 3M Cogent** | Cogent CSD550 Slap Scanner | `0x05BA` | `0x000B` | High-Speed Bulk IN Burst | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI Slap Certified** (< 300ms) |
| **Thales / 3M Cogent** | DactyScan84c 4:4:2 Slap Scanner | `0x05BA` | `0x0010` | High-Speed Bulk IN Burst | `FLAG_RD_SERVICE_OPTIMIZED` | **UIDAI Slap Certified** (< 250ms) |

### 2.4 GPS / GNSS Geotagging Receivers (UIDAI Aadhaar ECMP Mandatory Geofencing)

Sub-1ms COM port baud synchronization ensuring zero dropped NMEA sentences ($GPGGA, $GPRMC, $GPGSA) for mandatory station location binding.

| Vendor / Chipset | Model / Hardware Name | Vendor ID (VID) | Product ID (PID) | Transfer Profile | Turbo-URB Flags | Fix Cadence & Geofence SLA |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **GlobalSat** | BU-353-S4 USB GPS Receiver (SiRF IV) | `0x067B` | `0x2303` | USB CDC / Virtual COM (4800-115200) | `FLAG_LOW_LATENCY_SERIAL` | **1 Hz Fix Cadence** (SLA < 50ms sync) |
| **GlobalSat** | G-Star IV GPS Receiver (Prolific PL2303) | `0x067B` | `0x23A3` | USB CDC / Virtual COM (4800-115200) | `FLAG_LOW_LATENCY_SERIAL` | **1 Hz Fix Cadence** (SLA < 50ms sync) |
| **u-blox** | 7 / 8 / M8 GNSS High-Precision Engine | `0x1546` | `0x01A7` | USB CDC-ACM / UBX Binary & NMEA | `FLAG_LOW_LATENCY_SERIAL` | **1-10 Hz Fix** (SLA < 20ms sync) |
| **BioEnable / u-blox** | G2 GNSS Geotagging Receiver | `0x1546` | `0x01A8` | USB CDC-ACM / NMEA 0183 | `FLAG_LOW_LATENCY_SERIAL` | **1-5 Hz Fix** (SLA < 30ms sync) |
| **Generic USB NMEA** | CP2102 / FTDI / CH340 GPS Dongles | Various | Various | USB-to-UART Bridge / NMEA 0183 | `FLAG_LOW_LATENCY_SERIAL` | **1 Hz Fix Cadence** (SLA < 50ms sync) |

### 2.5 Real-Time RD Service & Geotagging SLA Specification Matrix

All Indian e-Governance operations hosted over USB Master Pro conform to the following end-to-end performance and privacy parameters:

```
+---------------------------------------------------------------------------------------------------------------+
|                                INDIAN E-GOVERNANCE HARDWARE SLA MATRIX                                        |
+---------------------------------------------------------------------------------------------------------------+
| Hardware Class         | Target Transfer Profile   | Max Frame Latency | Packet Loss SLA | Turbo-URB Buffer   |
+------------------------+---------------------------+-------------------+-----------------+--------------------+
| Single Biometrics (L1) | 512B Bulk IN Anticipatory | < 120 ms          | 0.000% Loss     | 4 Microframes      |
| Single Biometrics (L0) | 512B Bulk IN Anticipatory | < 150 ms          | 0.000% Loss     | 4 Microframes      |
| Dual Iris Scanners     | Isochronous Video Stream  | < 180 ms @ 30 FPS | < 0.001% Jitter | Microsecond Pacing |
| 4:4:2 Slap Scanners    | 12 MB/s Burst Bulk IN     | < 300 ms          | 0.000% Loss     | 8-Ring Anticipator |
| GPS / GNSS Geotagging  | Sub-1ms COM Sync (VCP)    | < 50 ms sentence  | 0.000% Drop     | Low-Latency Serial |
+---------------------------------------------------------------------------------------------------------------+
| Compliance Invariant: Zero-Retention Mandate enforced in anyusb_filter.sys — No raw buffers written to disk. |
+---------------------------------------------------------------------------------------------------------------+
```

### 2.6 Point-of-Sale (POS) Thermal Receipt & Barcode Printers

| Vendor | Model / Hardware Name | Vendor ID (VID) | Product ID (PID) | USB Class | Turbo-URB Flags | Tested Print Speed |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **TVS Electronics** | RP3200 Plus Thermal POS Receipt Printer | `0x0FE6` | `0x811E` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 250 mm/sec |
| **Epson** | TM-T88V High-Speed Receipt Printer | `0x04B8` | `0x0202` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 300 mm/sec |
| **Epson** | TM-T88VI Cloud-Enabled POS Printer | `0x04B8` | `0x0E15` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 350 mm/sec |
| **TSC Auto ID** | TTP-244 Pro Barcode Label Printer | `0x1203` | `0x0111` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 127 mm/sec |
| **HP Inc.** | LaserJet Pro M404dn Monochrome Printer | `0x03F0` | `0x5D11` | Class 0x07 (Printer) | `FLAG_BULK_OUT_BURST` | 40 ppm |

### 2.7 Industrial Serial & COM Port Adapters

| Chipset Vendor | Chipset Model | Vendor ID (VID) | Product ID (PID) | Driver Model | Turbo-URB Flags | Max Verified Baud |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **FTDI Chip** | FT232R USB-to-UART Bridge | `0x0403` | `0x6001` | FTDIBUS / VCP | `LOW_LATENCY_SERIAL` | 3,000,000 baud |
| **FTDI Chip** | FT2232H Dual High-Speed UART/FIFO | `0x0403` | `0x6010` | FTDIBUS / D2XX | `LOW_LATENCY_SERIAL` | 12,000,000 baud |
| **WCH (WinChipHead)** | CH340 / CH340G USB-to-Serial | `0x1A86` | `0x7523` | CH341S64.SYS | `LOW_LATENCY_SERIAL` | 2,000,000 baud |
| **Silicon Labs** | CP2102 USB-to-UART Bridge | `0x10C4` | `0xEA60` | silabser.sys | `LOW_LATENCY_SERIAL` | 1,000,000 baud |

### 2.8 Hardware Security Tokens & SmartCards (CCID)

| Vendor | Product Family | Vendor ID (VID) | Product ID (PID) | Protocol | Compliant Standards | Status |
| :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| **Yubico** | YubiKey 5 Series (5 NFC / 5C) | `0x1050` | `0x0407` | USB CCID (0x0B) | FIDO2 / U2F / PIV / OpenPGP | **Certified** |
| **Feitian Technologies** | ePass2003 PKI Token | `0x096E` | `0x0807` | USB CCID (0x0B) | CCA India DSC / PKCS#11 | **Certified** |
| **Watchdata Technologies** | PROXKey USB Token | `0x1B8F` | `0x0029` | USB CCID (0x0B) | CCA India DSC / CSP / PKCS#11 | **Certified** |
| **HID Global** | Omnikey 3121 USB SmartCard Reader | `0x076B` | `0x3021` | USB CCID (0x0B) | ISO 7816 Class A, B, C | **Certified** |

### 2.9 UVC Webcams & Imaging Peripherals

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
