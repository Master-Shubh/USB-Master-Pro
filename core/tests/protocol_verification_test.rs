//! Turbo-URB Wire Protocol Verification Suite
//! Validates serialization, deserialization, corrupt magic rejection, truncation protection,
//! numeric boundaries, and streaming wire payload framing.

use byteorder::{LittleEndian, WriteBytesExt};
use usb_master_core::protocol::*;
use usb_master_core::TransferType;

#[test]
fn test_header_roundtrip_all_fields() {
    let mut hdr = TurboUrbHeader::new(
        CMD_SUBMIT_URB,
        0x1122334455667788,
        0xAABBCCDD,
        0x82,
        TransferType::Bulk,
        65536,
    );
    hdr.flags = FLAG_COMPRESSED_ZSTD | FLAG_ANTICIPATORY;
    hdr.status = -32; // EPIPE
    hdr.actual_length = 32768;

    let bytes = hdr.serialize();
    assert_eq!(bytes.len(), TurboUrbHeader::SIZE);

    let parsed = TurboUrbHeader::deserialize(&bytes).expect("Deserialization failed");
    assert_eq!(parsed.magic, TURBO_USB_MAGIC);
    assert_eq!(parsed.version, PROTOCOL_VERSION);
    assert_eq!(parsed.command, CMD_SUBMIT_URB);
    assert_eq!(parsed.sequence_id, 0x1122334455667788);
    assert_eq!(parsed.device_id, 0xAABBCCDD);
    assert_eq!(parsed.endpoint_address, 0x82);
    assert_eq!(parsed.transfer_type, TransferType::Bulk);
    assert_eq!(parsed.flags, FLAG_COMPRESSED_ZSTD | FLAG_ANTICIPATORY);
    assert_eq!(parsed.status, -32);
    assert_eq!(parsed.buffer_length, 65536);
    assert_eq!(parsed.actual_length, 32768);
    assert_eq!(hdr, parsed);
}

#[test]
fn test_corrupt_magic_rejection() {
    let hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, 1, 1, 0x81, TransferType::Bulk, 128);
    let mut bytes = hdr.serialize();

    let invalid_magics: [u32; 5] = [
        0x00000000,
        0xDEADBEEF,
        0xFFFFFFFF,
        0x54555241, // "TURA" instead of "TURB"
        0x42525554, // Byte-swapped "BRUT"
    ];

    for &bad_magic in &invalid_magics {
        (&mut bytes[0..4]).write_u32::<LittleEndian>(bad_magic).unwrap();
        let res = TurboUrbHeader::deserialize(&bytes);
        assert!(res.is_err(), "Expected error for magic 0x{:08X}", bad_magic);
        let err_msg = res.err().unwrap();
        assert!(
            err_msg.contains("Invalid Turbo-URB magic"),
            "Error message should mention invalid magic, got: {}",
            err_msg
        );
    }
}

#[test]
fn test_buffer_truncation_detection() {
    let hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, 42, 100, 0x01, TransferType::Control, 64);
    let full_bytes = hdr.serialize();
    assert_eq!(full_bytes.len(), 36);

    // Test every truncation boundary from 0 up to 35 bytes
    for len in 0..TurboUrbHeader::SIZE {
        let truncated = &full_bytes[..len];
        let res = TurboUrbHeader::deserialize(truncated);
        assert!(
            res.is_err(),
            "Expected error for truncated buffer of length {}",
            len
        );
        let err_msg = res.err().unwrap();
        assert!(
            err_msg.contains("Buffer too short"),
            "Error message must indicate short buffer, got: {}",
            err_msg
        );
    }
}

#[test]
fn test_boundary_conditions() {
    let test_cases = vec![
        // Extreme sequence IDs within valid MAX_PAYLOAD_LENGTH bounds
        (0u64, 0u32, 0x00u8, TransferType::Control, 0u32, 0i32),
        (1u64, 1u32, 0x01u8, TransferType::Isochronous, 1u32, 1i32),
        (u64::MAX, u32::MAX, 0xFFu8, TransferType::Bulk, MAX_PAYLOAD_LENGTH, -1i32),
        (u64::MAX - 1, u32::MAX - 1, 0x8Fu8, TransferType::Interrupt, 1024 * 1024, i32::MIN),
        (123456789012345u64, 999999u32, 0x81u8, TransferType::Bulk, 65536, i32::MAX),
    ];

    for (seq, dev, ep, tt, buf_len, status) in test_cases {
        let mut hdr = TurboUrbHeader::new(CMD_COMPLETE_URB, seq, dev, ep, tt, buf_len);
        hdr.status = status;
        hdr.actual_length = buf_len.min(16384);

        let bytes = hdr.serialize();
        assert_eq!(bytes.len(), TurboUrbHeader::SIZE);

        let parsed = TurboUrbHeader::deserialize(&bytes).unwrap();
        assert_eq!(parsed.sequence_id, seq);
        assert_eq!(parsed.device_id, dev);
        assert_eq!(parsed.endpoint_address, ep);
        assert_eq!(parsed.transfer_type, tt);
        assert_eq!(parsed.buffer_length, buf_len);
        assert_eq!(parsed.status, status);
        assert_eq!(parsed.actual_length, buf_len.min(16384));
    }

    // Verify boundary violation for u32::MAX
    let oversize_hdr = TurboUrbHeader::new(CMD_COMPLETE_URB, 1, 1, 0x81, TransferType::Bulk, u32::MAX);
    let oversize_bytes = oversize_hdr.serialize();
    assert_eq!(
        TurboUrbHeader::deserialize(&oversize_bytes).unwrap_err(),
        "Frame length exceeds maximum allowed ceiling (16 MB)"
    );
}

#[test]
fn test_all_transfer_types_and_fallback() {
    use std::convert::TryFrom;

    // Valid transfer type bytes must parse correctly
    assert_eq!(TransferType::try_from(0u8).unwrap(), TransferType::Control);
    assert_eq!(TransferType::try_from(1u8).unwrap(), TransferType::Isochronous);
    assert_eq!(TransferType::try_from(2u8).unwrap(), TransferType::Bulk);
    assert_eq!(TransferType::try_from(3u8).unwrap(), TransferType::Interrupt);

    // Invalid/unknown transfer type bytes must now return Err (no silent fallback)
    assert!(TransferType::try_from(4u8).is_err());
    assert!(TransferType::try_from(42u8).is_err());
    assert!(TransferType::try_from(255u8).is_err());

    // Error message must contain the invalid byte value
    let err = TransferType::try_from(0xABu8).unwrap_err();
    assert!(err.contains("0xAB"), "Error should include hex value: {}", err);
}

#[test]
fn test_wire_payload_framing() {
    let payload = b"Aadhaar RD Service Biometric Packet [Frame 0x0042] - 500 DPI Raw Minutiae";
    let hdr = TurboUrbHeader::new(
        CMD_SUBMIT_URB,
        999,
        1,
        0x81,
        TransferType::Bulk,
        payload.len() as u32,
    );

    // Frame on wire: [36-byte Header] + [Payload]
    let mut wire_packet = hdr.serialize();
    wire_packet.extend_from_slice(payload);

    assert_eq!(wire_packet.len(), TurboUrbHeader::SIZE + payload.len());

    // Receiver parses header
    let parsed_hdr = TurboUrbHeader::deserialize(&wire_packet).expect("Header parse failed");
    assert_eq!(parsed_hdr.sequence_id, 999);
    assert_eq!(parsed_hdr.buffer_length as usize, payload.len());

    // Receiver slices payload based on header buffer_length
    let parsed_payload = &wire_packet[TurboUrbHeader::SIZE..TurboUrbHeader::SIZE + parsed_hdr.buffer_length as usize];
    assert_eq!(parsed_payload, payload);
}

#[test]
fn test_pipelined_stream_framing_zero_desync() {
    // Pipeline 50 distinct URBs into a single continuous stream
    let mut stream = Vec::new();
    let num_packets = 50;

    for i in 0..num_packets {
        let payload_len = (i * 37) % 512; // Variable sizes including 0
        let payload = vec![(i & 0xFF) as u8; payload_len];

        let mut hdr = TurboUrbHeader::new(
            if i % 2 == 0 { CMD_SUBMIT_URB } else { CMD_COMPLETE_URB },
            i as u64,
            42,
            if i % 2 == 0 { 0x02 } else { 0x82 },
            TransferType::Bulk,
            payload_len as u32,
        );
        hdr.actual_length = payload_len as u32;

        stream.extend_from_slice(&hdr.serialize());
        stream.extend_from_slice(&payload);
    }

    // Unpack stream sequentially and verify every frame
    let mut offset = 0;
    let mut parsed_count = 0;

    while offset < stream.len() {
        assert!(
            stream.len() - offset >= TurboUrbHeader::SIZE,
            "Stream has leftover unaligned bytes"
        );
        let hdr = TurboUrbHeader::deserialize(&stream[offset..]).expect("Stream desynchronization");
        offset += TurboUrbHeader::SIZE;

        let payload_len = hdr.buffer_length as usize;
        assert!(
            offset + payload_len <= stream.len(),
            "Payload extends past stream boundary"
        );
        let payload = &stream[offset..offset + payload_len];
        offset += payload_len;

        assert_eq!(hdr.sequence_id, parsed_count as u64);
        let expected_byte = (parsed_count & 0xFF) as u8;
        for &b in payload {
            assert_eq!(b, expected_byte, "Payload corruption detected in packet {}", parsed_count);
        }

        parsed_count += 1;
    }

    assert_eq!(parsed_count, num_packets);
}

#[test]
fn test_fragmented_stream_reassembly() {
    // Simulate TCP / QUIC network stream chunk fragmentation (e.g. 19 bytes per packet)
    let payload = vec![0xAB; 256];
    let hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, 777, 10, 0x81, TransferType::Bulk, payload.len() as u32);
    let mut wire_data = hdr.serialize();
    wire_data.extend_from_slice(&payload);

    let chunk_size = 19;
    let mut rx_accumulator = Vec::new();
    let mut assembled_header = None;
    let mut assembled_payload = None;

    for chunk in wire_data.chunks(chunk_size) {
        rx_accumulator.extend_from_slice(chunk);

        if assembled_header.is_none() && rx_accumulator.len() >= TurboUrbHeader::SIZE {
            assembled_header = Some(TurboUrbHeader::deserialize(&rx_accumulator).unwrap());
        }

        if let Some(ref h) = assembled_header {
            let total_expected = TurboUrbHeader::SIZE + h.buffer_length as usize;
            if rx_accumulator.len() >= total_expected {
                assembled_payload = Some(rx_accumulator[TurboUrbHeader::SIZE..total_expected].to_vec());
                break;
            }
        }
    }

    let h = assembled_header.expect("Failed to assemble header across fragments");
    let p = assembled_payload.expect("Failed to assemble payload across fragments");
    assert_eq!(h.sequence_id, 777);
    assert_eq!(p.len(), 256);
    assert_eq!(p, payload);
}

#[test]
fn test_isochronous_descriptor_attributes() {
    let iso = TurboIsoPacketDescriptor {
        offset: 1024,
        length: 512,
        status: 0,
    };
    assert_eq!(iso.offset, 1024);
    assert_eq!(iso.length, 512);
    assert_eq!(iso.status, 0);

    let serialized = serde_json::to_string(&iso).expect("Iso json serialization failed");
    let deserialized: TurboIsoPacketDescriptor = serde_json::from_str(&serialized).unwrap();
    assert_eq!(iso, deserialized);
}
