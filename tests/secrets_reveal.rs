use clipboard_history_mcp::core::store::{Store, SecretInput};
use tempfile::TempDir;

fn make_store() -> (TempDir, Store) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path, [0u8; 32]).unwrap();
    (tmp, store)
}

#[test]
fn last_revealed_at_set_after_unlock() {
    let (_tmp, store) = make_store();

    // 1. Add a secret.
    let id = store.add_secret(SecretInput {
        text: "hunter2".into(),
        secret_kind: "password".into(),
        source_app: None,
        window_title: None,
    }).unwrap();

    // 2. Assert last_revealed_at IS NULL before any reveal.
    let before = store.last_revealed_at(id).unwrap();
    assert!(before.is_none(), "last_revealed_at should be NULL before unlock");

    // 3. Unlock — should succeed and return plaintext.
    let ts_before = {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
    };
    let value = store.unlock_secret(id).unwrap();
    assert_eq!(value, "hunter2");

    // 4. Assert last_revealed_at IS NOT NULL and recent (within last 5 seconds).
    let after = store.last_revealed_at(id).unwrap();
    let ts = after.expect("last_revealed_at should be set after unlock");
    assert!(ts >= ts_before, "last_revealed_at should be >= timestamp before unlock");
    assert!(ts <= ts_before + 5_000, "last_revealed_at should be within 5 seconds");
}
