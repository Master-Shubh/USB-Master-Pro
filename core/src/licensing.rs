//! USB Master Pro — Cryptographic Licensing Engine
//! Validates offline, hardware-anchored Ed25519 signed license tokens.
//! Developed & Owned by BhartX Ecosystems Pvt Ltd.

use crate::identity::InstallationKeypair;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// BhartX Enterprise Root CA Public Key (Ed25519)
/// Used for offline verification of cryptographic license tokens.
pub const BHARTX_ROOT_LICENSE_PUBKEY: [u8; 32] = [
    163, 219, 54, 106, 106, 29, 202, 89, 0, 132, 0, 176, 66, 220, 126, 129,
    127, 184, 229, 40, 73, 63, 123, 180, 10, 122, 206, 177, 229, 14, 26, 157,
];

/// BhartX Root CA Master Seed (for signing tests and server-side issuance)
pub const BHARTX_ROOT_SEED: [u8; 32] = *b"BHARTX_MOCK_TEST_HARNESS_KEY_001";

/// Claims embedded within a cryptographic license payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseClaims {
    pub license_id: String,
    pub hardware_hash: String,
    pub customer_name: String,
    pub tier: String, // "EnterprisePro", "Community"
    pub max_devices: u32,
    pub features: Vec<String>,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl LicenseClaims {
    /// Returns true if this is an Enterprise Pro license
    pub fn is_enterprise(&self) -> bool {
        self.tier.eq_ignore_ascii_case("EnterprisePro")
    }

    /// Returns true if the license is currently active and unexpired
    pub fn is_valid_now(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.expires_at == 0 || self.expires_at >= now
    }

    /// Checks if a specific feature flag is granted by this license
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f.eq_ignore_ascii_case(feature))
    }
}

/// Errors occurring during license token verification.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LicenseError {
    #[error("Invalid license format. Expected '<base64_payload>.<hex_ed25519_signature>'")]
    InvalidFormat,

    #[error("Invalid base64 encoding in license payload")]
    InvalidEncoding,

    #[error("Invalid hexadecimal encoding in license signature")]
    InvalidSignature,

    #[error("Invalid signature length. Expected 128 hex characters")]
    InvalidSignatureLength,

    #[error("Cryptographic signature verification failed")]
    CryptographicSignatureFailed,

    #[error("Malformed JSON payload in license")]
    MalformedPayload,

    #[error("Hardware hash mismatch. License is bound to another machine")]
    HardwareMismatch,

    #[error("License has expired")]
    LicenseExpired,
}

/// Core cryptographic licensing engine for USB Master Pro.
pub struct LicenseEngine;

impl LicenseEngine {
    /// Obtains the BhartX Root CA keypair (derived deterministically from root seed)
    pub fn root_ca_keypair() -> InstallationKeypair {
        InstallationKeypair::from_seed(&BHARTX_ROOT_SEED)
    }

    /// Issues and signs a license token using the specified keypair.
    /// Token format: `<base64_payload>.<hex_ed25519_signature>`
    pub fn issue_token(
        claims: &LicenseClaims,
        signing_keypair: &InstallationKeypair,
    ) -> Result<String, LicenseError> {
        let json_bytes = serde_json::to_vec(claims).map_err(|_| LicenseError::MalformedPayload)?;
        let signature = signing_keypair.sign(&json_bytes);
        let payload_b64 = base64::engine::general_purpose::STANDARD.encode(&json_bytes);
        let sig_hex = hex::encode(signature);
        Ok(format!("{}.{}", payload_b64, sig_hex))
    }

    /// Verifies a license token against the embedded BhartX Root CA public key
    /// and the machine's local hardware hash.
    pub fn verify_token(
        raw_token: &str,
        local_hardware_hash: &str,
    ) -> Result<LicenseClaims, LicenseError> {
        Self::verify_token_with_pubkey(raw_token, local_hardware_hash, &BHARTX_ROOT_LICENSE_PUBKEY)
    }

    /// Verifies a license token against an explicit 32-byte public key.
    pub fn verify_token_with_pubkey(
        raw_token: &str,
        local_hardware_hash: &str,
        root_pubkey: &[u8; 32],
    ) -> Result<LicenseClaims, LicenseError> {
        let parts: Vec<&str> = raw_token.split('.').collect();
        if parts.len() != 2 {
            return Err(LicenseError::InvalidFormat);
        }

        let payload_b64 = parts[0];
        let sig_hex = parts[1];

        if sig_hex.len() != 128 {
            return Err(LicenseError::InvalidSignatureLength);
        }

        let sig_bytes = hex::decode(sig_hex).map_err(|_| LicenseError::InvalidSignature)?;
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);

        let payload_bytes = base64::engine::general_purpose::STANDARD
            .decode(payload_b64)
            .map_err(|_| LicenseError::InvalidEncoding)?;

        // Cryptographic verification: verify against decoded payload bytes or b64 string
        let valid = InstallationKeypair::verify_with_public_key(root_pubkey, &payload_bytes, &sig_arr)
            || InstallationKeypair::verify_with_public_key(root_pubkey, payload_b64.as_bytes(), &sig_arr);

        if !valid {
            return Err(LicenseError::CryptographicSignatureFailed);
        }

        // Parse claims JSON
        let claims: LicenseClaims = serde_json::from_slice(&payload_bytes)
            .map_err(|_| LicenseError::MalformedPayload)?;

        // Hardware binding validation
        if claims.hardware_hash != local_hardware_hash {
            return Err(LicenseError::HardwareMismatch);
        }

        // Expiration check (expires_at == 0 means perpetual license)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if claims.expires_at > 0 && claims.expires_at < now {
            return Err(LicenseError::LicenseExpired);
        }

        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_claims(hardware_hash: &str, expires_at: u64) -> LicenseClaims {
        LicenseClaims {
            license_id: "LIC-2026-BHARTX-9941".to_string(),
            hardware_hash: hardware_hash.to_string(),
            customer_name: "Indian Oil Corporation Ltd".to_string(),
            tier: "EnterprisePro".to_string(),
            max_devices: 32,
            features: vec![
                "TURBO_URB_ZSTD".to_string(),
                "ISOCH_JITTER_BUFFER".to_string(),
                "WHITE_LABEL".to_string(),
            ],
            issued_at: 1770000000,
            expires_at,
        }
    }

    #[test]
    fn test_root_pubkey_derivation_consistency() {
        let root_kp = LicenseEngine::root_ca_keypair();
        assert_eq!(*root_kp.public_key(), BHARTX_ROOT_LICENSE_PUBKEY);
    }

    #[test]
    fn test_valid_license_accepted() {
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
        // Far future expiration (year 2035)
        let claims = sample_claims(hw_hash, 2050000000);
        let root_kp = LicenseEngine::root_ca_keypair();
        let token = LicenseEngine::issue_token(&claims, &root_kp).expect("Failed to issue token");

        let verified = LicenseEngine::verify_token(&token, hw_hash)
            .expect("Valid license should be accepted");

        assert_eq!(verified.license_id, "LIC-2026-BHARTX-9941");
        assert_eq!(verified.customer_name, "Indian Oil Corporation Ltd");
        assert_eq!(verified.tier, "EnterprisePro");
        assert!(verified.is_enterprise());
        assert_eq!(verified.max_devices, 32);
        assert!(verified.has_feature("TURBO_URB_ZSTD"));
    }

    #[test]
    fn test_tampered_payload_rejected_with_cryptographic_signature_failed() {
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
        let claims = sample_claims(hw_hash, 2050000000);
        let root_kp = LicenseEngine::root_ca_keypair();
        let token = LicenseEngine::issue_token(&claims, &root_kp).unwrap();

        let parts: Vec<&str> = token.split('.').collect();
        // Modify a single character in the base64 payload
        let mut tampered_b64 = parts[0].to_string();
        if tampered_b64.starts_with('e') {
            tampered_b64.replace_range(0..1, "f");
        } else {
            tampered_b64.replace_range(0..1, "e");
        }
        let tampered_token = format!("{}.{}", tampered_b64, parts[1]);

        let result = LicenseEngine::verify_token(&tampered_token, hw_hash);
        assert_eq!(result, Err(LicenseError::CryptographicSignatureFailed));
    }

    #[test]
    fn test_wrong_signature_rejected_with_cryptographic_signature_failed() {
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
        let claims = sample_claims(hw_hash, 2050000000);
        // Signed with an unauthorized rogue keypair
        let rogue_kp = InstallationKeypair::generate();
        let rogue_token = LicenseEngine::issue_token(&claims, &rogue_kp).unwrap();

        let result = LicenseEngine::verify_token(&rogue_token, hw_hash);
        assert_eq!(result, Err(LicenseError::CryptographicSignatureFailed));
    }

    #[test]
    fn test_hardware_hash_mismatch_rejected() {
        let real_hw_hash = "1111111111111111111111111111111111111111111111111111111111111111";
        let other_hw_hash = "9999999999999999999999999999999999999999999999999999999999999999";
        let claims = sample_claims(real_hw_hash, 2050000000);
        let root_kp = LicenseEngine::root_ca_keypair();
        let token = LicenseEngine::issue_token(&claims, &root_kp).unwrap();

        // Valid signature, but local hardware does not match claims
        let result = LicenseEngine::verify_token(&token, other_hw_hash);
        assert_eq!(result, Err(LicenseError::HardwareMismatch));
    }

    #[test]
    fn test_expired_license_rejected() {
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
        // Expired in 2020 (UNIX timestamp 1577836800)
        let claims = sample_claims(hw_hash, 1577836800);
        let root_kp = LicenseEngine::root_ca_keypair();
        let token = LicenseEngine::issue_token(&claims, &root_kp).unwrap();

        let result = LicenseEngine::verify_token(&token, hw_hash);
        assert_eq!(result, Err(LicenseError::LicenseExpired));
    }

    #[test]
    fn test_invalid_format_and_signature_length() {
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";

        // Missing dot
        assert_eq!(
            LicenseEngine::verify_token("invalid_token_without_dot", hw_hash),
            Err(LicenseError::InvalidFormat)
        );

        // Multiple dots
        assert_eq!(
            LicenseEngine::verify_token("a.b.c", hw_hash),
            Err(LicenseError::InvalidFormat)
        );

        // Short signature hex
        assert_eq!(
            LicenseEngine::verify_token("cGF5bG9hZA==.deadbeef", hw_hash),
            Err(LicenseError::InvalidSignatureLength)
        );
    }
}
