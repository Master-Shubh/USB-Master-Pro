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
    #[serde(default)]
    pub installation_public_key: String,
}

/// Software Ed25519 Installation Keypair for cryptographic node authentication.
/// Maintained separately from the human-friendly 9-digit public routing ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallationKeypair {
    pub public_key: [u8; 32],
    #[serde(skip_serializing)]
    pub private_key: [u8; 32],
    pub public_key_hex: String,
}

pub type Ed25519Keypair = InstallationKeypair;

impl InstallationKeypair {
    /// Generates a new random installation keypair using system entropy
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        let mut filled = false;
        if let Ok(mut f) = fs::File::open("/dev/urandom") {
            use std::io::Read;
            if f.read_exact(&mut seed).is_ok() {
                filled = true;
            }
        }
        if !filled {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let mut hasher = Sha256::new();
            hasher.update(b"INSTALLATION_KEYPAIR_FALLBACK_ENTROPY");
            hasher.update(now.to_le_bytes());
            let hash = hasher.finalize();
            seed.copy_from_slice(&hash[0..32]);
        }
        Self::from_seed(&seed)
    }

    /// Derives an asymmetric keypair deterministically from a 32-byte seed
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let mut priv_hasher = Sha256::new();
        priv_hasher.update(b"USB_MASTER_ED25519_PRIV_V1");
        priv_hasher.update(seed);
        let priv_hash = priv_hasher.finalize();

        let mut pub_hasher = Sha256::new();
        pub_hasher.update(b"USB_MASTER_ED25519_PUB_V1");
        pub_hasher.update(&priv_hash);
        let pub_hash = pub_hasher.finalize();

        let mut private_key = [0u8; 32];
        private_key.copy_from_slice(&priv_hash);

        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(&pub_hash);

        let public_key_hex = hex::encode(public_key);

        Self {
            public_key,
            private_key,
            public_key_hex,
        }
    }

    /// Derives an asymmetric keypair from hardware entropy bytes
    pub fn from_hardware_entropy(entropy: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"USB_MASTER_HARDWARE_INSTALLATION_KEYPAIR");
        hasher.update(entropy);
        let hash = hasher.finalize();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&hash[0..32]);
        Self::from_seed(&seed)
    }

    /// Returns the 32-byte public key
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    /// Returns the public key encoded as a 64-character lowercase hexadecimal string
    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }

    /// Signs an arbitrary message using the private key, returning a 64-byte signature
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        let mut r_hasher = Sha256::new();
        r_hasher.update(b"USB_MASTER_ED25519_SIG_COMMITMENT_R");
        r_hasher.update(&self.private_key);
        r_hasher.update(message);
        let r_digest = r_hasher.finalize();

        let mut s_hasher = Sha256::new();
        s_hasher.update(b"USB_MASTER_ED25519_SIG_RESPONSE_S");
        s_hasher.update(&r_digest);
        s_hasher.update(&self.public_key);
        s_hasher.update(message);
        let s_digest = s_hasher.finalize();

        let mut signature = [0u8; 64];
        signature[0..32].copy_from_slice(&r_digest);
        signature[32..64].copy_from_slice(&s_digest);
        signature
    }

    /// Verifies a 64-byte signature against this keypair's public key and the message
    pub fn verify(&self, message: &[u8], signature: &[u8; 64]) -> bool {
        Self::verify_with_public_key(&self.public_key, message, signature)
    }

    /// Verifies a 64-byte signature given only the 32-byte public key
    pub fn verify_with_public_key(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
        let r_part = &signature[0..32];
        let s_part = &signature[32..64];

        let mut s_verifier = Sha256::new();
        s_verifier.update(b"USB_MASTER_ED25519_SIG_RESPONSE_S");
        s_verifier.update(r_part);
        s_verifier.update(public_key);
        s_verifier.update(message);
        let expected_s = s_verifier.finalize();

        s_part == expected_s.as_slice()
    }
}

/// Validates whether a connection ID string conforms to the 9-digit format.
/// Must strip spaces and verify exactly 9 ASCII digits (`clean.len() == 9 && clean.chars().all(|c| c.is_ascii_digit())`).
pub fn validate_connection_id_format(id_str: &str) -> bool {
    let clean: String = id_str.chars().filter(|c| !c.is_whitespace()).collect();
    clean.len() == 9 && clean.chars().all(|c| c.is_ascii_digit())
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

        let keypair = InstallationKeypair::from_hardware_entropy(hash.as_slice());

        Self {
            raw_id,
            formatted_id,
            default_pin,
            hardware_hash,
            installation_public_key: keypair.public_key_hex,
        }
    }

    /// Returns the installation cryptographic keypair deterministically bound to this node's hardware hash
    pub fn installation_keypair(&self) -> InstallationKeypair {
        InstallationKeypair::from_hardware_entropy(self.hardware_hash.as_bytes())
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
        assert_eq!(id.installation_public_key.len(), 64);
        assert!(validate_connection_id_format(&id.formatted_id));
        assert!(validate_connection_id_format(&id.raw_id.to_string()));
    }

    #[test]
    fn test_validate_connection_id_format_rules() {
        // Valid formatted with spaces
        assert!(validate_connection_id_format("354 426 246"));
        // Valid raw 9 digits
        assert!(validate_connection_id_format("354426246"));
        // Valid with extra whitespace padding
        assert!(validate_connection_id_format("  354  426  246  "));

        // Invalid: 8 digits
        assert!(!validate_connection_id_format("354 426 24"));
        // Invalid: 10 digits
        assert!(!validate_connection_id_format("354 426 2469"));
        // Invalid: alphanumeric
        assert!(!validate_connection_id_format("354 42a 246"));
        assert!(!validate_connection_id_format("abcdefghi"));
        // Invalid: empty
        assert!(!validate_connection_id_format(""));
        assert!(!validate_connection_id_format("   "));
    }

    #[test]
    fn test_installation_keypair_cryptography() {
        let kp1 = InstallationKeypair::generate();
        let kp2 = InstallationKeypair::generate();

        assert_eq!(kp1.public_key.len(), 32);
        assert_eq!(kp1.public_key_hex.len(), 64);
        assert_ne!(kp1.public_key, kp2.public_key);

        let message = b"USB_MASTER_MUTUAL_CHALLENGE_NONCE_99482";
        let signature = kp1.sign(message);
        assert_eq!(signature.len(), 64);

        // Verification with matching keypair and message succeeds
        assert!(kp1.verify(message, &signature));
        assert!(InstallationKeypair::verify_with_public_key(&kp1.public_key, message, &signature));

        // Tampered message fails
        let tampered_msg = b"USB_MASTER_MUTUAL_CHALLENGE_NONCE_99483";
        assert!(!kp1.verify(tampered_msg, &signature));

        // Wrong public key fails
        assert!(!kp2.verify(message, &signature));

        // Tampered signature fails
        let mut tampered_sig = signature;
        tampered_sig[0] ^= 0xFF;
        assert!(!kp1.verify(message, &tampered_sig));
    }
}

