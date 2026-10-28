//! Dual-Eye Synchronized Iris Scanner Hardware Adapter
//! Mantra MATISX / MIS100V2, CMITECH BMT-20 / EMX-30, IriTech IriUniverse / MK2120U.
//! Dual-eye synchronized capture, isochronous jitter buffer & bulk stream, SLA <180ms.

use super::{AdapterError, IDeviceAdapter, UrbWireMessage};
use crate::compression::TurboCompressor;
use crate::hardware_db::{DeviceCategory, FLAG_ISOCH_JITTER_BUFFER, FLAG_RD_SERVICE_OPTIMIZED};
use crate::protocol::{TransferType, FLAG_ANTICIPATORY, FLAG_COMPRESSED_ZSTD, FLAG_ISOCH_PACKET};

pub const MANTRA_VID: u16 = 0x2137;
pub const MIS100V2_PID: u16 = 0x0030;
pub const MATISX_PID: u16 = 0x0031;

pub const CMITECH_VID: u16 = 0x2914;
pub const CMITECH_PID: u16 = 0x0100;

pub const IRITECH_VID: u16 = 0x1A22;
pub const IRITECH_PID: u16 = 0x0010;

#[derive(Debug, Clone, Default)]
pub struct IrisScannersAdapter;

impl IrisScannersAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl IDeviceAdapter for IrisScannersAdapter {
    fn name(&self) -> &str {
        "Dual-Eye Synchronized Iris Scanner Adapter"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::IrisScanner
    }

    fn matches(&self, vid: u16, pid: u16) -> bool {
        (vid == MANTRA_VID && (pid == MIS100V2_PID || pid == MATISX_PID))
            || (vid == CMITECH_VID && pid == CMITECH_PID)
            || (vid == IRITECH_VID && pid == IRITECH_PID)
            || (vid == 0x27D0 && (pid == 0x0042 || pid == 0x0045))
            || (vid == 0x2B89 && (pid == 0x0020 || pid == 0x0030))
            || (vid == 0x2655 && (pid == 0x0110 || pid == 0x0120))
    }

    fn optimize_urb(&self, urb: &mut UrbWireMessage) -> Result<(), AdapterError> {
        if urb.payload.len() > self.max_packet_size() {
            return Err(AdapterError::PacketTooLarge {
                max_size: self.max_packet_size(),
                actual_size: urb.payload.len(),
            });
        }

        // Isochronous jitter buffer framing & anticipatory streaming
        if urb.header.transfer_type == TransferType::Isochronous {
            urb.header.flags |= FLAG_ISOCH_PACKET | FLAG_ANTICIPATORY;
        } else if urb.header.transfer_type == TransferType::Bulk {
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
        FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER
    }

    fn max_packet_size(&self) -> usize {
        64 * 1024
    }

    fn target_sla_ms(&self) -> u32 {
        180
    }
}
