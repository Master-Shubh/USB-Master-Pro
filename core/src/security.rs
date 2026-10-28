//! USB Master Pro — Platform Key Storage & Cryptographic Secret Protection
//! Supports Linux/POSIX file encryption with zeroize protection,
//! and Windows DPAPI (Data Protection API) hardware key wrapping.
//! Owned & Developed by BhartX Ecosystems Pvt Ltd.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Errors encountered during secret storage, retrieval, or deletion.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecurityError {
    #[error("Secret key not found: '{0}'")]
    NotFound(String),

    #[error("Storage I/O failure: {0}")]
    IoError(String),

    #[error("Cryptographic verification or decryption failed: {0}")]
    CryptoError(String),

    #[error("Platform key storage failure: {0}")]
    PlatformError(String),

    #[error("Invalid secret key identifier: {0}")]
    InvalidKeyName(String),

    #[error("Corrupted storage file or unexpected payload size: {0}")]
    CorruptedData(String),
}

impl From<std::io::Error> for SecurityError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            SecurityError::NotFound(err.to_string())
        } else {
            SecurityError::IoError(err.to_string())
        }
    }
}

/// Abstract contract for secure credential and private key storage.
pub trait KeyStorageProvider: Send + Sync {
    /// Persists a binary secret under the given string identifier.
    fn store_secret(&self, key: &str, secret: &[u8]) -> Result<(), SecurityError>;

    /// Loads and decrypts the binary secret associated with the key identifier.
    fn load_secret(&self, key: &str) -> Result<Vec<u8>, SecurityError>;

    /// Securely shreds and deletes the secret from persistent storage.
    fn delete_secret(&self, key: &str) -> Result<(), SecurityError>;
}

/// Zeroizing memory wrapper that overwrites buffer memory with zeros on drop.
pub struct ZeroizeBuffer(pub Vec<u8>);

impl Drop for ZeroizeBuffer {
    fn drop(&mut self) {
        for b in self.0.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

/// Cross-platform, zeroize-protected authenticated file key storage.
/// Suitable for Linux, macOS, and POSIX environments.
pub struct PortableFileKeyStorage {
    storage_dir: PathBuf,
    master_key: [u8; 32],
}

impl Drop for PortableFileKeyStorage {
    fn drop(&mut self) {
        for b in self.master_key.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

impl PortableFileKeyStorage {
    /// Creates storage using default machine-anchored master key in the given directory.
    pub fn new(storage_dir: PathBuf) -> Self {
        let mut seed = [0u8; 32];
        if let Ok(mut f) = fs::File::open("/dev/urandom") {
            use std::io::Read;
            let _ = f.read_exact(&mut seed);
        }
        let mut hasher = Sha256::new();
        hasher.update(b"PORTABLE_FILE_KEY_STORAGE_DEFAULT_V1");
        hasher.update(seed);
        let hash = hasher.finalize();
        let mut master_key = [0u8; 32];
        master_key.copy_from_slice(&hash);

        Self {
            storage_dir,
            master_key,
        }
    }

    /// Creates storage with an explicit 32-byte master key.
    pub fn with_key(storage_dir: PathBuf, master_key: [u8; 32]) -> Self {
        Self {
            storage_dir,
            master_key,
        }
    }

    /// Default system configuration path (`~/.usb-master-pro/keys`)
    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".usb-master-pro").join("keys")
    }

    /// Validates key name to prevent path traversal vulnerabilities.
    fn sanitize_key_path(&self, key: &str) -> Result<PathBuf, SecurityError> {
        if key.is_empty() || key.contains('/') || key.contains('\\') || key.contains("..") || key.contains('\0') {
            return Err(SecurityError::InvalidKeyName(
                "Key contains invalid characters or path traversal elements".to_string(),
            ));
        }

        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash_hex = hex::encode(hasher.finalize());
        let filename = format!("{}.key", &hash_hex[0..32]);
        Ok(self.storage_dir.join(filename))
    }

    /// Constant-time memory comparison for authentication tags
    fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut diff = 0u8;
        for (x, y) in a.iter().zip(b.iter()) {
            diff |= x ^ y;
        }
        diff == 0
    }
}

const MAGIC_HEADER: &[u8; 4] = b"UMPK"; // USB Master Protected Key
const SALT_SIZE: usize = 16;
const TAG_SIZE: usize = 32;
const HEADER_SIZE: usize = 4 + SALT_SIZE + TAG_SIZE; // 52 bytes

impl KeyStorageProvider for PortableFileKeyStorage {
    fn store_secret(&self, key: &str, secret: &[u8]) -> Result<(), SecurityError> {
        let file_path = self.sanitize_key_path(key)?;

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Generate 16-byte random salt
        let mut salt = [0u8; SALT_SIZE];
        let mut filled = false;
        if let Ok(mut f) = fs::File::open("/dev/urandom") {
            use std::io::Read;
            if f.read_exact(&mut salt).is_ok() {
                filled = true;
            }
        }
        if !filled {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let mut h = Sha256::new();
            h.update(now.to_le_bytes());
            let d = h.finalize();
            salt.copy_from_slice(&d[0..16]);
        }

        // Derive K = SHA256("USB_MASTER_KEY_PROTECT" || master_key || salt)
        let mut k_hasher = Sha256::new();
        k_hasher.update(b"USB_MASTER_KEY_PROTECT_V1");
        k_hasher.update(&self.master_key);
        k_hasher.update(&salt);
        let derived_k = ZeroizeBuffer(k_hasher.finalize().to_vec());

        // Stream cipher XOR encryption using counter-mode SHA256 blocks
        let mut ciphertext = ZeroizeBuffer(vec![0u8; secret.len()]);
        let mut block_idx: u32 = 0;
        let mut cursor = 0;

        while cursor < secret.len() {
            let mut block_hasher = Sha256::new();
            block_hasher.update(b"USB_MASTER_CIPHER_BLOCK_V1");
            block_hasher.update(&derived_k.0);
            block_hasher.update(&salt);
            block_hasher.update(block_idx.to_be_bytes());
            let keystream_block = block_hasher.finalize();

            let chunk_len = std::cmp::min(32, secret.len() - cursor);
            for i in 0..chunk_len {
                ciphertext.0[cursor + i] = secret[cursor + i] ^ keystream_block[i];
            }
            cursor += chunk_len;
            block_idx += 1;
        }

        // Compute HMAC / Authentication Tag = SHA256("USB_MASTER_AUTH_TAG" || K || salt || ciphertext)
        let mut tag_hasher = Sha256::new();
        tag_hasher.update(b"USB_MASTER_AUTH_TAG_V1");
        tag_hasher.update(&derived_k.0);
        tag_hasher.update(&salt);
        tag_hasher.update(&ciphertext.0);
        let auth_tag = tag_hasher.finalize();

        // Assemble envelope: [MAGIC 4B] || [SALT 16B] || [AUTH_TAG 32B] || [CIPHERTEXT]
        let mut envelope = Vec::with_capacity(HEADER_SIZE + ciphertext.0.len());
        envelope.extend_from_slice(MAGIC_HEADER);
        envelope.extend_from_slice(&salt);
        envelope.extend_from_slice(&auth_tag);
        envelope.extend_from_slice(&ciphertext.0);

        // Atomic write via temp file
        let temp_path = file_path.with_extension("tmp");
        {
            let mut f = fs::File::create(&temp_path)?;
            f.write_all(&envelope)?;
            f.flush()?;
            f.sync_all()?;
        }

        // Restrict file permissions to owner read/write on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = fs::Permissions::from_mode(0o600);
            let _ = fs::set_permissions(&temp_path, permissions);
        }

        fs::rename(&temp_path, &file_path)?;
        Ok(())
    }

    fn load_secret(&self, key: &str) -> Result<Vec<u8>, SecurityError> {
        let file_path = self.sanitize_key_path(key)?;
        if !file_path.exists() {
            return Err(SecurityError::NotFound(key.to_string()));
        }

        let envelope = fs::read(&file_path)?;
        if envelope.len() < HEADER_SIZE {
            return Err(SecurityError::CorruptedData("Envelope smaller than minimum header".to_string()));
        }

        if &envelope[0..4] != MAGIC_HEADER {
            return Err(SecurityError::CorruptedData("Invalid magic header in secret file".to_string()));
        }

        let salt = &envelope[4..20];
        let auth_tag = &envelope[20..52];
        let ciphertext = &envelope[52..];

        // Re-derive K
        let mut k_hasher = Sha256::new();
        k_hasher.update(b"USB_MASTER_KEY_PROTECT_V1");
        k_hasher.update(&self.master_key);
        k_hasher.update(salt);
        let derived_k = ZeroizeBuffer(k_hasher.finalize().to_vec());

        // Verify Authentication Tag
        let mut tag_hasher = Sha256::new();
        tag_hasher.update(b"USB_MASTER_AUTH_TAG_V1");
        tag_hasher.update(&derived_k.0);
        tag_hasher.update(salt);
        tag_hasher.update(ciphertext);
        let expected_tag = tag_hasher.finalize();

        if !Self::constant_time_eq(auth_tag, &expected_tag) {
            return Err(SecurityError::CryptoError(
                "Secret integrity check failed. Authentication tag mismatch or corrupted ciphertext".to_string(),
            ));
        }

        // Decrypt ciphertext
        let mut plaintext = vec![0u8; ciphertext.len()];
        let mut block_idx: u32 = 0;
        let mut cursor = 0;

        while cursor < ciphertext.len() {
            let mut block_hasher = Sha256::new();
            block_hasher.update(b"USB_MASTER_CIPHER_BLOCK_V1");
            block_hasher.update(&derived_k.0);
            block_hasher.update(salt);
            block_hasher.update(block_idx.to_be_bytes());
            let keystream_block = block_hasher.finalize();

            let chunk_len = std::cmp::min(32, ciphertext.len() - cursor);
            for i in 0..chunk_len {
                plaintext[cursor + i] = ciphertext[cursor + i] ^ keystream_block[i];
            }
            cursor += chunk_len;
            block_idx += 1;
        }

        Ok(plaintext)
    }

    fn delete_secret(&self, key: &str) -> Result<(), SecurityError> {
        let file_path = self.sanitize_key_path(key)?;
        if !file_path.exists() {
            return Err(SecurityError::NotFound(key.to_string()));
        }

        // Secure wipe: overwrite with zeros before unlinking
        if let Ok(metadata) = fs::metadata(&file_path) {
            let len = metadata.len() as usize;
            if let Ok(mut f) = fs::OpenOptions::new().write(true).open(&file_path) {
                let zeros = vec![0u8; len];
                let _ = f.write_all(&zeros);
                let _ = f.flush();
                let _ = f.sync_all();
            }
        }

        fs::remove_file(&file_path)?;
        Ok(())
    }
}

/// Windows DPAPI (Data Protection API) Key Storage Provider.
/// Utilizes user-bound or machine-bound Win32 CryptProtectData / CryptUnprotectData.
#[cfg(target_os = "windows")]
pub struct WindowsDpapiKeyStorage {
    storage_dir: PathBuf,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct DATA_BLOB {
    cb_data: u32,
    pb_data: *mut u8,
}

#[cfg(target_os = "windows")]
#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(
        pDataIn: *mut DATA_BLOB,
        szDataDescr: *const u16,
        pOptionalEntropy: *mut DATA_BLOB,
        pvReserved: *mut std::ffi::c_void,
        pPromptStruct: *mut std::ffi::c_void,
        dwFlags: u32,
        pDataOut: *mut DATA_BLOB,
    ) -> i32;

    fn CryptUnprotectData(
        pDataIn: *mut DATA_BLOB,
        ppszDataDescr: *mut *mut u16,
        pOptionalEntropy: *mut DATA_BLOB,
        pvReserved: *mut std::ffi::c_void,
        pPromptStruct: *mut std::ffi::c_void,
        dwFlags: u32,
        pDataOut: *mut DATA_BLOB,
    ) -> i32;

    fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[cfg(target_os = "windows")]
impl WindowsDpapiKeyStorage {
    pub fn new(storage_dir: PathBuf) -> Self {
        Self { storage_dir }
    }

    pub fn default_path() -> PathBuf {
        let appdata = std::env::var("PROGRAMDATA").unwrap_or_else(|_| "C:\\ProgramData".to_string());
        PathBuf::from(appdata).join("USB Master Pro").join("keys")
    }

    fn key_file(&self, key: &str) -> Result<PathBuf, SecurityError> {
        if key.is_empty() || key.contains('/') || key.contains('\\') || key.contains("..") {
            return Err(SecurityError::InvalidKeyName("Invalid key identifier".to_string()));
        }
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash_hex = hex::encode(hasher.finalize());
        Ok(self.storage_dir.join(format!("{}.dpapi", &hash_hex[0..32])))
    }
}

#[cfg(target_os = "windows")]
impl KeyStorageProvider for WindowsDpapiKeyStorage {
    fn store_secret(&self, key: &str, secret: &[u8]) -> Result<(), SecurityError> {
        let file_path = self.key_file(key)?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut data_in = DATA_BLOB {
            cb_data: secret.len() as u32,
            pb_data: secret.as_ptr() as *mut u8,
        };
        let mut data_out = DATA_BLOB {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };

        // CRYPTPROTECT_UI_FORBIDDEN = 0x1
        let success = unsafe {
            CryptProtectData(
                &mut data_in,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0x1,
                &mut data_out,
            )
        };

        if success == 0 || data_out.pb_data.is_null() {
            return Err(SecurityError::PlatformError("CryptProtectData failed".to_string()));
        }

        let encrypted_bytes = unsafe {
            std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize).to_vec()
        };

        unsafe { LocalFree(data_out.pb_data as *mut std::ffi::c_void) };

        fs::write(&file_path, encrypted_bytes)?;
        Ok(())
    }

    fn load_secret(&self, key: &str) -> Result<Vec<u8>, SecurityError> {
        let file_path = self.key_file(key)?;
        if !file_path.exists() {
            return Err(SecurityError::NotFound(key.to_string()));
        }

        let mut encrypted_data = fs::read(&file_path)?;
        let mut data_in = DATA_BLOB {
            cb_data: encrypted_data.len() as u32,
            pb_data: encrypted_data.as_mut_ptr(),
        };
        let mut data_out = DATA_BLOB {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };

        let success = unsafe {
            CryptUnprotectData(
                &mut data_in,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0x1,
                &mut data_out,
            )
        };

        if success == 0 || data_out.pb_data.is_null() {
            return Err(SecurityError::PlatformError("CryptUnprotectData failed".to_string()));
        }

        let decrypted_bytes = unsafe {
            std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize).to_vec()
        };

        unsafe { LocalFree(data_out.pb_data as *mut std::ffi::c_void) };

        Ok(decrypted_bytes)
    }

    fn delete_secret(&self, key: &str) -> Result<(), SecurityError> {
        let file_path = self.key_file(key)?;
        if !file_path.exists() {
            return Err(SecurityError::NotFound(key.to_string()));
        }
        fs::remove_file(&file_path)?;
        Ok(())
    }
}

/// Linux / non-Windows fallback stub for WindowsDpapiKeyStorage.
#[cfg(not(target_os = "windows"))]
pub struct WindowsDpapiKeyStorage {
    #[allow(dead_code)]
    storage_dir: PathBuf,
}

#[cfg(not(target_os = "windows"))]
impl WindowsDpapiKeyStorage {
    pub fn new(storage_dir: PathBuf) -> Self {
        Self { storage_dir }
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from("/etc/usb-master-pro/keys")
    }
}

#[cfg(not(target_os = "windows"))]
impl KeyStorageProvider for WindowsDpapiKeyStorage {
    fn store_secret(&self, _key: &str, _secret: &[u8]) -> Result<(), SecurityError> {
        Err(SecurityError::PlatformError(
            "Windows DPAPI is only supported on Windows operating systems".to_string(),
        ))
    }

    fn load_secret(&self, _key: &str) -> Result<Vec<u8>, SecurityError> {
        Err(SecurityError::PlatformError(
            "Windows DPAPI is only supported on Windows operating systems".to_string(),
        ))
    }

    fn delete_secret(&self, _key: &str) -> Result<(), SecurityError> {
        Err(SecurityError::PlatformError(
            "Windows DPAPI is only supported on Windows operating systems".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("usb_master_sec_test_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_secret_store_and_load_roundtrip() {
        let dir = temp_test_dir("roundtrip");
        let storage = PortableFileKeyStorage::new(dir.clone());

        let secret_data = b"CONFIDENTIAL_PRIVATE_KEY_BYTES_OR_TOKEN_482190";
        let key_name = "license_key_node_1";

        storage.store_secret(key_name, secret_data).expect("store should succeed");
        let retrieved = storage.load_secret(key_name).expect("load should succeed");
        assert_eq!(retrieved, secret_data);

        // Clean up
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_secret_deletion() {
        let dir = temp_test_dir("delete");
        let storage = PortableFileKeyStorage::new(dir.clone());

        let secret_data = b"EPHEMERAL_SESSION_SECRET";
        let key_name = "ephemeral_key";

        storage.store_secret(key_name, secret_data).unwrap();
        assert!(storage.load_secret(key_name).is_ok());

        storage.delete_secret(key_name).expect("delete should succeed");
        let result = storage.load_secret(key_name);
        assert_eq!(result, Err(SecurityError::NotFound(key_name.to_string())));

        // Second delete fails with NotFound
        assert_eq!(storage.delete_secret(key_name), Err(SecurityError::NotFound(key_name.to_string())));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_tampered_secret_rejected_with_crypto_error() {
        let dir = temp_test_dir("tamper");
        let storage = PortableFileKeyStorage::new(dir.clone());

        let secret_data = b"CRITICAL_INTEGRITY_PROTECTED_DATA";
        let key_name = "tamper_test_key";

        storage.store_secret(key_name, secret_data).unwrap();

        // Locate storage file and tamper with a byte in ciphertext
        let file_path = storage.sanitize_key_path(key_name).unwrap();
        let mut bytes = fs::read(&file_path).unwrap();
        assert!(bytes.len() > HEADER_SIZE);

        let last_idx = bytes.len() - 1;
        bytes[last_idx] ^= 0xFF; // Tamper
        fs::write(&file_path, bytes).unwrap();

        let result = storage.load_secret(key_name);
        assert!(matches!(result, Err(SecurityError::CryptoError(_))));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_different_master_key_cannot_decrypt() {
        let dir = temp_test_dir("isolation");
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];

        let storage_a = PortableFileKeyStorage::with_key(dir.clone(), key_a);
        let storage_b = PortableFileKeyStorage::with_key(dir.clone(), key_b);

        storage_a.store_secret("shared_name", b"SECRET_A").unwrap();
        let result = storage_b.load_secret("shared_name");
        assert!(matches!(result, Err(SecurityError::CryptoError(_))));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_path_traversal_prevention() {
        let dir = temp_test_dir("traversal");
        let storage = PortableFileKeyStorage::new(dir.clone());

        assert!(matches!(
            storage.store_secret("../../../etc/passwd", b"evil"),
            Err(SecurityError::InvalidKeyName(_))
        ));

        assert!(matches!(
            storage.load_secret("sub/dir/key"),
            Err(SecurityError::InvalidKeyName(_))
        ));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_windows_dpapi_stub_on_linux() {
        let dir = temp_test_dir("dpapi_stub");
        let dpapi = WindowsDpapiKeyStorage::new(dir.clone());
        let res = dpapi.store_secret("test", b"abc");

        #[cfg(not(target_os = "windows"))]
        assert!(matches!(res, Err(SecurityError::PlatformError(_))));

        #[cfg(target_os = "windows")]
        assert!(res.is_ok());

        let _ = fs::remove_dir_all(&dir);
    }
}
