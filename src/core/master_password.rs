use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;

const SALT_LEN: usize = 16;
const KEK_LEN: usize = 32;

pub struct WrappedKey {
    pub salt: Vec<u8>,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub fn derive_kek(password: &str, salt: &[u8]) -> Result<[u8; KEK_LEN]> {
    let params = Params::new(19_456, 2, 1, Some(KEK_LEN))
        .map_err(|e| anyhow!("argon2 params: {}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; KEK_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| anyhow!("argon2 hash: {}", e))?;
    Ok(out)
}

pub fn wrap_master_key(master: &[u8; 32], password: &str) -> Result<WrappedKey> {
    let mut salt = vec![0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    let kek = derive_kek(password, &salt)?;
    let (ct, nonce) = crate::core::crypto::encrypt_bytes(master, &kek);
    Ok(WrappedKey { salt, nonce, ciphertext: ct })
}

pub fn unwrap_master_key(wrapped: &WrappedKey, password: &str) -> Result<[u8; 32]> {
    let kek = derive_kek(password, &wrapped.salt)?;
    let pt = crate::core::crypto::decrypt_bytes(&wrapped.ciphertext, &wrapped.nonce, &kek)
        .context("incorrect master password or corrupted vault")?;
    let mut k = [0u8; 32];
    if pt.len() != 32 {
        return Err(anyhow!("decrypted master key is not 32 bytes"));
    }
    k.copy_from_slice(&pt);
    Ok(k)
}

pub fn prompt_password(prompt: &str) -> Result<String> {
    rpassword::prompt_password(prompt).map_err(|e| anyhow!("password prompt: {}", e))
}

pub fn prompt_password_with_confirmation() -> Result<String> {
    let p1 = prompt_password("Set a master password: ")?;
    let p2 = prompt_password("Confirm master password: ")?;
    if p1 != p2 {
        return Err(anyhow!("passwords did not match"));
    }
    if p1.len() < 8 {
        return Err(anyhow!("password must be at least 8 characters"));
    }
    Ok(p1)
}
