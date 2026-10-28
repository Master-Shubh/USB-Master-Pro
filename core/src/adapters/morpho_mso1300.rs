//! Idemia MorphoSmart MSO 1300 E3 / L1 & Startek FM220U Biometric Adapter
//! UIDAI Aadhaar certified security, bulk transfer acceleration.

use super::{AdapterError, IDeviceAdapter, UrbWireMessage};
use crate::compression::TurboCompressor;
use crate::hardware_db::{DeviceCategory, FLAG_RD_SERVICE_OPTIMIZED};
use crate::protocol::{TransferType, FLAG_ANTICIPATORY, FLAG_COMPRESSED_ZSTD};

pub const MORPHO_VID: u16 = 0x083A;
pub const MSO1300_E3_PID: u16 = 0x0001;
pub const MSO1300_L1_PID: u16 = 0x0005;

pub const STARTEK_VID: u16 = 0x0BCA;
pub const FM220U_PID: u16 = 0x8220;

#[derive(Debug, Clone, Default)]
pub struct MorphoMso1300Adapter;

impl MorphoMso1300Adapter {
    pub fn new() -> Self {
        Self
    }
}

impl IDeviceAdapter for MorphoMso1300Adapter {
    fn name(&self) -> &str {
        "Idemia MorphoSmart MSO 1300 / Startek FM220U Adapter"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::FingerprintScanner
    }

    fn matches(&self, vid: u16, pid: u16) -> bool {
        (vid == MORPHO_VID && (pid == MSO1300_E3_PID || pid == MSO1300_L1_PID))
            || (vid == STARTEK_VID && pid == FM220U_PID)
            || (vid == 0x079B && (pid == 0x0031 || pid == 0x0045 || pid == 0x0047 || pid == 0x0048))
            || (vid == 0x1FBA && (pid == 0x0013 || pid == 0x0014))
    }

    fn optimize_urb(&self, urb: &mut UrbWireMessage) -> Result<(), AdapterError> {
        if urb.payload.len() > self.max_packet_size() {
            return Err(AdapterError::PacketTooLarge {
                max_size: self.max_packet_size(),
                actual_size: urb.payload.len(),
            });
        }

        // Bulk transfer acceleration for UIDAI Aadhaar authentication
        if urb.header.transfer_type == TransferType::Bulk {
            urb.header.flags |= FLAG_ANTICIPATORY;
        }

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
        100
    }
}

/// Dedicated adapter alias for Startek FM220U
#[derive(Debug, Clone, Default)]
pub struct StartekFm220Adapter;

impl StartekFm220Adapter {
    pub fn new() -> Self {
        Self
    }
}

impl IDeviceAdapter for StartekFm220Adapter {
    fn name(&self) -> &str {
        "Startek FM220U Biometric Adapter"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::FingerprintScanner
    }

    fn matches(&self, vid: u16, pid: u16) -> bool {
        (vid == STARTEK_VID && pid == FM220U_PID)
            || (vid == 0x1FBA && (pid == 0x0013 || pid == 0x0014))
    }

    fn optimize_urb(&self, urb: &mut UrbWireMessage) -> Result<(), AdapterError> {
        MorphoMso1300Adapter.optimize_urb(urb)
    }

    fn required_flags(&self) -> u32 {
        FLAG_RD_SERVICE_OPTIMIZED
    }

    fn max_packet_size(&self) -> usize {
        64 * 1024
    }

    fn target_sla_ms(&self) -> u32 {
        100
    }
}
