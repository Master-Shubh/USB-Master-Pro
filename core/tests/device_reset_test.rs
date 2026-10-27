//! USB Device Reset State Machine Verification Suite
//! Verifies Soft Reset (Endpoint Halt Clear), Hard Reset (Bus Power Cycle & Re-enumeration),
//! and Replug Reset (Surprise Disconnect & PnP Hotplug Re-attachment).

use std::collections::{HashMap, HashSet};
use usb_master_core::buffer_pool::BufferPool;
use usb_master_core::hardware_db::{DeviceCategory, HardwareClassifier, FLAG_RD_SERVICE_OPTIMIZED};
use usb_master_core::protocol::*;
use usb_master_core::TransferType;

// Linux / POSIX USB Error Constants
pub const EPERM: i32 = -1;
pub const EBUSY: i32 = -16;
pub const ENODEV: i32 = -19;
pub const EPIPE: i32 = -32;
pub const ECONNRESET: i32 = -104;
pub const ETIMEDOUT: i32 = -110;

/// USB Device Operational States
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbState {
    Disconnected,
    DefaultAddress0,
    Addressed(u8),
    Configured,
    ActiveStreaming,
    HardResetting,
    Replugging,
}

/// Simulated USB Hardware Device Controller & State Machine
pub struct SimulatedUsbDevice {
    pub device_id: u32,
    pub vid: u16,
    pub pid: u16,
    pub address: u8,
    pub state: UsbState,
    pub stalled_endpoints: HashSet<u8>,
    pub data_toggles: HashMap<u8, bool>, // true = DATA1, false = DATA0
    pub in_flight_urbs: HashMap<u64, TurboUrbHeader>,
}

impl SimulatedUsbDevice {
    pub fn new(device_id: u32, vid: u16, pid: u16) -> Self {
        let mut dev = Self {
            device_id,
            vid,
            pid,
            address: 0,
            state: UsbState::Disconnected,
            stalled_endpoints: HashSet::new(),
            data_toggles: HashMap::new(),
            in_flight_urbs: HashMap::new(),
        };
        // Initialize data toggles
        dev.data_toggles.insert(0x00, false);
        dev.data_toggles.insert(0x81, false);
        dev.data_toggles.insert(0x82, false);
        dev
    }

    /// Attaches the device to the USB root hub (PnP attachment)
    pub fn attach(&mut self) -> Result<(), i32> {
        if self.state != UsbState::Disconnected && self.state != UsbState::Replugging {
            return Err(EBUSY);
        }
        self.state = UsbState::DefaultAddress0;
        self.address = 0;
        self.stalled_endpoints.clear();
        Ok(())
    }

    /// Detaches the device (Surprise removal / cable unplug)
    /// Aborts all in-flight URBs with ENODEV
    pub fn detach(&mut self) -> Vec<TurboUrbHeader> {
        self.state = UsbState::Disconnected;
        self.stalled_endpoints.clear();
        let mut cancelled = Vec::new();
        for (_, mut hdr) in self.in_flight_urbs.drain() {
            hdr.command = CMD_UNLINK_URB;
            hdr.status = ENODEV;
            cancelled.push(hdr);
        }
        cancelled
    }

    /// Set USB address (Part of standard enumeration)
    pub fn set_address(&mut self, addr: u8) -> Result<(), i32> {
        if self.state != UsbState::DefaultAddress0 {
            return Err(EPERM);
        }
        self.address = addr;
        self.state = UsbState::Addressed(addr);
        Ok(())
    }

    /// Set USB configuration (Activates endpoints)
    pub fn set_configuration(&mut self) -> Result<(), i32> {
        match self.state {
            UsbState::Addressed(_) => {
                self.state = UsbState::Configured;
                self.reset_all_data_toggles();
                Ok(())
            }
            _ => Err(EPERM),
        }
    }

    /// Transition to active streaming
    pub fn start_streaming(&mut self) -> Result<(), i32> {
        if self.state == UsbState::Configured {
            self.state = UsbState::ActiveStreaming;
            Ok(())
        } else {
            Err(EBUSY)
        }
    }

    /// Triggers a stall condition on a specific endpoint (e.g. CRC error, FIFO overflow)
    pub fn trigger_endpoint_stall(&mut self, ep: u8) {
        self.stalled_endpoints.insert(ep);
    }

    /// Soft Reset: Clears endpoint stall (USB CLEAR_FEATURE ENDPOINT_HALT)
    /// Resets the data toggle for this endpoint without disturbing overall device configuration
    pub fn clear_endpoint_halt(&mut self, ep: u8) -> Result<(), i32> {
        if self.state != UsbState::ActiveStreaming && self.state != UsbState::Configured {
            return Err(ENODEV);
        }
        if self.stalled_endpoints.remove(&ep) {
            // Reset toggle to DATA0
            self.data_toggles.insert(ep, false);
            Ok(())
        } else {
            // Endpoint wasn't stalled, but CLEAR_FEATURE is idempotent
            self.data_toggles.insert(ep, false);
            Ok(())
        }
    }

    /// Hard Reset: Bus-level SE0 line pull down (>10ms)
    /// Resets entire USB controller state to DefaultAddress0, unlinks all in-flight URBs
    pub fn hard_reset(&mut self) -> Vec<TurboUrbHeader> {
        self.state = UsbState::HardResetting;
        let mut cancelled = Vec::new();
        for (_, mut hdr) in self.in_flight_urbs.drain() {
            hdr.command = CMD_UNLINK_URB;
            hdr.status = ECONNRESET;
            cancelled.push(hdr);
        }
        self.stalled_endpoints.clear();
        self.reset_all_data_toggles();
        self.address = 0;
        self.state = UsbState::DefaultAddress0;
        cancelled
    }

    /// Replug Reset: Full unplug and re-enumeration sequence
    pub fn replug_reset(&mut self) -> (Vec<TurboUrbHeader>, UsbState) {
        self.state = UsbState::Replugging;
        let mut cancelled = Vec::new();
        for (_, mut hdr) in self.in_flight_urbs.drain() {
            hdr.command = CMD_UNLINK_URB;
            hdr.status = ENODEV;
            cancelled.push(hdr);
        }
        self.stalled_endpoints.clear();
        self.reset_all_data_toggles();
        self.address = 0;
        self.state = UsbState::Disconnected;

        // Auto-reattach upon reconnect
        let _ = self.attach();
        (cancelled, self.state)
    }

    /// Submit a URB to the device
    pub fn submit_urb(&mut self, hdr: TurboUrbHeader) -> Result<TurboUrbHeader, i32> {
        match self.state {
            UsbState::Disconnected => Err(ENODEV),
            UsbState::HardResetting | UsbState::Replugging => Err(EBUSY),
            UsbState::DefaultAddress0 | UsbState::Addressed(_) => {
                // Only Control transfers on EP0 allowed during pre-configuration
                if hdr.endpoint_address == 0x00 && hdr.transfer_type == TransferType::Control {
                    let mut resp = hdr;
                    resp.command = CMD_COMPLETE_URB;
                    resp.status = 0;
                    resp.actual_length = resp.buffer_length;
                    Ok(resp)
                } else {
                    Err(EBUSY)
                }
            }
            UsbState::Configured | UsbState::ActiveStreaming => {
                // Check if endpoint is stalled
                if self.stalled_endpoints.contains(&hdr.endpoint_address) {
                    let mut resp = hdr;
                    resp.command = CMD_COMPLETE_URB;
                    resp.status = EPIPE;
                    resp.actual_length = 0;
                    return Ok(resp);
                }

                // Toggle flip simulation
                let current_toggle = self.data_toggles.entry(hdr.endpoint_address).or_insert(false);
                *current_toggle = !*current_toggle;

                let mut resp = hdr;
                resp.command = CMD_COMPLETE_URB;
                resp.status = 0;
                resp.actual_length = resp.buffer_length;
                Ok(resp)
            }
        }
    }

    fn reset_all_data_toggles(&mut self) {
        for val in self.data_toggles.values_mut() {
            *val = false;
        }
    }
}

#[test]
fn test_soft_reset_endpoint_halt_recovery() {
    let mut dev = SimulatedUsbDevice::new(1, 0x27D0, 0x0041); // Mantra MFS100
    assert_eq!(dev.attach(), Ok(()));
    assert_eq!(dev.set_address(5), Ok(()));
    assert_eq!(dev.set_configuration(), Ok(()));
    assert_eq!(dev.start_streaming(), Ok(()));

    // Initial bulk transfer succeeds
    let urb1 = TurboUrbHeader::new(CMD_SUBMIT_URB, 101, 1, 0x81, TransferType::Bulk, 512);
    let resp1 = dev.submit_urb(urb1).expect("Initial URB submission failed");
    assert_eq!(resp1.status, 0);

    // Simulate endpoint 0x81 hardware stall (EPIPE)
    dev.trigger_endpoint_stall(0x81);

    // Further bulk reads on 0x81 now fail with EPIPE
    let urb2 = TurboUrbHeader::new(CMD_SUBMIT_URB, 102, 1, 0x81, TransferType::Bulk, 512);
    let resp2 = dev.submit_urb(urb2).unwrap();
    assert_eq!(resp2.status, EPIPE, "Stalled endpoint must return EPIPE");

    // Other endpoints (e.g. EP0 Control or EP 0x82) must NOT be blocked by EP 0x81 stall
    let ep0_urb = TurboUrbHeader::new(CMD_SUBMIT_URB, 103, 1, 0x00, TransferType::Control, 64);
    let ep0_resp = dev.submit_urb(ep0_urb).unwrap();
    assert_eq!(ep0_resp.status, 0, "EP0 Control must remain operational during EP 0x81 stall");

    // Host sends Soft Reset: USB CLEAR_FEATURE(ENDPOINT_HALT) for EP 0x81
    assert_eq!(dev.clear_endpoint_halt(0x81), Ok(()));
    assert!(!dev.stalled_endpoints.contains(&0x81));
    assert_eq!(dev.data_toggles.get(&0x81), Some(&false), "Data toggle must reset to DATA0");

    // Bulk transfer on EP 0x81 recovers and completes with status = 0
    let urb3 = TurboUrbHeader::new(CMD_SUBMIT_URB, 104, 1, 0x81, TransferType::Bulk, 512);
    let resp3 = dev.submit_urb(urb3).unwrap();
    assert_eq!(resp3.status, 0, "Bulk transfer must succeed after soft reset");
    assert_eq!(dev.state, UsbState::ActiveStreaming);
}

#[test]
fn test_hard_reset_bus_cycle_re_enumeration() {
    let mut dev = SimulatedUsbDevice::new(2, 0x08B0, 0x0031); // Morpho MSO 1300
    dev.attach().unwrap();
    dev.set_address(7).unwrap();
    dev.set_configuration().unwrap();
    dev.start_streaming().unwrap();

    // Queue 5 in-flight URBs
    for seq in 201..=205 {
        let hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, seq, 2, 0x82, TransferType::Bulk, 1024);
        dev.in_flight_urbs.insert(seq, hdr);
    }
    assert_eq!(dev.in_flight_urbs.len(), 5);

    // Host triggers Hard Reset (CMD_RESET_DEVICE / Bus power cycle)
    let cancelled = dev.hard_reset();

    // Verify all 5 in-flight URBs were aborted with ECONNRESET
    assert_eq!(cancelled.len(), 5);
    for urb in cancelled {
        assert_eq!(urb.command, CMD_UNLINK_URB);
        assert_eq!(urb.status, ECONNRESET);
    }
    assert_eq!(dev.in_flight_urbs.len(), 0);

    // Device state reverted to DefaultAddress0
    assert_eq!(dev.state, UsbState::DefaultAddress0);
    assert_eq!(dev.address, 0);

    // Bulk transfer attempts before re-enumeration must be rejected
    let bad_bulk = TurboUrbHeader::new(CMD_SUBMIT_URB, 206, 2, 0x82, TransferType::Bulk, 512);
    assert_eq!(dev.submit_urb(bad_bulk), Err(EBUSY));

    // Full Re-enumeration sequence
    // 1. EP0 Control Get Descriptor succeeds
    let get_desc = TurboUrbHeader::new(CMD_SUBMIT_URB, 207, 2, 0x00, TransferType::Control, 18);
    let desc_resp = dev.submit_urb(get_desc).unwrap();
    assert_eq!(desc_resp.status, 0);

    // 2. Set Address 12
    assert_eq!(dev.set_address(12), Ok(()));
    assert_eq!(dev.state, UsbState::Addressed(12));

    // 3. Set Configuration 1
    assert_eq!(dev.set_configuration(), Ok(()));
    assert_eq!(dev.state, UsbState::Configured);

    // 4. Start Streaming
    assert_eq!(dev.start_streaming(), Ok(()));

    // Normal bulk transfer resumes cleanly
    let ok_bulk = TurboUrbHeader::new(CMD_SUBMIT_URB, 208, 2, 0x82, TransferType::Bulk, 512);
    let resp = dev.submit_urb(ok_bulk).unwrap();
    assert_eq!(resp.status, 0);
}

#[test]
fn test_replug_surprise_removal_and_hotplug() {
    let mut dev = SimulatedUsbDevice::new(3, 0x27D0, 0x0041);
    dev.attach().unwrap();
    dev.set_address(3).unwrap();
    dev.set_configuration().unwrap();
    dev.start_streaming().unwrap();

    // 1. In-flight URBs active
    let hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, 301, 3, 0x81, TransferType::Bulk, 2048);
    dev.in_flight_urbs.insert(301, hdr);

    // 2. Surprise physical disconnect (cable pulled out)
    let unlinked = dev.detach();
    assert_eq!(unlinked.len(), 1);
    assert_eq!(unlinked[0].command, CMD_UNLINK_URB);
    assert_eq!(unlinked[0].status, ENODEV);
    assert_eq!(dev.state, UsbState::Disconnected);

    // Submitting URB when disconnected immediately returns ENODEV
    let blocked_urb = TurboUrbHeader::new(CMD_SUBMIT_URB, 302, 3, 0x81, TransferType::Bulk, 512);
    assert_eq!(dev.submit_urb(blocked_urb), Err(ENODEV));

    // 3. Cable re-plugged (Hotplug event)
    let (re_cancelled, new_state) = dev.replug_reset();
    assert_eq!(re_cancelled.len(), 0);
    assert_eq!(new_state, UsbState::DefaultAddress0);

    // 4. PnP Classifier verifies device upon replug
    let (cat, name, flags) = HardwareClassifier::classify(dev.vid, dev.pid, "Mantra MFS100");
    assert_eq!(cat, DeviceCategory::FingerprintScanner);
    assert!(name.contains("MFS100"));
    assert_ne!(flags & FLAG_RD_SERVICE_OPTIMIZED, 0);

    // 5. Enumeration & Session Restored
    assert_eq!(dev.set_address(4), Ok(()));
    assert_eq!(dev.set_configuration(), Ok(()));
    assert_eq!(dev.start_streaming(), Ok(()));

    // Normal traffic flows again
    let final_urb = TurboUrbHeader::new(CMD_SUBMIT_URB, 303, 3, 0x81, TransferType::Bulk, 512);
    let final_resp = dev.submit_urb(final_urb).unwrap();
    assert_eq!(final_resp.status, 0);
}

#[test]
fn test_buffer_pool_reclamation_during_reset() {
    let pool = BufferPool::new(16, 4096);
    let mut acquired_buffers = Vec::new();

    // Acquire 10 slabs simulating in-flight URB buffers
    for _ in 0..10 {
        let mut buf = pool.acquire();
        buf.as_mut_slice()[0] = 0xAA;
        acquired_buffers.push(buf);
    }

    // Reset occurs: All in-flight transfers cancelled, buffers dropped
    drop(acquired_buffers);

    // Verify all buffers are immediately available again from the pool
    for _ in 0..10 {
        let buf = pool.acquire();
        assert_eq!(buf.capacity(), 4096);
    }
}
