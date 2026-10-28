//! GPS / GNSS Geotagging Receiver Hardware Adapter
//! GlobalSat BU-353-S4, G-Star IV, BioEnable G2 GNSS.
//! Low-latency serial COM framing, continuous 1 Hz NMEA sentence preservation, FLAG_LOW_LATENCY_SERIAL.

use super::{AdapterError, IDeviceAdapter, UrbWireMessage};
use crate::hardware_db::{DeviceCategory, FLAG_LOW_LATENCY_SERIAL};
use crate::protocol::FLAG_ANTICIPATORY;

pub const GLOBALSAT_VID: u16 = 0x067B;
pub const BU353S4_PID: u16 = 0x2303;

pub const GSTAR_VID: u16 = 0x10C4;
pub const GSTAR4_PID: u16 = 0xEA60;

pub const BIOENABLE_VID: u16 = 0x1546;
pub const G2_GNSS_PID: u16 = 0x01A7;

#[derive(Debug, Clone, Default)]
pub struct GpsGnssAdapter;

impl GpsGnssAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl IDeviceAdapter for GpsGnssAdapter {
    fn name(&self) -> &str {
        "GPS / GNSS Geotagging Receiver Adapter"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::GpsDevice
    }

    fn matches(&self, vid: u16, pid: u16) -> bool {
        (vid == GLOBALSAT_VID && pid == BU353S4_PID)
            || (vid == GSTAR_VID && pid == GSTAR4_PID)
            || (vid == BIOENABLE_VID && pid == G2_GNSS_PID)
            || (vid == 0x067B && pid == 0x23A3)
            || (vid == 0x1546 && pid == 0x01A8)
    }

    fn optimize_urb(&self, urb: &mut UrbWireMessage) -> Result<(), AdapterError> {
        if urb.payload.len() > self.max_packet_size() {
            return Err(AdapterError::PacketTooLarge {
                max_size: self.max_packet_size(),
                actual_size: urb.payload.len(),
            });
        }

        // Low-latency serial COM framing: continuous 1 Hz NMEA sentence preservation
        urb.header.flags |= FLAG_ANTICIPATORY;

        Ok(())
    }

    fn required_flags(&self) -> u32 {
        FLAG_LOW_LATENCY_SERIAL
    }

    fn max_packet_size(&self) -> usize {
        512
    }

    fn target_sla_ms(&self) -> u32 {
        50
    }
}
