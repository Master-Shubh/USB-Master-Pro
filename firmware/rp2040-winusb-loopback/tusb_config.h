/**
 * USB Master Pro — RP2040 WinUSB Loopback Device
 * TinyUSB Configuration Header
 */

#ifndef _TUSB_CONFIG_H_
#define _TUSB_CONFIG_H_

#ifdef __cplusplus
extern "C" {
#endif

// Mode: Device
#define CFG_TUD_ENABLED       1
#define CFG_TUSB_MCU          OPT_MCU_RP2040

// Device OS: None (Bare Metal)
#define CFG_TUSB_OS           OPT_OS_NONE

// Device Speed: Full Speed (12 Mbps)
#define CFG_TUD_MAX_SPEED     OPT_MODE_FULL_SPEED

// Endpoints: 1 Control + 1 Bulk IN + 1 Bulk OUT
#define CFG_TUD_ENDPOINT_MAX  3

// Vendor Class Driver Enabled for Raw WinUSB Loopback
#define CFG_TUD_VENDOR        1
#define CFG_TUD_VENDOR_RX_BUFSIZE 512
#define CFG_TUD_VENDOR_TX_BUFSIZE 512

#ifdef __cplusplus
}
#endif

#endif // _TUSB_CONFIG_H_
