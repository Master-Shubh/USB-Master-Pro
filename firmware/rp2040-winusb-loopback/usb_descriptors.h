/**
 * USB Master Pro — RP2040 WinUSB Loopback Device Descriptors Header
 */

#ifndef _USB_DESCRIPTORS_H_
#define _USB_DESCRIPTORS_H_

#include "tusb.h"

// VID & PID for USB Master Pro Pico Test Device
#define USB_VID   0x2E8A // Raspberry Pi
#define USB_PID   0x4002 // USB Master Pro WinUSB Test Device
#define USB_BCD   0x0100 // Version 1.0.0

// Endpoints
#define EPNUM_VENDOR_OUT  0x01
#define EPNUM_VENDOR_IN   0x81

// Microsoft OS 1.0 Descriptors Constants
#define VENDOR_REQUEST_WEBUSB 0x01
#define VENDOR_REQUEST_MICROSOFT 0x02

#define ITF_NUM_VENDOR 0
#define ITF_NUM_TOTAL  1

#endif // _USB_DESCRIPTORS_H_
