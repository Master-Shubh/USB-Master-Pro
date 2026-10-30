//! Hardware Identity & 9-Digit AnyDesk-Style ID Generator
//! Generates an immutable, human-readable 9-digit ID anchored to platform hardware.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
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

    /// Derives an asymmetric keypair deterministically from a 32-byte seed using RFC 8032 Ed25519
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(seed);
        let verifying_key = signing_key.verifying_key();
        let public_key = verifying_key.to_bytes();
        let public_key_hex = hex::encode(public_key);

        Self {
            public_key,
            private_key: *seed,
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
        let signing_key = SigningKey::from_bytes(&self.private_key);
        let sig: Signature = signing_key.sign(message);
        sig.to_bytes()
    }

    /// Verifies a 64-byte signature against this keypair's public key and the message
    pub fn verify(&self, message: &[u8], signature: &[u8; 64]) -> bool {
        Self::verify_with_public_key(&self.public_key, message, signature)
    }

    /// Verifies a 64-byte signature given only the 32-byte public key using strict Ed25519 verification (RFC 8032)
    pub fn verify_with_public_key(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
        let Ok(verifying_key) = VerifyingKey::from_bytes(public_key) else {
            return false;
        };
        let sig = Signature::from_bytes(signature);
        verifying_key.verify_strict(message, &sig).is_ok()
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
        #[cfg(any(test, feature = "test-utils"))]
        if let Ok(override_hw_hash) = std::env::var("BHARTX_TEST_HARDWARE_HASH") {
            let trimmed = override_hw_hash.trim();
            if !trimmed.is_empty() {
                let hardware_hash = trimmed.to_string();
                let hash_bytes = hex::decode(&hardware_hash).unwrap_or_else(|_| {
                    let mut hasher = Sha256::new();
                    hasher.update(hardware_hash.as_bytes());
                    hasher.finalize().to_vec()
                });

                let slice = if hash_bytes.len() >= 4 { &hash_bytes[0..4] } else { &[0u8; 4] };
                let num = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]);
                let raw_id = 100_000_000 + (num % 900_000_000);

                let s = raw_id.to_string();
                let formatted_id = format!("{} {} {}", &s[0..3], &s[3..6], &s[6..9]);

                let pin_slice = if hash_bytes.len() >= 8 { &hash_bytes[4..8] } else { &[0u8; 4] };
                let pin_num = u32::from_be_bytes([pin_slice[0], pin_slice[1], pin_slice[2], pin_slice[3]]);
                let pin_val = 100_000 + (pin_num % 900_000);
                let pin_s = pin_val.to_string();
                let default_pin = format!("{} {}", &pin_s[0..3], &pin_s[3..6]);

                let keypair = InstallationKeypair::from_hardware_entropy(&hash_bytes);

                return Self {
                    raw_id,
                    formatted_id,
                    default_pin,
                    hardware_hash,
                    installation_public_key: keypair.public_key_hex,
                };
            }
        }

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
        if let Ok(bytes) = hex::decode(&self.hardware_hash) {
            InstallationKeypair::from_hardware_entropy(&bytes)
        } else {
            InstallationKeypair::from_hardware_entropy(self.hardware_hash.as_bytes())
        }
    }

    /// Creates an Ed25519 cryptographic registration proof for broker challenge-response authentication.
    /// Signs "USB_MASTER_REGISTER:<formatted_id>:<nonce>" with the node's Ed25519 private key
    /// and returns a 128-character hex-encoded signature.
    pub fn create_registration_proof(&self, nonce: &str) -> String {
        let message = format!("USB_MASTER_REGISTER:{}:{}", self.formatted_id, nonce);
        let keypair = self.installation_keypair();
        let sig = keypair.sign(message.as_bytes());
        hex::encode(sig)
    }
}

/// Verifies an Ed25519 registration proof against a public key, peer ID, and challenge nonce.
/// Verifies the signature over "USB_MASTER_REGISTER:<id>:<nonce>".
/// Transparently handles both formatted ("482 190 334") and unformatted ("482190334") IDs.
pub fn verify_registration_proof(
    public_key_hex: &str,
    id: &str,
    nonce: &str,
    signature_hex: &str,
) -> bool {
    if signature_hex.len() != 128 || public_key_hex.len() != 64 {
        return false;
    }

    let pubkey_bytes = match hex::decode(public_key_hex) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => return false,
    };

    let sig_bytes = match hex::decode(signature_hex) {
        Ok(b) if b.len() == 64 => {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&b);
            arr
        }
        _ => return false,
    };

    let msg1 = format!("USB_MASTER_REGISTER:{}:{}", id, nonce);
    if InstallationKeypair::verify_with_public_key(&pubkey_bytes, msg1.as_bytes(), &sig_bytes) {
        return true;
    }

    let clean_id: String = id.chars().filter(|c| !c.is_whitespace()).collect();
    if clean_id.len() == 9 {
        let formatted = format!("{} {} {}", &clean_id[0..3], &clean_id[3..6], &clean_id[6..9]);
        if formatted != id {
            let msg2 = format!("USB_MASTER_REGISTER:{}:{}", formatted, nonce);
            if InstallationKeypair::verify_with_public_key(&pubkey_bytes, msg2.as_bytes(), &sig_bytes) {
                return true;
            }
        }
        if clean_id != id {
            let msg3 = format!("USB_MASTER_REGISTER:{}:{}", clean_id, nonce);
            if InstallationKeypair::verify_with_public_key(&pubkey_bytes, msg3.as_bytes(), &sig_bytes) {
                return true;
            }
        }
    }

    false
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

    #[test]
    fn test_registration_proof_flow() {
        let identity = NodeIdentity::load_or_generate();
        let nonce = "4a9f3b8c2d1e0f9a8b7c6d5e4f3a2b1c";

        let proof = identity.create_registration_proof(nonce);
        assert_eq!(proof.len(), 128); // 64 bytes = 128 hex chars

        // Verify with formatted ID
        assert!(verify_registration_proof(
            &identity.installation_public_key,
            &identity.formatted_id,
            nonce,
            &proof
        ));

        // Verify with raw 9-digit ID (unformatted)
        assert!(verify_registration_proof(
            &identity.installation_public_key,
            &identity.raw_id.to_string(),
            nonce,
            &proof
        ));

        // Tampered nonce fails
        assert!(!verify_registration_proof(
            &identity.installation_public_key,
            &identity.formatted_id,
            "tampered_nonce_1234567890abcdef",
            &proof
        ));

        // Wrong ID fails
        assert!(!verify_registration_proof(
            &identity.installation_public_key,
            "123 456 789",
            nonce,
            &proof
        ));

        // Wrong public key fails
        let other_kp = InstallationKeypair::generate();
        assert!(!verify_registration_proof(
            other_kp.public_key_hex(),
            &identity.formatted_id,
            nonce,
            &proof
        ));

        // Invalid signature hex length fails safely
        assert!(!verify_registration_proof(
            &identity.installation_public_key,
            &identity.formatted_id,
            nonce,
            "deadbeef"
        ));
    }
}


