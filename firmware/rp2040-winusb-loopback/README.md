# USB Master Pro — RP2040 WinUSB Loopback Verification Firmware

This firmware turns a standard **Raspberry Pi Pico (RP2040)** into an automated WinUSB test harness for validating Gate P1-A4 of the USB Master Pro redirection pipeline.

---

## Hardware Specifications & Descriptors

- **Microcontroller:** Raspberry Pi Pico (RP2040)
- **Vendor ID (VID):** `0x2E8A` (Raspberry Pi)
- **Product ID (PID):** `0x4002` (USB Master Pro WinUSB Verification Peripheral)
- **Driver Model:** Standard Microsoft WinUSB (`WINUSB.SYS`) via automated WCID 1.0 OS Descriptors (Zero driver installation required on Windows 10/11)
- **Device Interface GUID:** `{dee824ef-729b-4a0e-9c14-b7117d33a817}`
- **Endpoints:**
  - Control Endpoint 0 (64 bytes)
  - Bulk OUT Endpoint `0x01` (64 bytes Full-Speed)
  - Bulk IN Endpoint `0x81` (64 bytes Full-Speed)

---

## Measurable Acceptance Criteria (Pass/Fail Gates)

| Test ID | Test Description | Evaluation Method | Measurable Pass Criterion |
| :--- | :--- | :--- | :--- |
| **AC-01** | **Automatic WinUSB Binding** | Windows Device Manager / SetupAPI | Device binds to `WINUSB.SYS` with GUID `{dee824ef-729b-4a0e-9c14-b7117d33a817}` without manual `.inf` install. |
| **AC-02** | **Sender Detection & Claim** | Daemon `/api/devices` & SetupAPI | Daemon auto-discovers device `2E8A:4002` and successfully claims interface handle via `WinUsb_Initialize`. |
| **AC-03** | **Vendor Control Transfer** | Control GET `bRequest=0x42` | Returns exact string `"PICO_WINUSB_TEST_OK"` across virtual bus without corruption. |
| **AC-04** | **Bulk Echo Data Integrity** | Bulk OUT `0x01` ➔ Bulk IN `0x81` | 100,000 packets of variable payload (1 to 512 bytes) echoed with **0 byte corruption** (`hash(IN) == hash(OUT)`). |
| **AC-05** | **Round-Trip Latency** | LAN Loopback (Sender ➔ Receiver) | Mean RTT under 2.5 ms; 99th percentile under 8.0 ms on standard 1 Gbps LAN. |
| **AC-06** | **Physical Unplug Recovery** | Physical cable disconnect during active stream | Daemon transitions session to `Closing ➔ Ready` within 3.0 seconds; no crash, no kernel hang, no BSOD. |
| **AC-07** | **Physical Reconnect Recovery** | Cable re-insertion | Device re-appears in catalog within 2.0 seconds; share toggle functions immediately without daemon restart. |

---

## Build & Flashing Instructions

### Prerequisites
- Raspberry Pi Pico SDK (`PICO_SDK_PATH` environment variable configured)
- CMake 3.13+
- GCC ARM Toolchain (`arm-none-eabi-gcc`)

### Compilation
```bash
mkdir build
cd build
cmake -DPICO_SDK_PATH=/path/to/pico-sdk ..
cmake --build . --config Release
```
This produces `pico_winusb_loopback.uf2`.

### Flashing
1. Hold down the **BOOTSEL** button on the Raspberry Pi Pico while plugging it into a USB port.
2. The Pico will mount as a mass-storage drive named `RPI-RP2`.
3. Drag and drop `pico_winusb_loopback.uf2` into the drive.
4. The Pico will reboot immediately into the WinUSB Loopback mode (onboard LED will turn solid green on USB mount).

---

## Verification Test Commands

Run the test suite from the USB Master Pro repository root:

```powershell
# 1. Enumerate WinUSB devices on local host
cargo run -p usb_master_cli -- list-winusb

# 2. Run automated loopback data integrity test
cargo test -p usb_master_core --test winusb_hardware_loopback_test -- --nocapture
```
