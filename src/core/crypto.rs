use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use keyring_core::{Entry, Error as KeyringError};
use rand::Rng;

const KEYCHAIN_SERVICE: &str = "clipboard-history-mcp";
const KEYCHAIN_ACCOUNT_V1: &str = "master-key-v1";

/// Initialize the platform keyring store once per process.
///
/// keyring 4 splits into connector crates (keyring) + core (keyring-core).
/// The default store must be set before Entry::new() is called.
/// We call use_native_store(true) which prefers the platform-native store:
///   macOS   → Apple Keychain
///   Linux   → Secret Service (D-Bus / GNOME Keyring / KWallet)
///   Windows → Windows Credential Manager
fn ensure_keyring_store() {
    use once_cell::sync::OnceCell;
    static INIT: OnceCell<()> = OnceCell::new();
    INIT.get_or_init(|| {
        keyring::use_native_store(true).expect("failed to init keyring store");
    });
}

pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12])> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce);
    let n = Nonce::from_slice(&nonce);
    let ct = cipher
        .encrypt(n, plaintext.as_bytes())
        .map_err(|_| anyhow!("AEAD encrypt failed"))?;
    Ok((ct, nonce))
}

pub fn decrypt(ciphertext: &[u8], nonce: &[u8], key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let n = Nonce::from_slice(nonce);
    let pt = cipher.decrypt(n, ciphertext).map_err(|_| anyhow!("AEAD authentication failed"))?;
    Ok(String::from_utf8(pt)?)
}

pub fn get_or_create_master_key(password_provider: impl FnOnce() -> Result<String>) -> Result<[u8; 32]> {
    if let Some(wrapped_entry) = read_master_key_v2()? {
        let pw = password_provider()?;
        let wrapped = crate::core::master_password::WrappedKey {
            salt: wrapped_entry.salt,
            nonce: wrapped_entry.nonce,
            ciphertext: wrapped_entry.ciphertext,
        };
        return crate::core::master_password::unwrap_master_key(&wrapped, &pw);
    }
    if let Some(legacy_key) = read_master_key_v1()? {
        tracing::warn!(
            "master-key-v1 compat mode active. Run `clipboard-history-mcp migrate-v2` \
             to set a master password and enable v4 secret protections."
        );
        return Ok(legacy_key);
    }
    // First-ever run — generate fresh, store as v1 (compat path) until user
    // sets a master password via `clipboard-history-mcp migrate-v2`.
    let k = generate_master_key();
    write_master_key_v1(&k)?;
    Ok(k)
}

pub fn read_master_key_v1() -> Result<Option<[u8; 32]>> {
    ensure_keyring_store();
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V1)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    match entry.get_secret() {
        Ok(bytes) if bytes.len() == 32 => {
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            Ok(Some(k))
        }
        Ok(_) => Err(anyhow!("master-key-v1 has wrong length")),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!("keyring read: {}", e)),
    }
}

fn write_master_key_v1(key: &[u8; 32]) -> Result<()> {
    ensure_keyring_store();
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V1)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    entry.set_secret(key).map_err(|e| anyhow!("keyring write: {}", e))?;
    Ok(())
}

fn generate_master_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    rand::rng().fill_bytes(&mut k);
    k
}

pub fn encrypt_bytes(plaintext: &[u8], key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12])> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce);
    let n = Nonce::from_slice(&nonce);
    let ct = cipher
        .encrypt(n, plaintext)
        .map_err(|_| anyhow!("AEAD encrypt failed"))?;
    Ok((ct, nonce))
}

pub fn decrypt_bytes(ciphertext: &[u8], nonce: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());
    let n = Nonce::from_slice(nonce);
    cipher
        .decrypt(n, ciphertext)
        .map_err(|_| anyhow!("AEAD authentication failed"))
}

const KEYCHAIN_ACCOUNT_V2: &str = "master-key-v2";

pub struct WrappedKeyEntry {
    pub salt: Vec<u8>,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub fn read_master_key_v2() -> Result<Option<WrappedKeyEntry>> {
    use crate::core::master_password::{MIN_CIPHERTEXT_LEN, NONCE_LEN, SALT_LEN};
    ensure_keyring_store();
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V2)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    let bytes = match entry.get_secret() {
        Ok(b) => b,
        Err(KeyringError::NoEntry) => return Ok(None),
        Err(e) => return Err(anyhow!("keyring read v2: {}", e)),
    };
    if bytes.len() < SALT_LEN + NONCE_LEN + MIN_CIPHERTEXT_LEN {
        return Err(anyhow!("master-key-v2 has invalid length: {}", bytes.len()));
    }
    let salt = bytes[..SALT_LEN].to_vec();
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&bytes[SALT_LEN..SALT_LEN + NONCE_LEN]);
    let ciphertext = bytes[SALT_LEN + NONCE_LEN..].to_vec();
    Ok(Some(WrappedKeyEntry { salt, nonce, ciphertext }))
}

pub fn write_master_key_v2(wrapped: &crate::core::master_password::WrappedKey) -> Result<()> {
    ensure_keyring_store();
    let mut buf = Vec::with_capacity(16 + 12 + wrapped.ciphertext.len());
    buf.extend_from_slice(&wrapped.salt);
    buf.extend_from_slice(&wrapped.nonce);
    buf.extend_from_slice(&wrapped.ciphertext);
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V2)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    entry.set_secret(&buf).map_err(|e| anyhow!("keyring write v2: {}", e))?;
    Ok(())
}

pub fn delete_master_key_v1() -> Result<()> {
    ensure_keyring_store();
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V1)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    match entry.delete_credential() {
        Ok(_) | Err(KeyringError::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!("keyring delete v1: {}", e)),
    }
}

#[cfg(test)]
mod encrypt_bytes_tests {
    use super::*;

    #[test]
    fn encrypt_bytes_returns_result() {
        let key = [0u8; 32];
        let r: Result<(Vec<u8>, [u8; 12])> = encrypt_bytes(b"hello", &key);
        assert!(r.is_ok());
    }
}
