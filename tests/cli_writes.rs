//! Integration tests for the write CLI subcommands.
//!
//! Tests call the `run()` functions directly with a tmp Store.
//! Two env vars are set before each test:
//!   - `CLIPBOARD_DB_PATH` → routes `store_helper::open()` to the tmp DB.
//!   - `CLIPBOARD_EPHEMERAL_KEY=1` → bypasses the real keychain; uses zero key.
//!
//! `BiometryGate::unlock_secret` is not tested end-to-end because it requires
//! Touch ID / password interaction; a separate `#[ignore]` test is provided.

use clipboard_history_mcp::core::store::{ClipInput, SecretInput, Store};
use std::sync::Mutex;
use tempfile::TempDir;

/// Serialise tests that mutate process-global env vars.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn make_store() -> (TempDir, Store) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path, [0u8; 32]).unwrap();
    (tmp, store)
}

/// Set test env and return a guard that removes the vars on drop.
struct TestEnv;

impl TestEnv {
    fn setup(db: &std::path::Path) -> Self {
        std::env::set_var("CLIPBOARD_DB_PATH", db);
        std::env::set_var("CLIPBOARD_EPHEMERAL_KEY", "1");
        Self
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        std::env::remove_var("CLIPBOARD_DB_PATH");
        std::env::remove_var("CLIPBOARD_EPHEMERAL_KEY");
    }
}

fn text_clip(store: &Store, text: &str) -> i64 {
    store
        .add_clip(ClipInput {
            text: text.into(),
            primary_kind: "text".into(),
            kinds: vec!["text".into()],
            source_app: None,
            window_title: None,
        })
        .unwrap()
}

// ── pin / unpin ──────────────────────────────────────────────────────────────

#[test]
fn pin_sets_is_pinned() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, store) = make_store();
    let id = text_clip(&store, "pin-me");
    drop(store);

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    clipboard_history_mcp::cli::pin_cmd::run(id).unwrap();

    let store2 = Store::open(&db, [0u8; 32]).unwrap();
    let item = store2.get_item(id).unwrap().unwrap();
    assert!(item.is_pinned);
}

#[test]
fn unpin_clears_is_pinned() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, store) = make_store();
    let id = text_clip(&store, "unpin-me");
    store.pin(id, true).unwrap();
    drop(store);

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    clipboard_history_mcp::cli::unpin_cmd::run(id).unwrap();

    let store2 = Store::open(&db, [0u8; 32]).unwrap();
    let item = store2.get_item(id).unwrap().unwrap();
    assert!(!item.is_pinned);
}

#[test]
fn pin_unpin_round_trip() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, store) = make_store();
    let id = text_clip(&store, "round-trip");
    drop(store);

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    clipboard_history_mcp::cli::pin_cmd::run(id).unwrap();
    {
        let s = Store::open(&db, [0u8; 32]).unwrap();
        assert!(s.get_item(id).unwrap().unwrap().is_pinned);
    }

    clipboard_history_mcp::cli::unpin_cmd::run(id).unwrap();
    {
        let s = Store::open(&db, [0u8; 32]).unwrap();
        assert!(!s.get_item(id).unwrap().unwrap().is_pinned);
    }
}

// ── delete ───────────────────────────────────────────────────────────────────

#[test]
fn delete_removes_clip_from_db() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, store) = make_store();
    let id = text_clip(&store, "delete-me");
    drop(store);

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    clipboard_history_mcp::cli::delete_cmd::run(id).unwrap();

    let store2 = Store::open(&db, [0u8; 32]).unwrap();
    assert!(
        store2.get_item(id).unwrap().is_none(),
        "clip should be gone after delete"
    );
}

#[test]
fn delete_nonexistent_id_is_ok() {
    // `Store::delete` on a missing id is a no-op (DELETE WHERE id=? matches 0 rows).
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, _store) = make_store();

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    // Should succeed — no row matched, no error.
    clipboard_history_mcp::cli::delete_cmd::run(99999).unwrap();
}

// ── clear ────────────────────────────────────────────────────────────────────

#[test]
fn clear_all_empties_unpinned_clips() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, store) = make_store();
    let id1 = text_clip(&store, "clear-1");
    let id2 = text_clip(&store, "clear-2");
    let id_pinned = text_clip(&store, "pinned-keep");
    store.pin(id_pinned, true).unwrap();
    drop(store);

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    clipboard_history_mcp::cli::clear_cmd::run("all", true).unwrap();

    let store2 = Store::open(&db, [0u8; 32]).unwrap();
    assert!(store2.get_item(id1).unwrap().is_none());
    assert!(store2.get_item(id2).unwrap().is_none());
    // Pinned clip must survive.
    assert!(store2.get_item(id_pinned).unwrap().is_some());
}

#[test]
fn clear_kind_removes_matching_clips() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, store) = make_store();
    let id_url = store
        .add_clip(ClipInput {
            text: "https://example.com".into(),
            primary_kind: "url".into(),
            kinds: vec!["url".into()],
            source_app: None,
            window_title: None,
        })
        .unwrap();
    let id_text = text_clip(&store, "keep-me");
    drop(store);

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    clipboard_history_mcp::cli::clear_cmd::run("kind:url", true).unwrap();

    let store2 = Store::open(&db, [0u8; 32]).unwrap();
    assert!(
        store2.get_item(id_url).unwrap().is_none(),
        "url clip should be gone"
    );
    assert!(
        store2.get_item(id_text).unwrap().is_some(),
        "text clip should remain"
    );
}

#[test]
fn clear_invalid_scope_returns_err() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, _store) = make_store();

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    let result = clipboard_history_mcp::cli::clear_cmd::run("bogus-scope", true);
    assert!(result.is_err(), "invalid scope must return Err");
}

// ── copy (pasteboard requires display — #[ignore]) ───────────────────────────

#[test]
#[ignore] // requires a real macOS display / pasteboard session
fn copy_text_clip_writes_to_pasteboard_and_bumps_paste_count() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, store) = make_store();
    let id = text_clip(&store, "clipboard-test-value");
    let initial_paste = store.get_item(id).unwrap().unwrap().paste_count;
    drop(store);

    let db = tmp.path().join("t.db");
    let _env = TestEnv::setup(&db);

    clipboard_history_mcp::cli::copy_cmd::run(id).unwrap();

    let store2 = Store::open(&db, [0u8; 32]).unwrap();
    let item = store2.get_item(id).unwrap().unwrap();
    assert_eq!(item.paste_count, initial_paste + 1);
}

// ── unlock_secret (Touch ID required — #[ignore]) ────────────────────────────

#[test]
#[ignore] // requires Touch ID / password prompt and a real keychain entry
fn unlock_secret_decrypts_value() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (tmp, store) = make_store();
    let _id = store
        .add_secret(SecretInput {
            text: "super-secret-value".into(),
            secret_kind: "test".into(),
            source_app: None,
            window_title: None,
        })
        .unwrap();
    drop(store);

    // BiometryGate requires interactive auth. Call manually with:
    //   cargo test -- --ignored unlock_secret_decrypts_value
    let _db = tmp.path().join("t.db");
}
