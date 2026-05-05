use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use rand::RngCore;
use security_framework::passwords::{set_generic_password, get_generic_password};

const KEYCHAIN_SERVICE: &str = "clipboard-history-mcp";
const KEYCHAIN_ACCOUNT: &str = "master-key-v1";

pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> (Vec<u8>, [u8; 12]) {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let n = Nonce::from_slice(&nonce);
    let ct = cipher.encrypt(n, plaintext.as_bytes()).expect("encrypt");
    (ct, nonce)
}

pub fn decrypt(ciphertext: &[u8], nonce: &[u8], key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let n = Nonce::from_slice(nonce);
    let pt = cipher.decrypt(n, ciphertext).map_err(|_| anyhow!("AEAD authentication failed"))?;
    Ok(String::from_utf8(pt)?)
}

pub fn get_or_create_master_key() -> Result<[u8; 32]> {
    if let Ok(bytes) = get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        if bytes.len() == 32 {
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            return Ok(k);
        }
    }
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, &k)
        .map_err(|e| anyhow!("Keychain write failed: {}", e))?;
    Ok(k)
}
