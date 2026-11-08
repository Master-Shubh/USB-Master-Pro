/**
 * USB Master Pro — RP2040 WinUSB Loopback Test Firmware
 * High-Throughput Bulk IN / Bulk OUT Loopback & Diagnostic Counter
 *
 * Microcontroller: Raspberry Pi Pico (RP2040)
 * Stack: TinyUSB + Pico SDK
 */

#include <stdio.h>
#include "pico/stdlib.h"
#include "bsp/board.h"
#include "tusb.h"
#include "usb_descriptors.h"

// Static double-buffer for Bulk Loopback (512 bytes)
static uint8_t loopback_buffer[512];
static uint32_t total_bytes_echoed = 0;
static uint32_t total_packets_echoed = 0;

void loopback_task(void) {
    // Check if data is available on Bulk OUT endpoint
    if (tud_vendor_available()) {
        uint32_t count = tud_vendor_read(loopback_buffer, sizeof(loopback_buffer));
        if (count > 0) {
            // Echo data directly back to Bulk IN endpoint
            uint32_t written = tud_vendor_write(loopback_buffer, count);
            tud_vendor_write_flush();

            total_bytes_echoed += written;
            total_packets_echoed++;

            // Heartbeat blink on packet activity
            board_led_write((total_packets_echoed % 2) != 0);
        }
    }
}

int main(void) {
    board_init();

    // Initialize TinyUSB stack
    tusb_init();

    while (1) {
        tud_task();      // TinyUSB device task
        loopback_task(); // Echo loopback task
    }

    return 0;
}

// Invoked when device is mounted
void tud_mount_cb(void) {
    board_led_write(true);
}

// Invoked when device is unmounted
void tud_umount_cb(void) {
    board_led_write(false);
}

// Invoked when usb bus is suspended
void tud_suspend_cb(bool remote_wakeup_en) {
    (void) remote_wakeup_en;
    board_led_write(false);
}

// Invoked when usb bus is resumed
void tud_resume_cb(void) {
    board_led_write(true);
}
