//! USB Device Reset State Machine: Soft Reset, Hard Reset, and Simulated Replug
//! Implements the three production reset mechanisms extracted from forensic analysis of
//! SimplyCore USB Redirector v6.12 (`ResetMethods\<DevInstance>`):
//! - Mode 0 (Soft Reset): Endpoint Halt Clear (USB CLEAR_FEATURE ENDPOINT_HALT), FIFO flush, DATA0 toggle reset.
//! - Mode 1 (Hard Reset): USB Root Hub Port Reset signaling (SE0 line asserted >10ms), reverts to DefaultAddress0.
//! - Mode 2 (Simulated Replug): Virtual Bus PDO surprise removal (`IRP_MN_SURPRISE_REMOVAL`), debounce delay,
//!   and PnP hotplug re-enumeration (`IoInvalidateDeviceRelations`).

use crate::protocol::{TransferType, TurboUrbHeader, CMD_UNLINK_URB};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub const DEFAULT_HARD_RESET_SE0_MS: u64 = 20;    // USB 2.0 Spec §7.1.7.5: SE0 asserted min 10ms, typ 20ms
pub const DEFAULT_REPLUG_DEBOUNCE_MS: u64 = 100;  // Debounce delay to let host OS class drivers unload

// Standard POSIX / Kernel Error Codes
pub const EPERM: i32 = -1;
pub const EBUSY: i32 = -16;
pub const ENODEV: i32 = -19;
pub const EPIPE: i32 = -32;
pub const ECONNRESET: i32 = -104;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ResetMethod {
    SoftReset = 0,
    HardReset = 1,
    SimulatedReplug = 2,
}

impl From<u32> for ResetMethod {
    fn from(val: u32) -> Self {
        match val {
            0 => ResetMethod::SoftReset,
            1 => ResetMethod::HardReset,
            2 => ResetMethod::SimulatedReplug,
            _ => ResetMethod::SoftReset,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceOperationalState {
    Disconnected,
    DefaultAddress0,
    Addressed(u8),
    Configured(u8),
    ActiveStreaming,
    SoftResetting,
    HardResetting,
    SurpriseRemoved,
    ReplugDebouncing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelledUrbInfo {
    pub sequence_id: u64,
    pub endpoint_address: u8,
    pub status: i32,
    pub reason: String,
}

/// Reset State Machine for a single physical or synthetic USB device
pub struct DeviceResetController {
    pub device_id: u32,
    pub state: DeviceOperationalState,
    pub active_method: Option<ResetMethod>,
    pub address: u8,
    pub active_config: u8,
    pub stalled_endpoints: HashSet<u8>,
    reset_start_time: Option<Instant>,
    hard_reset_duration: Duration,
    replug_debounce_duration: Duration,
    pub in_flight_urbs: Vec<TurboUrbHeader>,
    pub total_soft_resets: u64,
    pub total_hard_resets: u64,
    pub total_replug_resets: u64,
}

impl DeviceResetController {
    pub fn new(device_id: u32) -> Self {
        Self {
            device_id,
            state: DeviceOperationalState::Disconnected,
            active_method: None,
            address: 0,
            active_config: 0,
            stalled_endpoints: HashSet::new(),
            reset_start_time: None,
            hard_reset_duration: Duration::from_millis(DEFAULT_HARD_RESET_SE0_MS),
            replug_debounce_duration: Duration::from_millis(DEFAULT_REPLUG_DEBOUNCE_MS),
            in_flight_urbs: Vec::new(),
            total_soft_resets: 0,
            total_hard_resets: 0,
            total_replug_resets: 0,
        }
    }

    /// Attaches the device initially to the virtual host bus
    pub fn attach_initial(&mut self) -> Result<(), i32> {
        if self.state != DeviceOperationalState::Disconnected {
            return Err(EBUSY);
        }
        self.state = DeviceOperationalState::DefaultAddress0;
        self.address = 0;
        self.active_config = 0;
        self.stalled_endpoints.clear();
        Ok(())
    }

    /// Completes the enumeration handshake: assigns an address
    pub fn set_address(&mut self, address: u8) -> Result<(), i32> {
        match self.state {
            DeviceOperationalState::DefaultAddress0 => {
                self.address = address;
                self.state = DeviceOperationalState::Addressed(address);
                Ok(())
            }
            _ => Err(EPERM),
        }
    }

    /// Completes configuration: activates endpoint pipelines
    pub fn set_configuration(&mut self, config: u8) -> Result<(), i32> {
        match self.state {
            DeviceOperationalState::DefaultAddress0
            | DeviceOperationalState::Addressed(_)
            | DeviceOperationalState::Configured(_) => {
                self.active_config = config;
                self.state = DeviceOperationalState::Configured(config);
                self.stalled_endpoints.clear();
                Ok(())
            }
            _ => Err(EPERM),
        }
    }

    /// Transitions to active biometric/bulk/isoch streaming state
    pub fn start_streaming(&mut self) -> Result<(), i32> {
        match self.state {
            DeviceOperationalState::Configured(_) | DeviceOperationalState::ActiveStreaming => {
                self.state = DeviceOperationalState::ActiveStreaming;
                Ok(())
            }
            _ => Err(EBUSY),
        }
    }

    // -------------------------------------------------------------------------
    // Reset Triggers
    // -------------------------------------------------------------------------

    /// Mode 0: Soft Reset (Endpoint Halt Clear / FIFO flush)
    /// Non-disruptive: resets specific endpoint stall without aborting unrelated pipes
    pub fn trigger_soft_reset(&mut self, endpoint: u8) -> Result<(), i32> {
        if !self.is_connected() {
            return Err(ENODEV);
        }
        self.total_soft_resets += 1;
        self.stalled_endpoints.remove(&endpoint);
        Ok(())
    }

    /// Mode 1: Hard Reset (USB Bus Line SE0 Port Reset)
    /// Pulls USB D+/D- lines low (SE0) for duration_ms.
    /// Aborts all in-flight URBs with ECONNRESET.
    /// Device reverts to unconfigured DefaultAddress0.
    pub fn trigger_hard_reset(&mut self) -> Vec<CancelledUrbInfo> {
        self.total_hard_resets += 1;
        self.state = DeviceOperationalState::HardResetting;
        self.active_method = Some(ResetMethod::HardReset);
        self.reset_start_time = Some(Instant::now());
        self.stalled_endpoints.clear();
        self.address = 0;
        self.active_config = 0;

        self.drain_and_cancel_in_flight(ECONNRESET, "USB Port Hard Reset (SE0 assertion)")
    }

    /// Mode 2: Simulated Replug (Surprise Removal & Debounced Hotplug)
    /// Triggers surprise removal PnP event, drops all pipes, waits debounce window,
    /// then hotplugs back onto the virtual root hub as a new device.
    pub fn trigger_simulated_replug(&mut self) -> Vec<CancelledUrbInfo> {
        self.total_replug_resets += 1;
        self.state = DeviceOperationalState::SurpriseRemoved;
        self.active_method = Some(ResetMethod::SimulatedReplug);
        self.reset_start_time = Some(Instant::now());
        self.stalled_endpoints.clear();
        self.address = 0;
        self.active_config = 0;

        self.drain_and_cancel_in_flight(ENODEV, "Simulated Replug Surprise Removal")
    }

    /// Periodic poll to advance state machines based on elapsed physical time
    pub fn poll_state(&mut self, now: Instant) {
        if let Some(start) = self.reset_start_time {
            let elapsed = now.duration_since(start);

            match self.state {
                DeviceOperationalState::HardResetting => {
                    if elapsed >= self.hard_reset_duration {
                        // Port reset signaling completed; device is powered and at Address 0
                        self.state = DeviceOperationalState::DefaultAddress0;
                        self.reset_start_time = None;
                        self.active_method = None;
                    }
                }
                DeviceOperationalState::SurpriseRemoved => {
                    // Host OS has registered surprise removal; enter debounce
                    self.state = DeviceOperationalState::ReplugDebouncing;
                }
                DeviceOperationalState::ReplugDebouncing => {
                    if elapsed >= self.replug_debounce_duration {
                        // Hotplug re-attachment event on root hub port
                        self.state = DeviceOperationalState::DefaultAddress0;
                        self.reset_start_time = None;
                        self.active_method = None;
                    }
                }
                _ => {}
            }
        }
    }

    // -------------------------------------------------------------------------
    // Operational Queries & Invariant Checks
    // -------------------------------------------------------------------------

    pub fn is_connected(&self) -> bool {
        !matches!(
            self.state,
            DeviceOperationalState::Disconnected
                | DeviceOperationalState::SurpriseRemoved
                | DeviceOperationalState::ReplugDebouncing
        )
    }

    pub fn is_ready_for_data(&self) -> bool {
        matches!(
            self.state,
            DeviceOperationalState::Configured(_) | DeviceOperationalState::ActiveStreaming
        )
    }

    pub fn can_accept_urb(&self, endpoint: u8, xfer: TransferType) -> Result<(), i32> {
        match self.state {
            DeviceOperationalState::Disconnected | DeviceOperationalState::SurpriseRemoved => {
                Err(ENODEV)
            }
            DeviceOperationalState::HardResetting | DeviceOperationalState::ReplugDebouncing => {
                Err(EBUSY)
            }
            DeviceOperationalState::DefaultAddress0 | DeviceOperationalState::Addressed(_) => {
                // Pre-configuration: Only EP0 Control transfers allowed
                if endpoint == 0x00 && xfer == TransferType::Control {
                    Ok(())
                } else {
                    Err(EBUSY)
                }
            }
            DeviceOperationalState::Configured(_) | DeviceOperationalState::ActiveStreaming => {
                if self.stalled_endpoints.contains(&endpoint) {
                    Err(EPIPE)
                } else {
                    Ok(())
                }
            }
            DeviceOperationalState::SoftResetting => Err(EBUSY),
        }
    }

    /// Injects an in-flight URB to be tracked
    pub fn track_urb(&mut self, hdr: TurboUrbHeader) {
        self.in_flight_urbs.push(hdr);
    }

    /// Removes completed URB from tracking
    pub fn untrack_urb(&mut self, sequence_id: u64) {
        self.in_flight_urbs.retain(|u| u.sequence_id != sequence_id);
    }

    /// Aborts all tracked in-flight URBs with the specified error code
    fn drain_and_cancel_in_flight(&mut self, status_code: i32, reason: &str) -> Vec<CancelledUrbInfo> {
        let mut cancelled = Vec::with_capacity(self.in_flight_urbs.len());
        for mut u in self.in_flight_urbs.drain(..) {
            u.command = CMD_UNLINK_URB;
            u.status = status_code;
            cancelled.push(CancelledUrbInfo {
                sequence_id: u.sequence_id,
                endpoint_address: u.endpoint_address,
                status: status_code,
                reason: reason.to_string(),
            });
        }
        cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_reset_clears_single_stall() {
        let mut dev = DeviceResetController::new(10);
        assert_eq!(dev.attach_initial(), Ok(()));
        assert_eq!(dev.set_address(3), Ok(()));
        assert_eq!(dev.set_configuration(1), Ok(()));
        assert_eq!(dev.start_streaming(), Ok(()));

        // Inject stall on endpoint 0x82
        dev.stalled_endpoints.insert(0x82);
        assert_eq!(dev.can_accept_urb(0x82, TransferType::Bulk), Err(EPIPE));
        // EP0 and other endpoints unaffected
        assert_eq!(dev.can_accept_urb(0x00, TransferType::Control), Ok(()));
        assert_eq!(dev.can_accept_urb(0x81, TransferType::Bulk), Ok(()));

        // Trigger Soft Reset on 0x82
        assert_eq!(dev.trigger_soft_reset(0x82), Ok(()));
        assert!(!dev.stalled_endpoints.contains(&0x82));
        assert_eq!(dev.can_accept_urb(0x82, TransferType::Bulk), Ok(()));
        assert_eq!(dev.total_soft_resets, 1);
    }

    #[test]
    fn test_hard_reset_drains_urbs_and_reverts_address() {
        let mut dev = DeviceResetController::new(20);
        dev.attach_initial().unwrap();
        dev.set_address(5).unwrap();
        dev.set_configuration(1).unwrap();
        dev.start_streaming().unwrap();

        // Track 3 in-flight bulk URBs
        for seq in 1..=3 {
            dev.track_urb(TurboUrbHeader::new(
                0x0001,
                seq,
                20,
                0x81,
                TransferType::Bulk,
                512,
            ));
        }
        assert_eq!(dev.in_flight_urbs.len(), 3);

        // Trigger Hard Reset
        let cancelled = dev.trigger_hard_reset();
        assert_eq!(cancelled.len(), 3);
        for item in cancelled {
            assert_eq!(item.status, ECONNRESET);
        }
        assert_eq!(dev.in_flight_urbs.len(), 0);
        assert_eq!(dev.state, DeviceOperationalState::HardResetting);
        assert_eq!(dev.address, 0);

        // Fast-forward past hard reset duration
        let later = Instant::now() + Duration::from_millis(50);
        dev.poll_state(later);
        assert_eq!(dev.state, DeviceOperationalState::DefaultAddress0);

        // Re-enumeration
        assert_eq!(dev.set_address(8), Ok(()));
        assert_eq!(dev.set_configuration(1), Ok(()));
        assert_eq!(dev.start_streaming(), Ok(()));
        assert_eq!(dev.can_accept_urb(0x81, TransferType::Bulk), Ok(()));
    }

    #[test]
    fn test_simulated_replug_full_hotplug_lifecycle() {
        let mut dev = DeviceResetController::new(30);
        dev.attach_initial().unwrap();
        dev.set_address(4).unwrap();
        dev.set_configuration(1).unwrap();

        let cancelled = dev.trigger_simulated_replug();
        assert_eq!(cancelled.len(), 0);
        assert_eq!(dev.state, DeviceOperationalState::SurpriseRemoved);

        // While surprise removed, cannot accept URBs
        assert_eq!(dev.can_accept_urb(0x00, TransferType::Control), Err(ENODEV));

        // Advance debounce
        let now = Instant::now();
        dev.poll_state(now);
        assert_eq!(dev.state, DeviceOperationalState::ReplugDebouncing);

        // Complete debounce delay
        let after_debounce = now + Duration::from_millis(150);
        dev.poll_state(after_debounce);
        assert_eq!(dev.state, DeviceOperationalState::DefaultAddress0);

        // Re-attach succeeds
        assert_eq!(dev.set_address(9), Ok(()));
        assert_eq!(dev.set_configuration(1), Ok(()));
        assert_eq!(dev.start_streaming(), Ok(()));
        assert!(dev.is_ready_for_data());
    }
}
