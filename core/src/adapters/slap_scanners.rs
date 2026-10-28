//! 4:4:2 Ten-Print Slap Scanner Hardware Adapter
//! Mantra MFS600, Idemia MorphoTop 100 / MORPHS, Thales/3M Cogent CS500e / CSD550.
//! 4:4:2 ten-print slap capture, high-throughput Zstd-1 frame chunking, SLA <250ms.

use super::{AdapterError, IDeviceAdapter, UrbWireMessage};
use crate::compression::TurboCompressor;
use crate::hardware_db::{DeviceCategory, FLAG_RD_SERVICE_OPTIMIZED};
use crate::protocol::{TransferType, FLAG_ANTICIPATORY, FLAG_COMPRESSED_ZSTD};

pub const MANTRA_VID: u16 = 0x2137;
pub const MFS600_PID: u16 = 0x0060;

pub const MORPHOTOP_VID: u16 = 0x083A;
pub const MORPHOTOP_PID: u16 = 0x0050;

pub const THALES_VID: u16 = 0x1312;
pub const CS500E_PID: u16 = 0x2001;
pub const CSD550_PID: u16 = 0x2002;

#[derive(Debug, Clone, Default)]
pub struct SlapScannersAdapter;

impl SlapScannersAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl IDeviceAdapter for SlapScannersAdapter {
    fn name(&self) -> &str {
        "4:4:2 Ten-Print Slap Scanner Adapter"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::SlapScanner
    }

    fn matches(&self, vid: u16, pid: u16) -> bool {
        (vid == MANTRA_VID && pid == MFS600_PID)
            || (vid == MORPHOTOP_VID && pid == MORPHOTOP_PID)
            || (vid == THALES_VID && (pid == CS500E_PID || pid == CSD550_PID))
            || (vid == 0x27D0 && (pid == 0x0060 || pid == 0x0061))
            || (vid == 0x079B && (pid == 0x0033 || pid == 0x0034))
            || (vid == 0x08B0 && pid == 0x0033)
            || (vid == 0x05BA && (pid == 0x000A || pid == 0x000B || pid == 0x0010))
    }

    fn optimize_urb(&self, urb: &mut UrbWireMessage) -> Result<(), AdapterError> {
        if urb.payload.len() > self.max_packet_size() {
            return Err(AdapterError::PacketTooLarge {
                max_size: self.max_packet_size(),
                actual_size: urb.payload.len(),
            });
        }

        if urb.header.transfer_type == TransferType::Bulk {
            urb.header.flags |= FLAG_ANTICIPATORY;
        }

        // High-throughput Zstd-1 compression for large 4:4:2 slap frames
        if !urb.payload.is_empty() && (urb.header.flags & FLAG_COMPRESSED_ZSTD) == 0 {
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
        1024 * 1024 // 1 MB high-throughput ten-print slap buffer
    }

    fn target_sla_ms(&self) -> u32 {
        250
    }
}
