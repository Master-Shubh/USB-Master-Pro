# ADR-005: Transport Protocol Selection (TCP/TLS vs. WebRTC & QUIC)

**Status**: Accepted  
**Date**: 2026-10-28  
**Author**: BhartX Architecture Guild  
**Decision Makers**: Antigravity (AI CTO & Commander), bhartx-coder  

---

## 1. Context & Problem Statement

Legacy USB redirectors rely on plain, unencrypted TCP connections listening on static inbound ports (e.g. SimplyCore port 32032 or VirtualHere port 7575). 

This architectural legacy creates severe operational barriers:
1. **Windows Firewall Blocking**: Windows blocks unsolicited inbound TCP connections by default, requiring administrative firewall configuration prompts.
2. **Head-of-Line Blocking**: In plain TCP, a single dropped packet stalls the entire stream. For USB virtualization, this causes isochronous audio/video packets to stall behind retransmissions.
3. **Router Port Forwarding**: Users must configure router NAT port forwarding or pay for static public IP addresses to connect remote kiosks.

---

## 2. Decision Drivers

1. **Zero-Prompt Firewall Evasion**: The software must connect without triggering Windows Firewall inbound security dialogs.
2. **NAT & Hotspot Traversal**: Transparent operation over 4G/5G mobile hotspots and enterprise symmetric NATs.
3. **Head-of-Line Blocking Mitigation**: Independent stream channels for bulk transfers versus isochronous video streams.
4. **End-to-End Encryption**: Mandatory cryptographic privacy for biometric data in transit.

---

## 3. Considered Options

* **Option 1: Raw TCP Socket on Port 32032 (Legacy Pattern)**:
  - *Pros*: Simple to implement.
  - *Cons*: Triggers firewall alerts, blocked by NAT, no built-in encryption, head-of-line blocking stalls webcam streams.
* **Option 2: Pure QUIC (HTTP/3 over UDP)**:
  - *Pros*: Multiplexed streams, zero head-of-line blocking, modern TLS 1.3 encryption.
  - *Cons*: Lacks standardized peer-to-peer NAT hole punching (STUN/ICE) out of the box; requires custom relay infrastructure.
* **Option 3: WebRTC DataChannels (SCTP over DTLS over UDP 443) with TLS 443 Relay Fallback**:
  - *Pros*: Built-in RFC 8445 ICE hole punching, outbound UDP 443 permitted by all routers, DTLS 1.3 / AES-256-GCM encryption, independent reliable and unreliable SCTP streams, AnyDesk-like connectivity.

---

## 4. Decision Outcome

**Chosen Option: Option 3 (WebRTC DataChannels with TLS 443 Relay Fallback)**.

### Architectural Blueprint:
1. **Outbound Traversal**: Both nodes establish outbound connections to STUN/TURN servers on standard port 443. Windows Firewall permits outbound UDP/TCP automatically without user prompts.
2. **Stream Multiplexing**:
   - Bulk and Control transfers run over an SCTP ordered reliable channel.
   - Isochronous transfers (UVC webcams and iris streams) run over an SCTP unordered, time-bounded channel, preventing video frame jitter.
3. **Guaranteed Fallback**: If symmetric carrier-grade NAT prevents direct UDP hole punching, the connection automatically transitions to a TURN relay over TLS Port 443.
