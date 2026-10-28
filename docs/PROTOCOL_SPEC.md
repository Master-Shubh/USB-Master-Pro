# USB Master Pro — Turbo-URB Wire Protocol Specification

**Document Version**: 1.0.0  
**Wire Magic**: `0x54555242` ("TURB" ASCII)  
**Protocol Version**: `0x0100` (v1.0)  
**Byte Ordering**: Strictly Little-Endian (`<LE>`)  
**Header Size**: Exactly 36 Bytes (Packed, Zero Padding)  
**Classification**: Network Systems & Low-Level Wire Framing  

---

## 1. Protocol Architecture & Design Principles

The Turbo-URB protocol is an ultra-low-latency, zero-copy binary wire framing format optimized for virtualizing USB Request Blocks (URBs) across WebRTC DataChannels and QUIC/TLS datagram streams. 

Unlike the legacy USB/IP RFC protocol (which introduces 48-byte headers with nested big-endian pointers, lack of compression signaling, and zero anticipatory read capabilities), Turbo-URB is engineered for:
1. **Sub-Microsecond Parsing**: 36-byte fixed-size header mapped directly into register-aligned memory structures.
2. **Deterministic Endianness**: Strictly Little-Endian matching the host x86_64 architecture of Windows kernel URBs and eliminating byte-swap cycles.
3. **Pipelining & Multiplexing**: Concurrent endpoint multiplexing via monotonic 64-bit `sequence_id` tags.
4. **Adaptive Inline Compression**: Dynamic flagging for real-time Zstandard dictionary compression.

---

## 2. 36-Byte Packed Binary Header Layout

Every network packet transmitted across the data plane begins with the fixed 36-byte `TurboUrbHeader`. If a data payload accompanies the command (e.g. Bulk OUT data, Control setup packets, or Bulk IN completion rasters), it follows immediately after byte offset 35.

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       Magic (0x54555242)                      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|        Protocol Version       |          Command Code         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                      Sequence ID (64-Bit)                     +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                           Device ID                           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| Endpoint Addr | Transfer Type |             Flags             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Status Code                           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Buffer Length                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Actual Length                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| [Payload Follows Contiguously for Actual Length / Buffer Len] |
+---------------------------------------------------------------+
```

### 2.1 Field Definitions & Memory Offsets

| Byte Offset | Field Name | Type | Representation | Description |
| :---: | :--- | :---: | :---: | :--- |
| **0x00 .. 0x03** | `magic` | `u32` | Little-Endian | Constant ASCII signature `0x54555242` ("TURB"). Packets with invalid magic are dropped immediately. |
| **0x04 .. 0x05** | `version` | `u16` | Little-Endian | Protocol version number. Current baseline: `0x0100` (1.0). |
| **0x06 .. 0x07** | `command` | `u16` | Little-Endian | Operational command opcode (`0x0001` through `0x0008`). |
| **0x08 .. 0x0F** | `sequence_id` | `u64` | Little-Endian | Monotonically incrementing transaction sequence ID. Pairs submit and complete packets. |
| **0x10 .. 0x13** | `device_id` | `u32` | Little-Endian | Target virtual device ID on the bus. |
| **0x14** | `endpoint_address` | `u8` | Byte | USB endpoint address. Bit 7 indicates direction: `0` = OUT (Host-to-Device), `1` = IN (Device-to-Host). |
| **0x15** | `transfer_type` | `u8` | Byte | USB transfer classification: `0` = Control, `1` = Isochronous, `2` = Bulk, `3` = Interrupt. |
| **0x16 .. 0x17** | `flags` | `u16` | Little-Endian | Bitfield flags signaling compression, error states, and anticipatory buffering. |
| **0x18 .. 0x1B** | `status` | `i32` | Little-Endian | USBD or NTSTATUS error code. `0` = `STATUS_SUCCESS`. Non-zero indicates USB stall or transfer failure. |
| **0x1C .. 0x1F** | `buffer_length` | `u32` | Little-Endian | Expected or allocated transfer buffer size in bytes. |
| **0x20 .. 0x23** | `actual_length` | `u32` | Little-Endian | Actual number of payload bytes transmitted in this packet. |

---

## 3. Command Codes

| Opcode | Symbolic Identifier | Direction | Semantics & Execution Flow |
| :---: | :--- | :---: | :--- |
| `0x0001` | `CMD_SUBMIT_URB` | Guest $\to$ Host | Submits a URB to the physical hardware. For OUT transfers, payload bytes accompany the header. For IN transfers, `buffer_length` specifies requested read size. |
| `0x0002` | `CMD_COMPLETE_URB` | Host $\to$ Guest | Returns completion status and received data for a previously submitted URB. Matches `sequence_id`. |
| `0x0003` | `CMD_UNLINK_URB` | Guest $\to$ Host | Requests cancellation of an in-flight URB (e.g. on client timeout, IRP cancellation, or device reset). |
| `0x0004` | `CMD_RESET_DEVICE` | Guest $\to$ Host | Triggers an SE0 USB bus reset on the target physical port to recover a stalled device. |
| `0x0005` | `CMD_ATTACH_DEVICE` | Peer $\leftrightarrow$ Peer | Initiates virtual device mounting; exchanges device descriptors and speed configurations. |
| `0x0006` | `CMD_DETACH_DEVICE` | Peer $\leftrightarrow$ Peer | Unmounts virtual device; triggers surprise removal (`STATUS_DEVICE_REMOVED`) in guest OS. |
| `0x0007` | `CMD_PING` | Peer $\leftrightarrow$ Peer | Data-plane latency keep-alive probe containing a high-resolution 64-bit timestamp. |
| `0x0008` | `CMD_PONG` | Peer $\leftrightarrow$ Peer | Echo response to `CMD_PING` used to compute real-time WAN roundtrip time (RTT). |

---

## 4. Transfer Types & Bitfield Flags

### 4.1 Transfer Types (`transfer_type`)
```rust
#[repr(u8)]
pub enum TransferType {
    Control = 0,     // Endpoint 0 setup packets & standard device requests
    Isochronous = 1, // Time-sensitive streaming audio/video (UVC webcams)
    Bulk = 2,        // High-volume burst transfers (Biometric sensors, printers)
    Interrupt = 3,   // Low-latency periodic polling (SmartCard CCID status, HID)
}
```

### 4.2 Flags Bitfield (`flags`)

| Flag Name | Value | Description |
| :--- | :---: | :--- |
| `FLAG_COMPRESSED_ZSTD` | `0x0001` | Payload following the header is compressed using Zstandard-1. Receiver must decompress into pre-allocated buffer. |
| `FLAG_ERROR_RESPONSE` | `0x0002` | Transfer halted due to endpoint stall, pipe error, or hardware failure. `status` field contains NTSTATUS code. |
| `FLAG_ISOCH_PACKET` | `0x0004` | Payload contains an array of `TurboIsoPacketDescriptor` structures preceding raw media stream frames. |
| `FLAG_ANTICIPATORY` | `0x0008` | Read buffer was satisfied from the speculative bulk prefetch ring rather than an active round-trip WAN query. |

---

## 5. Non-Idempotent Retry Safety Invariants

In standard network protocols, packet loss is resolved by blind retransmission. **In USB virtualization, retransmitting a non-idempotent URB causes catastrophic hardware corruption**:
- Retrying an unacknowledged POS thermal print command causes duplicate invoices or receipts.
- Retrying a SmartCard / PKI token write command (`SCARD_TRANSMIT`) corrupts cryptographic state or locks the PIN counter.
- Retrying a biometric capture control command triggers a firmware stall.

### 5.1 Strict Retry Rules
1. **Bulk IN Reads (Idempotent)**: Speculative read pooling is permitted only for Bulk IN endpoints flagged as `FLAG_RD_SERVICE_OPTIMIZED`.
2. **Bulk OUT & Control OUT (Strictly Non-Idempotent)**: Bulk OUT data transfers and Control transfers with OUT data payloads **MUST NEVER** be retried upon socket timeout. If a connection drops while an OUT transfer is in-flight, the session MUST transition to `Stalled` or `Detaching`, abort the transaction, and notify the user interface.
3. **Monotonic Sequence Enforcement**: Receivers drop out-of-order `CMD_SUBMIT_URB` sequence IDs to prevent command interleaving.
