/**
 * USB Master Pro — RP2040 WinUSB Loopback Device Descriptors
 * Implements WCID 1.0 Microsoft OS Feature Descriptors to auto-bind WinUSB.SYS on Windows 10/11
 * Interface GUID: {dee824ef-729b-4a0e-9c14-b7117d33a817}
 */

#include "tusb.h"
#include "usb_descriptors.h"

// --------------------------------------------------------------------+
// Device Descriptor
// --------------------------------------------------------------------+
tusb_desc_device_t const desc_device = {
    .bLength            = sizeof(tusb_desc_device_t),
    .bDescriptorType    = TUSB_DESC_DEVICE,
    .bcdUSB             = 0x0200,

    .bDeviceClass       = 0x00,
    .bDeviceSubClass    = 0x00,
    .bDeviceProtocol    = 0x00,
    .bMaxPacketSize0    = CFG_TUD_ENDPOINT0_SIZE,

    .idVendor           = USB_VID,
    .idProduct          = USB_PID,
    .bcdDevice          = USB_BCD,

    .iManufacturer      = 0x01,
    .iProduct           = 0x02,
    .iSerialNumber      = 0x03,

    .bNumConfigurations = 0x01
};

uint8_t const * tud_descriptor_device_cb(void) {
    return (uint8_t const *) &desc_device;
}

// --------------------------------------------------------------------+
// Configuration Descriptor
// --------------------------------------------------------------------+
#define CONFIG_TOTAL_LEN  (TUD_CONFIG_DESC_LEN + TUD_VENDOR_DESC_LEN)

uint8_t const desc_configuration[] = {
    // Config number, interface count, string index, total length, attribute, power in mA
    TUD_CONFIG_DESCRIPTOR(1, ITF_NUM_TOTAL, 0, CONFIG_TOTAL_LEN, 0x80, 100),

    // Interface number, string index, EP Out & IN address, EP size
    TUD_VENDOR_DESCRIPTOR(ITF_NUM_VENDOR, 4, EPNUM_VENDOR_OUT, EPNUM_VENDOR_IN, 64)
};

uint8_t const * tud_descriptor_configuration_cb(uint8_t index) {
    (void) index;
    return desc_configuration;
}

// --------------------------------------------------------------------+
// String Descriptors
// --------------------------------------------------------------------+
char const *string_desc_arr[] = {
    (const char[]) { 0x09, 0x04 },         // 0: Supported language: English (0x0409)
    "USB Master Pro Engineering",          // 1: Manufacturer
    "WinUSB Loopback Verification Device",  // 2: Product
    "UMP-PICO-2026-0001",                  // 3: Serial
    "WinUSB High-Throughput Endpoint",     // 4: Interface 0 Name
};

static uint16_t _desc_str[64];

// Microsoft OS 1.0 String Descriptor at index 0xEE: "MSFT100" + Vendor Code 0x02
#define MSFT_OS_STRING_DESC_LEN 18
static uint8_t const desc_ms_os_string[] = {
    MSFT_OS_STRING_DESC_LEN,
    TUSB_DESC_STRING,
    'M', 0x00, 'S', 0x00, 'F', 0x00, 'T', 0x00,
    '1', 0x00, '0', 0x00, '0', 0x00,
    VENDOR_REQUEST_MICROSOFT,
    0x00
};

uint16_t const * tud_descriptor_string_cb(uint8_t index, uint16_t langid) {
    (void) langid;
    uint8_t count;

    if (index == 0xEE) {
        return (uint16_t const *) desc_ms_os_string;
    }

    if (index == 0) {
        memcpy(&_desc_str[1], string_desc_arr[0], 2);
        count = 1;
    } else {
        if (index >= sizeof(string_desc_arr) / sizeof(string_desc_arr[0])) return NULL;

        const char *str = string_desc_arr[index];
        count = strlen(str);
        if (count > 63) count = 63;

        for (uint8_t i = 0; i < count; i++) {
            _desc_str[1 + i] = str[i];
        }
    }

    _desc_str[0] = (TUSB_DESC_STRING << 8) | (2 * count + 2);
    return _desc_str;
}

// --------------------------------------------------------------------+
// Microsoft OS 1.0 Feature Descriptors (WCID)
// --------------------------------------------------------------------+
#define MS_COMPAT_ID_LEN 40
static uint8_t const desc_ms_compat_id[] = {
    // Header (16 bytes)
    0x28, 0x00, 0x00, 0x00,                         // dwLength (40 bytes)
    0x00, 0x01,                                     // bcdVersion = 1.00
    0x04, 0x00,                                     // wIndex = 0x0004 (Extended Compatible ID)
    0x01,                                           // bCount = 1 section
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,       // Reserved

    // Section 1: Interface 0 Compatible ID (24 bytes)
    ITF_NUM_VENDOR,                                 // bInterfaceNumber = 0
    0x01,                                           // Reserved
    'W', 'I', 'N', 'U', 'S', 'B', 0x00, 0x00,       // compatibleID = "WINUSB\0\0"
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,// subCompatibleID = NULL
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00              // Reserved
};

// Extended Properties: DeviceInterfaceGUIDs = "{dee824ef-729b-4a0e-9c14-b7117d33a817}\0"
#define MS_EXT_PROPERTY_LEN 142
static uint8_t const desc_ms_ext_property[] = {
    // Header (10 bytes)
    0x8E, 0x00, 0x00, 0x00,                         // dwLength (142 bytes)
    0x00, 0x01,                                     // bcdVersion = 1.00
    0x05, 0x00,                                     // wIndex = 0x0005 (Extended Properties)
    0x01, 0x00,                                     // wCount = 1 custom property

    // Custom Property 1 (132 bytes)
    0x84, 0x00, 0x00, 0x00,                         // dwSize = 132 bytes
    0x07, 0x00, 0x00, 0x00,                         // dwPropertyDataType = REG_MULTI_SZ (7)
    0x2A, 0x00,                                     // wPropertyNameLength = 42 bytes (21 wchar)
    // "DeviceInterfaceGUIDs\0" in UTF-16LE
    'D', 0, 'e', 0, 'v', 0, 'i', 0, 'c', 0, 'e', 0, 'I', 0, 'n', 0, 't', 0, 'e', 0, 'r', 0, 'f', 0, 'a', 0, 'c', 0, 'e', 0, 'G', 0, 'U', 0, 'I', 0, 'D', 0, 's', 0, 0, 0,
    0x50, 0x00, 0x00, 0x00,                         // dwPropertyDataLength = 80 bytes (40 wchar)
    // "{dee824ef-729b-4a0e-9c14-b7117d33a817}\0\0" in UTF-16LE
    '{', 0, 'd', 0, 'e', 0, 'e', 0, '8', 0, '2', 0, '4', 0, 'e', 0, 'f', 0, '-', 0,
    '7', 0, '2', 0, '9', 0, 'b', 0, '-', 0, '4', 0, 'a', 0, '0', 0, 'e', 0, '-', 0,
    '9', 0, 'c', 0, '1', 0, '4', 0, '-', 0, 'b', 0, '7', 0, '1', 0, '1', 0, '7', 0,
    'd', 0, '3', 0, '3', 0, 'a', 0, '8', 0, '1', 0, '7', 0, '}', 0, 0, 0, 0, 0
};

// --------------------------------------------------------------------+
// TinyUSB Vendor Control Request Handler (Intercepts Microsoft OS Descriptors)
// --------------------------------------------------------------------+
bool tud_vendor_control_xfer_cb(uint8_t rhport, uint8_t stage, tusb_control_request_t const * request) {
    if (stage != CONTROL_STAGE_SETUP) return true;

    // Handle Microsoft OS Feature Descriptors
    if (request->bRequest == VENDOR_REQUEST_MICROSOFT) {
        if (request->wIndex == 0x0004) {
            // Extended Compatible ID
            return tud_control_xfer(rhport, request, (void*)(uintptr_t)desc_ms_compat_id, sizeof(desc_ms_compat_id));
        } else if (request->wIndex == 0x0005) {
            // Extended Properties (Interface GUID)
            return tud_control_xfer(rhport, request, (void*)(uintptr_t)desc_ms_ext_property, sizeof(desc_ms_ext_property));
        }
    }

    // Custom Vendor Ping Request (bRequest = 0x42)
    if (request->bRequest == 0x42) {
        static char const ping_resp[] = "PICO_WINUSB_TEST_OK";
        return tud_control_xfer(rhport, request, (void*)(uintptr_t)ping_resp, sizeof(ping_resp));
    }

    return false; // Stall unhandled vendor requests
}
