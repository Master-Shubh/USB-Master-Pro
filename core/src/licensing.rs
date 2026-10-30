//! USB Master Pro — Cryptographic Licensing Engine
//! Validates offline, hardware-anchored Ed25519 signed license tokens.
//! Developed & Owned by BhartX Ecosystems Pvt Ltd.

use crate::identity::InstallationKeypair;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// BhartX Production Root CA Public Key (Ed25519 - RFC 8032)
/// Used for offline verification of cryptographic license tokens in production.
/// Root private seed is stored strictly offline / off-repo in HSM/secure vault.
pub const BHARTX_PRODUCTION_ROOT_LICENSE_PUBKEY: [u8; 32] = [
    102, 79, 204, 176, 207, 73, 52, 236, 91, 182, 71, 184, 161, 84, 59, 155,
    25, 65, 43, 63, 15, 190, 249, 162, 24, 100, 252, 179, 181, 8, 87, 204,
];

/// BhartX Root CA Public Key alias for backwards compatibility.
pub const BHARTX_ROOT_LICENSE_PUBKEY: [u8; 32] = BHARTX_PRODUCTION_ROOT_LICENSE_PUBKEY;

/// BhartX Test Root CA Seed (COMPROMISED - STRICTLY FOR IN-TREE TEST HARNESS ONLY).
/// SECURITY INVARIANT: Must NEVER be embedded in release client binaries or treated as a real production CA.
/// Any token signed with this seed MUST be rejected by the production verifier.
#[cfg(any(test, feature = "test-utils"))]
pub const BHARTX_TEST_ROOT_SEED: [u8; 32] = *b"BHARTX_MOCK_TEST_HARNESS_KEY_001";

/// Legacy alias for test harness.
#[cfg(any(test, feature = "test-utils"))]
pub const BHARTX_ROOT_SEED: [u8; 32] = BHARTX_TEST_ROOT_SEED;

/// Static production-signed license fixture for hermetic CI and test harness validation.
/// Payload is signed by the BhartX Production Root CA and anchored to STATIC_PRODUCTION_FIXTURE_HW_HASH.
#[cfg(any(test, feature = "test-utils"))]
pub const STATIC_PRODUCTION_FIXTURE_TOKEN: &str = "eyJsaWNlbnNlX2lkIjoiTElDLTIwMjYtQkhBUlRYLUNFUlRJRklFRCIsImhhcmR3YXJlX2hhc2giOiI4ZmEwM2E3NTk4M2NlZGQ5ZDM2N2E0OWQ0ZmYwZWIwYmVhODk0Njk3ZmMwMmNmYjRmZmE5Nzc0OGMzNWZkZmVhIiwiY3VzdG9tZXJfbmFtZSI6IlVJREFJIEFhZGhhYXIgU2V2YSBLZW5kcmEiLCJ0aWVyIjoiRW50ZXJwcmlzZVBybyIsIm1heF9kZXZpY2VzIjo2NCwiZmVhdHVyZXMiOlsiVFVSQk9fVVJCX1pTVEQiLCJJU09DSF9KSVRURVJfQlVGRkVSIiwiRUdPVl9SRF9PUFRJTUlaRUQiXSwiaXNzdWVkX2F0IjoxNzcwMDAwMDAwLCJleHBpcmVzX2F0IjoyMDUwMDAwMDAwfQ==.4fabad0931c1a4ec872d8604ca43c8ca1f3f564c40e4594c339318725c19089fbf4989513f33b6997fcacbeada3ae64c03d6cc0160cae1f54da7bede5b549f03";

/// Hardware hash corresponding to the static production fixture token.
#[cfg(any(test, feature = "test-utils"))]
pub const STATIC_PRODUCTION_FIXTURE_HW_HASH: &str = "8fa03a75983cedd9d367a49d4ff0eb0bea894697fc02cfb4ffa97748c35fdfea";

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
    /// Obtains the BhartX Test Root CA keypair (derived from the compromised test seed).
    /// Gated exclusively to test harness and test-utils feature; physically absent in production client builds.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn test_ca_keypair() -> InstallationKeypair {
        InstallationKeypair::from_seed(&BHARTX_TEST_ROOT_SEED)
    }

    /// Obtains the BhartX Root CA keypair for testing and synthetic token generation.
    /// Prefers the offline production keypair if present at `/home/shubh/.config/bhartx/licensing/production_root_ca.key`
    /// (or custom path via BHARTX_PRODUCTION_ROOT_KEY_PATH).
    /// Falls back to the test CA keypair.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn root_ca_keypair() -> InstallationKeypair {
        if let Some(kp) = Self::load_offline_production_keypair() {
            kp
        } else {
            Self::test_ca_keypair()
        }
    }

    /// Loads the offline production root keypair from the filesystem if present.
    /// Gated exclusively to test / test-utils. NEVER compiled into production client builds.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn load_offline_production_keypair() -> Option<InstallationKeypair> {
        let key_path = std::env::var("BHARTX_PRODUCTION_ROOT_KEY_PATH")
            .unwrap_or_else(|_| "/home/shubh/.config/bhartx/licensing/production_root_ca.key".to_string());
        if let Ok(bytes) = std::fs::read(&key_path) {
            if bytes.len() == 32 {
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&bytes);
                return Some(InstallationKeypair::from_seed(&seed));
            } else if bytes.len() >= 64 {
                if let Ok(hex_str) = std::str::from_utf8(&bytes) {
                    if let Ok(decoded) = hex::decode(hex_str.trim()) {
                        if decoded.len() == 32 {
                            let mut seed = [0u8; 32];
                            seed.copy_from_slice(&decoded);
                            return Some(InstallationKeypair::from_seed(&seed));
                        }
                    }
                }
            }
        }
        None
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

    /// Verifies a license token against the embedded BhartX Production Root CA public key
    /// and the machine's local hardware hash.
    /// In production builds, MUST trust ONLY BHARTX_PRODUCTION_ROOT_LICENSE_PUBKEY with ZERO fallback.
    pub fn verify_token(
        raw_token: &str,
        local_hardware_hash: &str,
    ) -> Result<LicenseClaims, LicenseError> {
        Self::verify_token_with_pubkey(
            raw_token,
            local_hardware_hash,
            &BHARTX_PRODUCTION_ROOT_LICENSE_PUBKEY,
        )
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
        let test_kp = LicenseEngine::test_ca_keypair();
        let expected_test_pubkey = InstallationKeypair::from_seed(&BHARTX_TEST_ROOT_SEED).public_key;
        assert_eq!(*test_kp.public_key(), expected_test_pubkey);

        if let Some(prod_kp) = LicenseEngine::load_offline_production_keypair() {
            assert_eq!(*prod_kp.public_key(), BHARTX_PRODUCTION_ROOT_LICENSE_PUBKEY);
        }
    }

    #[test]
    fn test_valid_license_accepted() {
        // 1. Verify static production-signed fixture against production verifier
        let verified_prod = LicenseEngine::verify_token(
            STATIC_PRODUCTION_FIXTURE_TOKEN,
            STATIC_PRODUCTION_FIXTURE_HW_HASH,
        ).expect("Static production fixture must be accepted by production verifier");

        assert_eq!(verified_prod.license_id, "LIC-2026-BHARTX-CERTIFIED");
        assert_eq!(verified_prod.customer_name, "UIDAI Aadhaar Seva Kendra");
        assert_eq!(verified_prod.tier, "EnterprisePro");
        assert!(verified_prod.is_enterprise());
        assert_eq!(verified_prod.max_devices, 64);
        assert!(verified_prod.has_feature("TURBO_URB_ZSTD"));
        assert!(verified_prod.has_feature("ISOCH_JITTER_BUFFER"));
        assert!(verified_prod.has_feature("EGOV_RD_OPTIMIZED"));

        // 2. Test round-trip dynamic issuance and verification with test CA keypair
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
        let claims = sample_claims(hw_hash, 2050000000);
        let test_kp = LicenseEngine::test_ca_keypair();
        let token = LicenseEngine::issue_token(&claims, &test_kp).expect("Failed to issue test token");

        let verified_test = LicenseEngine::verify_token_with_pubkey(&token, hw_hash, test_kp.public_key())
            .expect("Dynamic test token must be accepted by verify_token_with_pubkey");
        assert_eq!(verified_test.license_id, "LIC-2026-BHARTX-9941");
        assert_eq!(verified_test.customer_name, "Indian Oil Corporation Ltd");
        assert_eq!(verified_test.tier, "EnterprisePro");
        assert!(verified_test.is_enterprise());
        assert_eq!(verified_test.max_devices, 32);
        assert!(verified_test.has_feature("TURBO_URB_ZSTD"));
    }

    #[test]
    fn test_tampered_payload_rejected_with_cryptographic_signature_failed() {
        let parts: Vec<&str> = STATIC_PRODUCTION_FIXTURE_TOKEN.split('.').collect();
        // Modify a single character in the base64 payload
        let mut tampered_b64 = parts[0].to_string();
        if tampered_b64.starts_with('e') {
            tampered_b64.replace_range(0..1, "f");
        } else {
            tampered_b64.replace_range(0..1, "e");
        }
        let tampered_token = format!("{}.{}", tampered_b64, parts[1]);

        let result = LicenseEngine::verify_token(&tampered_token, STATIC_PRODUCTION_FIXTURE_HW_HASH);
        assert_eq!(result, Err(LicenseError::CryptographicSignatureFailed));
    }

    #[test]
    fn test_wrong_signature_rejected_with_cryptographic_signature_failed() {
        let claims = sample_claims(STATIC_PRODUCTION_FIXTURE_HW_HASH, 2050000000);
        // Signed with an unauthorized rogue keypair
        let rogue_kp = InstallationKeypair::generate();
        let rogue_token = LicenseEngine::issue_token(&claims, &rogue_kp).unwrap();

        let result = LicenseEngine::verify_token(&rogue_token, STATIC_PRODUCTION_FIXTURE_HW_HASH);
        assert_eq!(result, Err(LicenseError::CryptographicSignatureFailed));
    }

    #[test]
    fn test_hardware_hash_mismatch_rejected() {
        let other_hw_hash = "9999999999999999999999999999999999999999999999999999999999999999";

        // Production verifier rejects hardware hash mismatch on validly signed production fixture
        let result = LicenseEngine::verify_token(STATIC_PRODUCTION_FIXTURE_TOKEN, other_hw_hash);
        assert_eq!(result, Err(LicenseError::HardwareMismatch));

        // Test keypair dynamic token also rejects hardware hash mismatch
        let real_hw_hash = "1111111111111111111111111111111111111111111111111111111111111111";
        let claims = sample_claims(real_hw_hash, 2050000000);
        let test_kp = LicenseEngine::test_ca_keypair();
        let token = LicenseEngine::issue_token(&claims, &test_kp).unwrap();

        let test_result = LicenseEngine::verify_token_with_pubkey(&token, other_hw_hash, test_kp.public_key());
        assert_eq!(test_result, Err(LicenseError::HardwareMismatch));
    }

    #[test]
    fn test_expired_license_rejected() {
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
        // Expired in 2020 (UNIX timestamp 1577836800)
        let claims = sample_claims(hw_hash, 1577836800);
        let test_kp = LicenseEngine::test_ca_keypair();
        let token = LicenseEngine::issue_token(&claims, &test_kp).unwrap();

        let result = LicenseEngine::verify_token_with_pubkey(&token, hw_hash, test_kp.public_key());
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

    #[test]
    fn test_production_verifier_strictly_rejects_test_key_signed_tokens() {
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
        let claims = sample_claims(hw_hash, 2050000000);
        let test_kp = LicenseEngine::test_ca_keypair();
        let token = LicenseEngine::issue_token(&claims, &test_kp).expect("Failed to issue test token");

        // Production verifier MUST strictly reject tokens signed by the compromised test seed
        let result = LicenseEngine::verify_token(&token, hw_hash);
        assert_eq!(result, Err(LicenseError::CryptographicSignatureFailed));
    }

    #[test]
    fn test_production_verifier_accepts_production_key_signed_tokens() {
        // Hermetic CI verification using static production-signed fixture
        let verified = LicenseEngine::verify_token(
            STATIC_PRODUCTION_FIXTURE_TOKEN,
            STATIC_PRODUCTION_FIXTURE_HW_HASH,
        ).expect("Production token fixture must be accepted");
        assert_eq!(verified.license_id, "LIC-2026-BHARTX-CERTIFIED");
        assert_eq!(verified.tier, "EnterprisePro");
        assert!(verified.is_enterprise());

        // Dynamic offline issuance check when developer environment has the offline keypair
        if let Some(prod_kp) = LicenseEngine::load_offline_production_keypair() {
            assert_eq!(*prod_kp.public_key(), BHARTX_PRODUCTION_ROOT_LICENSE_PUBKEY);
            let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
            let claims = sample_claims(hw_hash, 2050000000);
            let dynamic_token = LicenseEngine::issue_token(&claims, &prod_kp).expect("Failed to issue production token");
            let dynamic_verified = LicenseEngine::verify_token(&dynamic_token, hw_hash).expect("Dynamic production token must be accepted");
            assert_eq!(dynamic_verified.license_id, "LIC-2026-BHARTX-9941");
            assert_eq!(dynamic_verified.tier, "EnterprisePro");
            assert!(dynamic_verified.is_enterprise());
        }
    }

    #[test]
    fn test_tampered_token_rejected_under_ed25519_dalek() {
        let hw_hash = STATIC_PRODUCTION_FIXTURE_HW_HASH;
        let token = STATIC_PRODUCTION_FIXTURE_TOKEN;

        let parts: Vec<&str> = token.split('.').collect();
        let b64_payload = parts[0];
        let sig_hex = parts[1];

        // 1. Bit flip in payload
        let mut tampered_b64 = b64_payload.to_string();
        let last_char = tampered_b64.pop().unwrap();
        let flipped = if last_char == 'A' { 'B' } else { 'A' };
        tampered_b64.push(flipped);
        let tampered_payload_token = format!("{}.{}", tampered_b64, sig_hex);
        assert!(LicenseEngine::verify_token(&tampered_payload_token, hw_hash).is_err());

        // 2. Modifying 1 bit of signature
        let mut sig_bytes = hex::decode(sig_hex).unwrap();
        sig_bytes[0] ^= 0x01; // flip 1 bit
        let tampered_sig_hex = hex::encode(sig_bytes);
        let tampered_sig_token = format!("{}.{}", b64_payload, tampered_sig_hex);
        let result = LicenseEngine::verify_token(&tampered_sig_token, hw_hash);
        assert_eq!(result, Err(LicenseError::CryptographicSignatureFailed));
    }

    #[test]
    fn test_legacy_sha256_forgery_impossible() {
        let hw_hash = "a4f89d816c4f74d0932e6a9821804d9d15024b423714bfa4d82260ff8719bc01";
        let claims = sample_claims(hw_hash, 2050000000);
        let json_bytes = serde_json::to_vec(&claims).unwrap();
        let payload_b64 = base64::engine::general_purpose::STANDARD.encode(&json_bytes);

        // Craft legacy simulated signature with arbitrary R and SHA-256 S commitment
        use sha2::{Digest, Sha256};
        let fake_r = [0x42u8; 32];
        let mut s_hasher = Sha256::new();
        s_hasher.update(b"USB_MASTER_ED25519_SIG_RESPONSE_S");
        s_hasher.update(&fake_r);
        s_hasher.update(&BHARTX_PRODUCTION_ROOT_LICENSE_PUBKEY);
        s_hasher.update(&json_bytes);
        let fake_s = s_hasher.finalize();

        let mut fake_sig = [0u8; 64];
        fake_sig[0..32].copy_from_slice(&fake_r);
        fake_sig[32..64].copy_from_slice(&fake_s);
        let fake_sig_hex = hex::encode(fake_sig);

        let forged_token = format!("{}.{}", payload_b64, fake_sig_hex);

        // Strict Ed25519 Dalek verification MUST reject this arbitrary R/S forgery
        let result = LicenseEngine::verify_token(&forged_token, hw_hash);
        assert_eq!(result, Err(LicenseError::CryptographicSignatureFailed));
    }
}
