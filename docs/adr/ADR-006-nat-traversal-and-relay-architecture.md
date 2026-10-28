# ADR-006: NAT Traversal, STUN/TURN Relays, and Rendezvous Architecture

**Status**: Accepted  
**Date**: 2026-10-28  
**Author**: BhartX Architecture Guild  
**Decision Makers**: Antigravity (AI CTO & Commander), bhartx-coder  

---

## 1. Context & Problem Statement

Users frequently deploy biometric fingerprint scanners and thermal printers at remote kiosks, bank branches, and rural CSC centers connected via 4G/5G cellular dongles. These environments operate behind Carrier-Grade NAT (CGNAT) where machines do not possess public IPv4 addresses and cannot accept inbound connections.

A commercial USB virtualization solution must connect these remote nodes without requiring IT departments to configure firewall rules, set up VPNs, or assign static IPs.

---

## 2. Decision Drivers

1. **Zero Configuration Deployment**: The end user only enters a 9-digit AnyUSB ID; all network routing happens autonomously.
2. **High Direct Connection Rate (>80%)**: Maximize direct P2P connections to achieve minimum latency (<15 ms).
3. **100% Guaranteed Reachability**: Fallback relay for the remaining ~20% of nodes behind double-symmetric NATs or restrictive corporate banking proxies.
4. **Relay Cost Efficiency**: Relay bandwidth must be minimized through real-time Zstandard data compression.

---

## 3. Considered Options

* **Option 1: Static VPN Mesh (WireGuard / OpenVPN)**:
  - *Pros*: Secure tunnel.
  - *Cons*: Requires administrative driver installation on both ends, complex IP address management, high deployment friction for non-technical operators.
* **Option 2: Centralized Proxy Forwarding**:
  - *Pros*: Simple client software.
  - *Cons*: 100% of all USB traffic routes through central servers; massive cloud bandwidth costs, high latency, potential single point of failure.
* **Option 3: Standardized ICE (RFC 8445) + STUN (RFC 5389) + Coturn TURN Relays (RFC 5766)**:
  - *Pros*: Automatic peer-to-peer hole punching over UDP 443; fallback to TURN relay over TLS 443 only when direct P2P is mathematically impossible; proven AnyDesk / WebRTC model.

---

## 4. Decision Outcome

**Chosen Option: Option 3 (Standardized ICE + STUN + Coturn TURN Relays on Port 443)**.

```mermaid
flowchart TD
    subgraph Discovery ["Phase 1: Outbound WSS Discovery"]
        A["Node A (482 190 334)"] -->|WSS Outbound 443| BROKER["Signaling Broker"]
        B["Node B (712 883 910)"] -->|WSS Outbound 443| BROKER
    end

    subgraph ICE ["Phase 2: RFC 8445 ICE Candidate Gathering"]
        A -->|STUN Binding UDP 443| STUN["Coturn STUN Server"]
        B -->|STUN Binding UDP 443| STUN
        STUN -.->|Reflexive Candidate (IP:Port)| A
        STUN -.->|Reflexive Candidate (IP:Port)| B
    end

    subgraph P2P ["Phase 3: Direct Peer-to-Peer Connection"]
        A <===>|Direct P2P WebRTC DataChannel (UDP 443)| B
    end

    subgraph Fallback ["Phase 4: Symmetric NAT Fallback"]
        A -.->|TLS 443 Encrypted Relay| TURN["Coturn Anycast Relay"]
        TURN -.->|TLS 443 Encrypted Relay| B
    end
```

### Operational Rules:
1. Both nodes connect outbound on standard port 443.
2. STUN binding requests discover public IP/port mappings.
3. If NAT types allow direct mapping, a direct UDP 443 peer-to-peer DataChannel is opened.
4. If either side is behind a strict symmetric NAT, traffic transparently fails over to Coturn TURN relay over TLS 443.
