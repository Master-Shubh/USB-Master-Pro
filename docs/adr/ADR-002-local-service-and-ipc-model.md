# ADR-002: Local Service Architecture & IPC Communication Model

**Status**: Accepted  
**Date**: 2026-10-28  
**Author**: BhartX Architecture Guild  
**Decision Makers**: Antigravity (AI CTO & Commander), bhartx-coder  

---

## 1. Context & Problem Statement

The USB Master Pro architecture requires reliable, low-overhead communication between the user-facing interface (dashboard) and the background virtualization engine responsible for driver interaction and network streaming. 

Legacy systems often rely on proprietary binary Windows Named Pipes or DCOM interfaces. However, these interfaces introduce cross-session security complexities (e.g. Session 0 isolation on Windows Vista/10/11) and create tight coupling that hampers automated API testing.

---

## 2. Decision Drivers

1. **Security & Session 0 Isolation**: Communication must function cleanly whether the engine runs as an interactive user process or a Windows background service, without tripping Windows UAC or named pipe DACL bugs.
2. **Standardized Interoperability**: Support for standard HTTP REST verbs and full-duplex event streaming.
3. **Low Latency & High Concurrency**: Minimal serialization overhead for local commands (<1 ms).
4. **Ease of Automated Testing**: REST endpoints must be easily testable with standard integration test tools.

---

## 3. Considered Options

* **Option 1: Windows Named Pipes (`\\.\pipe\usbmasterpro`)**:
  - *Pros*: Built-in Windows kernel primitive, process-level access control.
  - *Cons*: Difficult to consume from modern web-based UI technologies without native FFI shims; prone to ACL corruption across different Windows user accounts.
* **Option 2: Windows COM / DCOM**:
  - *Pros*: Deep legacy Windows integration.
  - *Cons*: Extreme complexity, registry bloat, rigid interfaces, poor cross-language support.
* **Option 3: Local Loopback HTTP REST + WebSocket Server (`127.0.0.1:19842`)**:
  - *Pros*: Universal compatibility, zero-copy JSON parsing via Serde, reactive real-time updates via WebSockets (`/api/events`), trivial automated testing (`daemon_ipc_test.rs`).

---

## 4. Decision Outcome

**Chosen Option: Option 3 (Local Loopback HTTP REST + WebSocket on `127.0.0.1:19842`)**.

### Implementation Strategy:
- The daemon binds strictly to `127.0.0.1:19842` (loopback only, never exposed on `0.0.0.0`).
- REST endpoints (`/api/identity`, `/api/devices`, `/api/forward`, `/api/connect`, `/api/respond`, `/api/status`) manage synchronous state.
- A WebSocket endpoint (`/api/events`) streams live asynchronous notifications:
  * Device hot-plug insertion and removal.
  * Incoming peer connection requests (powering the Accept/Reject modal).
  * Real-time link throughput and latency telemetry.

---

## 5. Security Guardrails

1. **Strict Loopback Binding**: Axum binds exclusively to `127.0.0.1`. Remote machines on the local LAN cannot access the management port.
2. **CORS Hardening**: Access is restricted to same-origin requests or local client applications.
