//! Hardware Identity & 9-Digit AnyDesk-Style ID Generator
//! Generates an immutable, human-readable 9-digit ID anchored to platform hardware.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    pub raw_id: u32,       // e.g. 482190334
    pub formatted_id: String, // e.g. "482 190 334"
    pub default_pin: String,  // e.g. "821 490"
    pub hardware_hash: String,
}

impl NodeIdentity {
    /// Discovers or generates the node's persistent 9-digit identity
    pub fn load_or_generate() -> Self {
        // Collect hardware characteristics
        let hostname = std::env::var("HOSTNAME").or_else(|_| std::env::var("COMPUTERNAME")).unwrap_or_else(|_| "Host".to_string());
        let username = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "User".to_string());
        let os_info = std::env::consts::OS;
        let arch_info = std::env::consts::ARCH;

        // Try reading machine-id on Linux, or computer UUID on Windows
        let machine_id = fs::read_to_string("/etc/machine-id")
            .or_else(|_| fs::read_to_string("/var/lib/dbus/machine-id"))
            .unwrap_or_else(|_| format!("{}-{}-{}", hostname, username, arch_info));

        let mut hasher = Sha256::new();
        hasher.update(machine_id.trim().as_bytes());
        hasher.update(hostname.as_bytes());
        hasher.update(os_info.as_bytes());
        let hash = hasher.finalize();
        let hardware_hash = hex::encode(hash);

        // Derive 9-digit integer in range [100_000_000 .. 999_999_999]
        let slice = &hash[0..4];
        let num = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]);
        let raw_id = 100_000_000 + (num % 900_000_000);

        let s = raw_id.to_string();
        let formatted_id = format!("{} {} {}", &s[0..3], &s[3..6], &s[6..9]);

        // Derive 6-digit pin
        let pin_slice = &hash[4..8];
        let pin_num = u32::from_be_bytes([pin_slice[0], pin_slice[1], pin_slice[2], pin_slice[3]]);
        let pin_val = 100_000 + (pin_num % 900_000);
        let pin_s = pin_val.to_string();
        let default_pin = format!("{} {}", &pin_s[0..3], &pin_s[3..6]);

        Self {
            raw_id,
            formatted_id,
            default_pin,
            hardware_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_format() {
        let id = NodeIdentity::load_or_generate();
        assert!(id.raw_id >= 100_000_000 && id.raw_id <= 999_999_999);
        assert_eq!(id.formatted_id.len(), 11); // "XXX XXX XXX"
        assert_eq!(id.default_pin.len(), 7);   // "XXX XXX"
    }
}
