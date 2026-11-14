# ADR-011: Lean Architecture Lock — WinUSB User-Mode Capture & KMDF VHCI Production Model

**Status**: Accepted & Supercedes ADR-003  
**Date**: 2026-11-15  
**Author**: BhartX Architecture Guild  
**Decision Makers**: Founder, Chief Technology Officer (CTO), Product Management  

---

## 1. Context & Problem Statement

ADR-003 originally specified a dual-driver model using a KMDF Upper Filter Driver (`anyusb_filter.sys`) on the Sender (Host A) and a Virtual Host Controller Interface (`anyusb_vhci.sys`) on the Receiver (Host B).

During engineering implementation and field validation, critical architectural risks were established:
1. **Host Stability & BSOD Risk:** Attaching an upper filter driver dynamically to arbitrary physical USB device stacks in kernel mode risks system crashes (BSOD) if a third-party vendor driver conflicts during power-state transitions or surprise removal.
2. **Installation Friction:** Filter drivers on physical device stacks require administrative INF modifications, class filter registry edits, and trigger Windows Driver Signature Enforcement / reboot requirements.
3. **Core Target Alignment:** 95% of target peripherals for Indian e-Governance, BFSI, and logistics (Mantra MFS100, Morpho MSO1300, Startek, RP2040, POS printers) operate reliably via standard Bulk and Control transfers supported natively by Microsoft's WinUSB subsystem.

---

## 2. Decision Outcome

**Strategic Lock: Adopt WinUSB User-Mode Bridge (Sender) + KMDF VHCI (Receiver) as the sole production USB redirection architecture for USB Master Pro v1.0.**

### Architectural Specification:

```
┌────────────────────────────────────────────────────────┐       ┌────────────────────────────────────────────────────────┐
│                   HOST A (SENDER)                      │       │                  HOST B (RECEIVER)                     │
│  Physical machine where real USB device is plugged in  │       │   Remote machine where native software needs device    │
│                                                        │       │                                                        │
│  [Physical Hardware] (e.g. Mantra MFS100 / RP2040)     │       │  [Native OEM App] (e.g. Aadhaar RD Service / POS App)  │
│          ▲                                             │       │          ▲                                             │
│          │ Win32 WinUSB Direct FFI                     │       │          │ Standard Windows USBD API Calls             │
│          ▼                                             │       │          ▼                                             │
│  [USBMasterPro.exe / Daemon] (100% User-Mode)          │       │  [anyusb_vhci.sys] (KMDF Virtual Bus Root Driver)      │
│  - SetupAPI Device Enumeration                         │       │  - Child UPDO: USB\VID_xxxx&PID_xxxx                   │
│  - Overlapped I/O Pump                                 │       │  - Synthesizes Configuration Descriptors & Pipes       │
│  - Zero Kernel Driver Installed on Host A              │       │          ▲                                             │
│          ▲                                             │       │          │ IOCTL_ANYUSB_VHCI_FETCH / COMPLETE_URB      │
│          │                                             │       │          ▼                                             │
│          │                                             │       │  [USBMasterPro.exe / Daemon] (Receiver Client Engine)  │
│          │                                             │       │  - Unpacks Turbo-URB wire frames                       │
│          │                                             │       │  - Dispatches completions to KMDF                      │
└──────────┼─────────────────────────────────────────────┘       └──────────┼─────────────────────────────────────────────┘
           │                                                                │
           └───────────────────► [ZERO-KNOWLEDGE TUNNEL] ◄──────────────────┘
                                 - Turbo-URB Framing (36B Header + Setup Packet)
                                 - Zstandard Adaptive Compression
                                 - Authenticated End-to-End Encryption (E2EE)
```

---

## 3. Detailed Component Disposition

1. **Host A (Sender):**
   - Pure user-mode capture via `core/src/winusb/` (`WinUsb_Initialize`, `WinUsb_ControlTransfer`, `WinUsb_ReadPipe`, `WinUsb_WritePipe`).
   - ZERO kernel drivers installed on the Sender PC.
2. **Host B (Receiver):**
   - Single production driver: `drivers/vhci/anyusb_vhci.sys` (KMDF 1.33).
   - Manages virtual USB root hub and child PDOs with kernel-level `URB_FUNCTION_SELECT_CONFIGURATION` and `URB_FUNCTION_SELECT_INTERFACE` handle synthesis.
3. **Filter Driver (`drivers/filter/`):**
   - Excluded from production installation packages (`installer/usb_master_pro.iss`).
   - Source code retained in repository under experimental/legacy status for future Isochronous/UVC research.
4. **Adapter Framework (`core/src/adapters/`):**
   - Retained as certified device compatibility profiles with generic Turbo-URB as the default engine.
5. **Security & Cryptography:**
   - Cryptographic boundaries strictly enforced: End-to-end encryption (E2EE) on Turbo-URB data payload ensures the signaling broker operates as a zero-knowledge pipe with zero access to biometric/plaintext USB bytes.

---

## 4. Consequences

### Positive:
- **Zero BSOD Risk on Sender:** No kernel code runs on Host A.
- **Faster Installation:** Sender requires zero driver installation; instant launch.
- **Smaller Installer:** Excludes unused filter driver package.
- **Full Indian e-Gov Compatibility:** Verified descriptor and pipe handling for Mantra MFS100, Morpho MSO1300, and RP2040 loopback test fixtures.

### Negative / Trade-Offs:
- Devices requiring Isochronous streaming (e.g. UVC webcams) are deferred to post-v1.0. (Bulk and Control devices, covering 100% of target biometrics and crypto tokens, are fully supported).
