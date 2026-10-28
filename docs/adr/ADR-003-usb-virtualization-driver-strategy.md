# ADR-003: USB Virtualization Driver Architecture & Co-Existence Strategy

**Status**: Accepted  
**Date**: 2026-10-28  
**Author**: BhartX Architecture Guild  
**Decision Makers**: Antigravity (AI CTO & Commander), bhartx-coder  

---

## 1. Context & Problem Statement

To redirect USB peripherals over IP on Windows, software must:
1. Capture raw USB Request Blocks (URBs) from physical devices on the sharing computer.
2. Emulate a hardware USB Host Controller on the receiving computer so Windows PnP dynamically mounts the peripheral as if physically connected to a local motherboard port.

Legacy solutions (e.g. SimplyCore `tusbd.sys` / `tusbdbus.sys`) rely on ancient Windows Driver Model (WDM) architectures dating back to Windows XP. WDM drivers lack built-in power management, require manual IRP cancellation bookkeeping, and cause catastrophic system blue screens (BSOD) when both filter and bus drivers are active simultaneously.

---

## 2. Decision Drivers

1. **Kernel Framework Stability**: Modern KMDF (Kernel-Mode Driver Framework 1.33) with automated I/O queue management, power tracking, and Microsoft WHQL compliance.
2. **Driverless Device Sharing on Host**: The sharing computer must not require proprietary manufacturer drivers to capture URBs.
3. **Full OEM Driver Compatibility on Guest**: Receiving computer must dynamically bind official vendor drivers (e.g. Mantra RD Service, Morpho driver, `usbstor.sys`).
4. **Co-Existence & Anti-Loopback Invariant**: When a single computer acts as both Sender and Receiver, the filter driver must never intercept its own synthetic virtual devices.

---

## 3. Considered Options

* **Option 1: User-Mode WinUSB / LibUSB Passthrough**:
  - *Pros*: Runs completely in user mode; no custom kernel drivers required.
  - *Cons*: Cannot virtualize Isochronous streaming (UVC cameras), cannot capture devices already claimed by existing Windows class drivers, cannot synthesize new child PDOs.
* **Option 2: Legacy WDM Filter & Bus Drivers (SimplyCore Pattern)**:
  - *Pros*: Proven 2000-era technique.
  - *Cons*: Fragile IRP tracking, high risk of BSOD on surprise unplug, fails modern Microsoft Driver Guard and HVCI integrity checks.
* **Option 3: Modern KMDF 1.33 Dual-Driver Architecture**:
  - *Host*: Upper Filter Driver (`anyusb_filter.sys`) attached surgically to target device instances.
  - *Guest*: Virtual Host Controller Driver (`anyusb_vhci.sys`) synthesizing dynamic child Physical Device Objects (`UPDO`).

---

## 4. Decision Outcome

**Chosen Option: Option 3 (Modern KMDF 1.33 Dual-Driver Architecture)**.

### Architectural Breakdown:

```
HOST SHARING MACHINE                               GUEST MOUNTING MACHINE
┌─────────────────────────────────┐               ┌─────────────────────────────────┐
│ Target Physical Device Instance │               │ Windows Device Manager & PnP    │
│ (e.g. Mantra MFS100 / USB Key)  │               │ (Loads official Mantra driver)  │
└────────────────┬────────────────┘               └────────────────▲────────────────┘
                 │ (Raw URB IRPs)                                  │ (Native IRPs)
┌────────────────▼────────────────┐               ┌────────────────┴────────────────┐
│ anyusb_filter.sys (KMDF 1.33)   │               │ anyusb_vhci.sys (KMDF 1.33)     │
│ • Intercepts SUBMIT_URB         │               │ • Emulates Root USB Controller  │
│ • Targeted Instance Attachment  │               │ • Synthesizes Child UPDOs       │
│ • Locks Non-Paged Pool Memory   │               │ • Calls InvalidateDeviceRelation│
└─────────────────────────────────┘               └─────────────────────────────────┘
```

### Self-Exclusion Guarantee:
To eliminate recursive driver attachment and BSOD loops, `anyusb_vhci.sys` labels its synthetic bus enumerator as `LOGOS_VUSB`. `anyusb_filter.sys` checks this property during `EvtDeviceAdd` and immediately returns `STATUS_NOT_SUPPORTED` for any `LOGOS_VUSB` device node.
