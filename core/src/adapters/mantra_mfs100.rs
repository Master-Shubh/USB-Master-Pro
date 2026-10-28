//! Mantra MFS100 / MFS110 L1 Biometric Hardware Adapter
//! Certified for UIDAI Aadhaar and Ayushman Bharat PM-JAY.
//! Speculative bulk read queue, RD service SLA target <120ms.

use super::{AdapterError, IDeviceAdapter, UrbWireMessage};
use crate::compression::TurboCompressor;
use crate::hardware_db::{DeviceCategory, FLAG_RD_SERVICE_OPTIMIZED};
use crate::protocol::{TransferType, FLAG_ANTICIPATORY, FLAG_COMPRESSED_ZSTD};

pub const MANTRA_VID: u16 = 0x2137;
pub const MFS100_PID: u16 = 0x0020;
pub const MFS110_L1_PID: u16 = 0x0041;

#[derive(Debug, Clone, Default)]
pub struct MantraMfs100Adapter;

impl MantraMfs100Adapter {
    pub fn new() -> Self {
        Self
    }
}

impl IDeviceAdapter for MantraMfs100Adapter {
    fn name(&self) -> &str {
        "Mantra MFS100 / MFS110 Biometric Adapter"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::FingerprintScanner
    }

    fn matches(&self, vid: u16, pid: u16) -> bool {
        (vid == MANTRA_VID && (pid == MFS100_PID || pid == MFS110_L1_PID))
            || (vid == 0x27D0 && (pid == 0x0041 || pid == 0x0040 || pid == 0x0110))
    }

    fn optimize_urb(&self, urb: &mut UrbWireMessage) -> Result<(), AdapterError> {
        if urb.payload.len() > self.max_packet_size() {
            return Err(AdapterError::PacketTooLarge {
                max_size: self.max_packet_size(),
                actual_size: urb.payload.len(),
            });
        }

        // Speculative bulk read queue optimization for RD service
        if urb.header.transfer_type == TransferType::Bulk {
            urb.header.flags |= FLAG_ANTICIPATORY;
        }

        // Real-time compression for biometric payloads
        if urb.payload.len() > 64 && (urb.header.flags & FLAG_COMPRESSED_ZSTD) == 0 {
            if let Ok(compressed) = TurboCompressor::compress(&urb.payload) {
                if compressed.len() < urb.payload.len() {
                    urb.payload = compressed;
                    urb.header.flags |= FLAG_COMPRESSED_ZSTD;
                    urb.header.actual_length = urb.payload.len() as u32;
                }
            }
        }

        Ok(())
    }

    fn required_flags(&self) -> u32 {
        FLAG_RD_SERVICE_OPTIMIZED
    }

    fn max_packet_size(&self) -> usize {
        64 * 1024
    }

    fn target_sla_ms(&self) -> u32 {
        120
    }
}
