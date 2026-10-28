//! Turbo-URB Wire Protocol Definition & Framing
//! High-throughput serialization for USB Request Blocks (URBs) across WebRTC/QUIC streams.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

pub const TURBO_USB_MAGIC: u32 = 0x54555242; // "TURB" in ASCII
pub const PROTOCOL_VERSION: u16 = 0x0100;    // v1.0
pub const MAX_PAYLOAD_LENGTH: u32 = 16 * 1024 * 1024; // 16 MB maximum frame size

/// Operation safety classification for idempotency and replay guarantees
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationSafety {
    IdempotentSafeToRetry,
    NonIdempotentUncertain,
}

/// Classifies USB/network operations according to idempotency and side-effect safety.
/// - Idempotent: CMD_PING, CMD_PONG, Control GET requests, Bulk IN read before execution.
/// - Non-Idempotent / Uncertain: Bulk OUT writes, vendor control OUT, reset device, isochronous streams.
pub fn classify_operation(command: u16, transfer_type: u8, endpoint: u8) -> OperationSafety {
    match command {
        CMD_PING | CMD_PONG => OperationSafety::IdempotentSafeToRetry,
        CMD_RESET_DEVICE | CMD_ATTACH_DEVICE | CMD_DETACH_DEVICE | CMD_UNLINK_URB => {
            OperationSafety::NonIdempotentUncertain
        }
        CMD_SUBMIT_URB | CMD_COMPLETE_URB => {
            let is_in = (endpoint & 0x80) != 0;
            match TransferType::from(transfer_type) {
                TransferType::Isochronous => OperationSafety::NonIdempotentUncertain,
                TransferType::Bulk => {
                    if is_in {
                        OperationSafety::IdempotentSafeToRetry
                    } else {
                        OperationSafety::NonIdempotentUncertain
                    }
                }
                TransferType::Control => {
                    if is_in {
                        OperationSafety::IdempotentSafeToRetry
                    } else {
                        OperationSafety::NonIdempotentUncertain
                    }
                }
                TransferType::Interrupt => {
                    if is_in {
                        OperationSafety::IdempotentSafeToRetry
                    } else {
                        OperationSafety::NonIdempotentUncertain
                    }
                }
            }
        }
        _ => OperationSafety::NonIdempotentUncertain,
    }
}

// Command Codes
pub const CMD_SUBMIT_URB: u16 = 0x0001;
pub const CMD_COMPLETE_URB: u16 = 0x0002;
pub const CMD_UNLINK_URB: u16 = 0x0003;
pub const CMD_RESET_DEVICE: u16 = 0x0004;
pub const CMD_ATTACH_DEVICE: u16 = 0x0005;
pub const CMD_DETACH_DEVICE: u16 = 0x0006;
pub const CMD_PING: u16 = 0x0007;
pub const CMD_PONG: u16 = 0x0008;

// Transfer Types (Standard USB Spec)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TransferType {
    Control = 0,
    Isochronous = 1,
    Bulk = 2,
    Interrupt = 3,
}

impl From<u8> for TransferType {
    fn from(val: u8) -> Self {
        match val {
            0 => TransferType::Control,
            1 => TransferType::Isochronous,
            2 => TransferType::Bulk,
            3 => TransferType::Interrupt,
            _ => TransferType::Bulk,
        }
    }
}

// Flags
pub const FLAG_COMPRESSED_ZSTD: u16 = 0x0001;
pub const FLAG_ERROR_RESPONSE: u16 = 0x0002;
pub const FLAG_ISOCH_PACKET: u16 = 0x0004;
pub const FLAG_ANTICIPATORY: u16 = 0x0008;

/// 36-Byte Fixed Packed Header for all Turbo-URB Network Packets
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurboUrbHeader {
    pub magic: u32,
    pub version: u16,
    pub command: u16,
    pub sequence_id: u64,
    pub device_id: u32,
    pub endpoint_address: u8,
    pub transfer_type: TransferType,
    pub flags: u16,
    pub status: i32,
    pub buffer_length: u32,
    pub actual_length: u32,
}

impl TurboUrbHeader {
    pub const SIZE: usize = 36;

    pub fn new(
        command: u16,
        sequence_id: u64,
        device_id: u32,
        endpoint_address: u8,
        transfer_type: TransferType,
        buffer_length: u32,
    ) -> Self {
        Self {
            magic: TURBO_USB_MAGIC,
            version: PROTOCOL_VERSION,
            command,
            sequence_id,
            device_id,
            endpoint_address,
            transfer_type,
            flags: 0,
            status: 0,
            buffer_length,
            actual_length: 0,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::SIZE);
        buf.write_u32::<LittleEndian>(self.magic).unwrap();
        buf.write_u16::<LittleEndian>(self.version).unwrap();
        buf.write_u16::<LittleEndian>(self.command).unwrap();
        buf.write_u64::<LittleEndian>(self.sequence_id).unwrap();
        buf.write_u32::<LittleEndian>(self.device_id).unwrap();
        buf.write_u8(self.endpoint_address).unwrap();
        buf.write_u8(self.transfer_type as u8).unwrap();
        buf.write_u16::<LittleEndian>(self.flags).unwrap();
        buf.write_i32::<LittleEndian>(self.status).unwrap();
        buf.write_u32::<LittleEndian>(self.buffer_length).unwrap();
        buf.write_u32::<LittleEndian>(self.actual_length).unwrap();
        buf
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < Self::SIZE {
            return Err(format!(
                "Buffer too short: expected {} bytes, got {}",
                Self::SIZE,
                bytes.len()
            ));
        }

        let mut rdr = Cursor::new(bytes);
        let magic = rdr.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        if magic != TURBO_USB_MAGIC {
            return Err(format!("Invalid Turbo-URB magic: 0x{:08X}", magic));
        }

        let version = rdr.read_u16::<LittleEndian>().map_err(|e| e.to_string())?;
        let command = rdr.read_u16::<LittleEndian>().map_err(|e| e.to_string())?;
        let sequence_id = rdr.read_u64::<LittleEndian>().map_err(|e| e.to_string())?;
        let device_id = rdr.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        let endpoint_address = rdr.read_u8().map_err(|e| e.to_string())?;
        let transfer_type_raw = rdr.read_u8().map_err(|e| e.to_string())?;
        let flags = rdr.read_u16::<LittleEndian>().map_err(|e| e.to_string())?;
        let status = rdr.read_i32::<LittleEndian>().map_err(|e| e.to_string())?;
        let buffer_length = rdr.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        let actual_length = rdr.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;

        if buffer_length > MAX_PAYLOAD_LENGTH || actual_length > MAX_PAYLOAD_LENGTH {
            return Err("Frame length exceeds maximum allowed ceiling (16 MB)".to_string());
        }

        Ok(Self {
            magic,
            version,
            command,
            sequence_id,
            device_id,
            endpoint_address,
            transfer_type: TransferType::from(transfer_type_raw),
            flags,
            status,
            buffer_length,
            actual_length,
        })
    }
}

/// Isochronous packet descriptor for continuous video/audio streams
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurboIsoPacketDescriptor {
    pub offset: u32,
    pub length: u32,
    pub status: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_roundtrip() {
        let hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, 10042, 1, 0x81, TransferType::Bulk, 512);
        let bytes = hdr.serialize();
        assert_eq!(bytes.len(), TurboUrbHeader::SIZE);

        let parsed = TurboUrbHeader::deserialize(&bytes).expect("Failed to deserialize");
        assert_eq!(hdr, parsed);
    }

    #[test]
    fn test_frame_length_bounds_validation() {
        // Exactly 16 MB -> should succeed
        let mut hdr_exact = TurboUrbHeader::new(CMD_SUBMIT_URB, 1, 1, 0x81, TransferType::Bulk, MAX_PAYLOAD_LENGTH);
        hdr_exact.actual_length = MAX_PAYLOAD_LENGTH;
        let bytes_exact = hdr_exact.serialize();
        assert!(TurboUrbHeader::deserialize(&bytes_exact).is_ok());

        // buffer_length > 16 MB -> must be rejected
        let hdr_oversize_buf = TurboUrbHeader::new(CMD_SUBMIT_URB, 1, 1, 0x81, TransferType::Bulk, MAX_PAYLOAD_LENGTH + 1);
        let bytes_oversize_buf = hdr_oversize_buf.serialize();
        let err = TurboUrbHeader::deserialize(&bytes_oversize_buf).unwrap_err();
        assert_eq!(err, "Frame length exceeds maximum allowed ceiling (16 MB)");

        // actual_length > 16 MB -> must be rejected
        let mut hdr_oversize_act = TurboUrbHeader::new(CMD_SUBMIT_URB, 1, 1, 0x81, TransferType::Bulk, 512);
        hdr_oversize_act.actual_length = MAX_PAYLOAD_LENGTH + 1024;
        let bytes_oversize_act = hdr_oversize_act.serialize();
        let err2 = TurboUrbHeader::deserialize(&bytes_oversize_act).unwrap_err();
        assert_eq!(err2, "Frame length exceeds maximum allowed ceiling (16 MB)");
    }

    #[test]
    fn test_operation_safety_classification() {
        // Idempotent commands
        assert_eq!(classify_operation(CMD_PING, 0, 0), OperationSafety::IdempotentSafeToRetry);
        assert_eq!(classify_operation(CMD_PONG, 0, 0), OperationSafety::IdempotentSafeToRetry);

        // Control GET (IN endpoint 0x80)
        assert_eq!(classify_operation(CMD_SUBMIT_URB, TransferType::Control as u8, 0x80), OperationSafety::IdempotentSafeToRetry);
        // Control SET/OUT (OUT endpoint 0x00)
        assert_eq!(classify_operation(CMD_SUBMIT_URB, TransferType::Control as u8, 0x00), OperationSafety::NonIdempotentUncertain);

        // Bulk IN (read)
        assert_eq!(classify_operation(CMD_SUBMIT_URB, TransferType::Bulk as u8, 0x81), OperationSafety::IdempotentSafeToRetry);
        // Bulk OUT (write)
        assert_eq!(classify_operation(CMD_SUBMIT_URB, TransferType::Bulk as u8, 0x02), OperationSafety::NonIdempotentUncertain);

        // Isochronous stream (IN or OUT is uncertain)
        assert_eq!(classify_operation(CMD_SUBMIT_URB, TransferType::Isochronous as u8, 0x81), OperationSafety::NonIdempotentUncertain);
        assert_eq!(classify_operation(CMD_SUBMIT_URB, TransferType::Isochronous as u8, 0x01), OperationSafety::NonIdempotentUncertain);

        // Device reset
        assert_eq!(classify_operation(CMD_RESET_DEVICE, 0, 0), OperationSafety::NonIdempotentUncertain);
        // Unlink URB
        assert_eq!(classify_operation(CMD_UNLINK_URB, 0, 0), OperationSafety::NonIdempotentUncertain);
    }
}

