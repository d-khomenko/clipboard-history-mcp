use clipboard_history_mcp::core::crypto::{encrypt, decrypt};

#[test]
fn roundtrip_utf8() {
    let key = [0u8; 32];
    let plaintext = "sk-proj-abcXYZ123";
    let (ciphertext, nonce) = encrypt(plaintext, &key).unwrap();
    assert_eq!(nonce.len(), 12);
    assert!(ciphertext.len() > plaintext.len());
    let recovered = decrypt(&ciphertext, &nonce, &key).unwrap();
    assert_eq!(recovered, plaintext);
}

#[test]
fn rejects_tampered_ciphertext() {
    let key = [0u8; 32];
    let (mut ct, nonce) = encrypt("hello", &key).unwrap();
    ct[0] ^= 0xFF;
    assert!(decrypt(&ct, &nonce, &key).is_err());
}

#[test]
fn rejects_wrong_key() {
    let k1 = [0u8; 32];
    let k2 = [1u8; 32];
    let (ct, nonce) = encrypt("hello", &k1).unwrap();
    assert!(decrypt(&ct, &nonce, &k2).is_err());
}

#[test]
fn encrypt_returns_result_not_panic() {
    let key = [0u8; 32];
    // Even on the impossible-error path the type must be Result so the
    // compiler proves we never panic in production.
    let r: Result<(Vec<u8>, [u8; 12]), anyhow::Error> = encrypt("test", &key);
    assert!(r.is_ok());
}
