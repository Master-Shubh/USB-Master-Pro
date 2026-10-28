# ADR-010: Logging, Diagnostics, and Biometric Privacy Retention Architecture

**Status**: Accepted  
**Date**: 2026-10-28  
**Author**: BhartX Architecture Guild  
**Decision Makers**: Antigravity (AI CTO & Commander), bhartx-coder, bhartx-security  

---

## 1. Context & Problem Statement

Production systems require robust logging and diagnostic telemetry to identify network stalls, measure throughput, and monitor driver errors. 

However, USB Master Pro virtualizes sensitive peripherals, including fingerprint scanners (Mantra/Morpho), iris cameras, smart cards, and keyboard strokes. Under Indian UIDAI regulations and international privacy frameworks (GDPR, HIPAA), logging unencrypted biometric payloads, encryption keys, or authentication PINs to disk or cloud logging systems constitutes a severe security violation.

---

## 2. Decision Drivers

1. **UIDAI Zero-Retention Mandate**: Absolute prohibition of storing raw biometric images, minutiae data, or decrypted PID blocks on disk.
2. **Deterministic Memory Zeroization**: Buffers holding transient biometric or smartcard payloads must be cryptographically wiped immediately after network completion.
3. **Diagnostic Utility**: Logs must provide rich troubleshooting metadata (URB sequence IDs, endpoint addresses, byte lengths, compression ratios, latency) without exposing the underlying data bytes.
4. **Ring Buffer Crash Dumps**: In-memory ring buffer for recent diagnostic events, dumped to disk only upon fatal crash with payload masking.

---

## 3. Considered Options

* **Option 1: Standard Debug Logging (Hex Dump Payloads)**:
  - *Pros*: Easy for developers to debug wire anomalies.
  - *Cons*: Violates UIDAI compliance; stores biometric data in plaintext log files; unacceptable security risk.
* **Option 2: Completely Disabled Logging**:
  - *Pros*: Zero privacy leak risk.
  - *Cons*: Impossible to diagnose field network stalls or driver timing issues.
* **Option 3: Strict Metadata-Only Logging with Non-Paged Pool Memory Locking & Cryptographic Zeroization**:
  - *Pros*: Captures essential telemetry (timing, endpoints, transfer types, compression statistics); raw data payloads are never logged; memory buffers are pinned to physical RAM and wiped with compiler barriers.

---

## 4. Decision Outcome

**Chosen Option: Option 3 (Strict Metadata-Only Logging with Memory Locking & Cryptographic Zeroization)**.

### Architectural Controls:
1. **Payload Masking Invariant**:
   - The logger records: `timestamp`, `sequence_id`, `endpoint_address`, `transfer_type`, `buffer_length`, `actual_length`, `status`, `duration_ms`.
   - The logger NEVER records: byte dumps, hex strings, or bitmap headers of bulk payloads.
2. **Non-Paged Memory Locking**:
   - `MmBuildMdlForNonPagedPool` locks all kernel URB buffers into non-paged RAM, preventing the Windows kernel memory manager from swapping biometric buffers to `pagefile.sys`.
3. **Compiler Barrier Zeroization**:
   - C/Kernel: `RtlSecureZeroMemory(Buffer, Length)`
   - Rust: `zeroize::Zeroize::zeroize(&mut buffer)`
   - Ensures memory wiping operations cannot be elided by compiler dead-store optimizations.
4. **Log Retention Window**:
   - Operating logs in `%PROGRAMDATA%\USB Master Pro\logs\` are capped at 5 files of 10 MB each with automatic FIFO rotation.
