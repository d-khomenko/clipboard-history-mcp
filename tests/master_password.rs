use clipboard_history_mcp::core::master_password::{derive_kek, unwrap_master_key, wrap_master_key};

#[test]
fn wrap_unwrap_roundtrip() {
    let master = [42u8; 32];
    let pw = "correct horse battery staple";
    let wrapped = wrap_master_key(&master, pw).unwrap();
    let recovered = unwrap_master_key(&wrapped, pw).unwrap();
    assert_eq!(recovered, master);
}

#[test]
fn unwrap_with_wrong_password_fails() {
    let master = [7u8; 32];
    let wrapped = wrap_master_key(&master, "right-pw").unwrap();
    let err = unwrap_master_key(&wrapped, "wrong-pw").unwrap_err();
    assert!(err.to_string().contains("password") || err.to_string().contains("AEAD"));
}

#[test]
fn kek_is_deterministic_for_same_inputs() {
    let salt = [9u8; 16];
    let a = derive_kek("hello", &salt).unwrap();
    let b = derive_kek("hello", &salt).unwrap();
    assert_eq!(a, b);
}

#[test]
fn kek_differs_for_different_salts() {
    let a = derive_kek("hello", &[1u8; 16]).unwrap();
    let b = derive_kek("hello", &[2u8; 16]).unwrap();
    assert_ne!(a, b);
}
