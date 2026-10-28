# ADR-007: Installation Identity & Cryptographic Pairing Architecture

**Status**: Accepted  
**Date**: 2026-10-28  
**Author**: BhartX Architecture Guild  
**Decision Makers**: Antigravity (AI CTO & Commander), bhartx-coder  

---

## 1. Context & Problem Statement

Legacy USB redirection software requires operators to memorize or discover arbitrary dynamic IP addresses (e.g. `192.168.1.142` or `10.42.0.15`). In corporate networks with dynamic DHCP leases or remote cellular connections, IP addresses change constantly, breaking existing configurations.

USB Master Pro requires an immutable, human-friendly node identification system that remains persistent across software restarts and reboots, paired with secure mutual connection authorization.

---

## 2. Decision Drivers

1. **Human Usability**: A memorable, phone-number-like identifier (e.g. `482 190 334`) that operators can read over a phone call or chat.
2. **Hardware Anchoring**: The identifier must be deterministically tied to the host machine's hardware components.
3. **Mutual Authorization**: Protection against unauthorized connection attempts or peripheral hijacking.
4. **Offline Resilience**: Identity must be generated locally without requiring cloud database round-trips.

---

## 3. Considered Options

* **Option 1: Random UUID v4**:
  - *Pros*: Globally unique.
  - *Cons*: Impossible for human operators to memorize or dictate (`f81d4fae-7dec-11d0-a765-00a0c91e6bf6`), changes on every reinstall if not cached.
* **Option 2: Cloud Account / Email Login**:
  - *Pros*: Familiar enterprise identity.
  - *Cons*: Requires account registration friction, creates internet dependence, incompatible with offline or air-gapped field deployments.
* **Option 3: AnyDesk-Style 9-Digit ID Derived from Hardware SHA-256 Hash**:
  - *Pros*: Human-friendly 9-digit format (`XXX XXX XXX`), persistent across reinstallations, derived deterministically from hardware registers, paired with rolling 6-digit PIN.

---

## 4. Decision Outcome

**Chosen Option: Option 3 (AnyDesk-Style 9-Digit ID Derived from Hardware SHA-256 Hash)**.

### Mathematical Formulation:
1. Gather hardware properties:
   $$\text{HardwareEntropy} = \text{SMBIOS UUID} \parallel \text{CPUID} \parallel \text{BIOS Serial} \parallel \text{MAC Address}$$
2. Compute SHA-256 digest:
   $$H = \text{SHA256}(\text{HardwareEntropy})$$
3. Derive 9-digit integer:
   $$\text{RawID} = 100\,000\,000 + (\text{Uint32}(H[0..4]) \pmod{900\,000\,000})$$
4. Format:
   $$\text{FormattedID} = \text{format!}(\text{"\{\} \{\} \{\}"}, S[0..3], S[3..6], S[6..9])$$
5. Derive 6-digit rolling PIN:
   $$\text{DefaultPIN} = 100\,000 + (\text{Uint32}(H[4..8]) \pmod{900\,000})$$

### Mutual Authorization Flow:
1. Operator A enters Remote ID `712 883 910` and clicks **CONNECT**.
2. Operator B's screen displays an interactive modal showing Operator A's hostname, ID, and requested device.
3. Connection is granted ONLY when Operator B clicks **ACCEPT** or Operator A provides the correct pre-shared security PIN.
