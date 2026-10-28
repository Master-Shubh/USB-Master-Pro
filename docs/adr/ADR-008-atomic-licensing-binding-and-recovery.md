# ADR-008: Atomic Hardware-Locked Licensing & Recovery Architecture

**Status**: Accepted  
**Date**: 2026-10-28  
**Author**: BhartX Architecture Guild  
**Decision Makers**: Antigravity (AI CTO & Commander), bhartx-coder  

---

## 1. Context & Problem Statement

Commercial USB virtualization platforms must protect intellectual property and enforce subscription tiers (e.g. Free Community Tier: 1 device; Enterprise Pro Tier: unlimited devices and high-speed compression). 

Legacy software relies on registry-based serial numbers easily copied between computers, or online DRM servers that deactivate software if the kiosk temporarily loses internet access.

---

## 2. Decision Drivers

1. **Hardware Node Binding**: Licenses must be locked to immutable machine hardware to prevent illicit duplication.
2. **Offline Resilience**: Once activated, the software must function indefinitely without mandatory internet check-ins.
3. **Reinstallation Recovery**: Reinstalling the operating system or updating USB Master Pro must seamlessly restore the existing license.
4. **Zero Impact on Latency**: License validation must occur asynchronously at startup and never block the real-time URB streaming pipeline.

---

## 3. Considered Options

* **Option 1: Online Floating License Server**:
  - *Pros*: Centralized revocation control.
  - *Cons*: Fails when kiosks are offline; recurring heartbeats introduce network failure points.
* **Option 2: Simple Registry String Key**:
  - *Pros*: Easy to deploy.
  - *Cons*: Vulnerable to trivial piracy and registry scraping.
* **Option 3: Cryptographic Hardware-Bound License Token (Ed25519 Signed)**:
  - *Pros*: Signed with BhartX Ecosystems private master key; embeds the node's `hardware_hash` and expiration date; verified completely offline using embedded public key; persisted via Windows DPAPI.

---

## 4. Decision Outcome

**Chosen Option: Option 3 (Cryptographic Hardware-Bound License Token)**.

### Token Architecture:
```json
{
  "license_id": "LIC-2026-89410",
  "hardware_hash": "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01",
  "customer": "National Rural Health Mission",
  "tier": "EnterprisePro",
  "max_concurrent_devices": 32,
  "features": ["TURBO_URB_ZSTD", "ISOCH_JITTER_BUFFER", "WHITE_LABEL"],
  "valid_until": 1893456000,
  "signature": "3a4f82..."
}
```

### Recovery Invariants:
1. When USB Master Pro launches, it loads `%PROGRAMDATA%\USB Master Pro\license.key`.
2. The engine computes the current machine's `hardware_hash`.
3. If the license file is missing, the engine queries the BhartX license recovery API over HTTPS passing only the deterministic `hardware_hash`.
4. If a valid entitlement exists for that hash, the server returns the signed token, which is cached locally under DPAPI encryption.
