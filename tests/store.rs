use clipboard_history_mcp::core::store::{Store, ClipInput, SecretInput, ImageClipInput};
use std::sync::Mutex;
use tempfile::TempDir;

/// Tests that mutate `CLIPBOARD_DATA_DIR` need to serialise — Cargo runs
/// integration tests in parallel and the env is process-global.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn make_store() -> (TempDir, Store) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path, [0u8; 32]).unwrap();
    (tmp, store)
}

#[test]
fn insert_clip() {
    let (_tmp, store) = make_store();
    let id = store.add_clip(ClipInput {
        text: "hello".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: Some("Term".into()),
        window_title: None,
    }).unwrap();
    assert!(id > 0);
    let item = store.get_item(id).unwrap().unwrap();
    assert_eq!(item.text.unwrap(), "hello");
    assert_eq!(item.primary_kind, "text");
}

#[test]
fn dedup_by_hash() {
    let (_tmp, store) = make_store();
    let i1 = store.add_clip(ClipInput { text: "x".into(), primary_kind: "text".into(), kinds: vec!["text".into()], source_app: None, window_title: None }).unwrap();
    let i2 = store.add_clip(ClipInput { text: "x".into(), primary_kind: "text".into(), kinds: vec!["text".into()], source_app: None, window_title: None }).unwrap();
    assert_eq!(i1, i2);
    let item = store.get_item(i1).unwrap().unwrap();
    assert_eq!(item.copy_count, 2);
}

#[test]
fn secret_redacted_and_unlocks() {
    let (_tmp, store) = make_store();
    let id = store.add_secret(SecretInput {
        text: "sk-abc".into(),
        secret_kind: "openai".into(),
        source_app: Some("Safari".into()),
        window_title: Some("OpenAI".into()),
    }).unwrap();
    let item = store.get_item(id).unwrap().unwrap();
    assert!(item.text.is_none());
    assert!(item.preview.contains("REDACTED"));
    let value = store.unlock_secret(id).unwrap();
    assert_eq!(value, "sk-abc");
}

#[test]
fn fts_finds_secret_by_window_title() {
    let (_tmp, store) = make_store();
    store.add_secret(SecretInput {
        text: "sk-abc".into(),
        secret_kind: "openai".into(),
        source_app: Some("Safari".into()),
        window_title: Some("OpenAI Platform".into()),
    }).unwrap();
    let hits = store.search("OpenAI", 10).unwrap();
    assert!(!hits.is_empty());
    assert!(hits[0].text.is_none());
}

#[test]
fn add_image_clip_writes_blob_and_row() {
    let _g = ENV_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path, [0u8; 32]).unwrap();

    let bytes = b"\x89PNG\r\n\x1a\n".to_vec(); // PNG header — not a real image, but enough for hash
    let id = store.add_image_clip(ImageClipInput {
        bytes: bytes.clone(),
        mime: "image/png".into(),
        source_app: Some("Preview".into()),
        window_title: None,
    }).unwrap();

    let item = store.get_item(id).unwrap().unwrap();
    assert_eq!(item.payload_kind, "image");
    assert_eq!(item.mime_type.as_deref(), Some("image/png"));
    assert!(item.blob_path.as_ref().unwrap().ends_with(".png"));
    assert_eq!(item.blob_size_bytes, Some(bytes.len() as i64));

    std::env::remove_var("CLIPBOARD_DATA_DIR");
}

#[test]
fn add_image_clip_dedups_on_repeat() {
    let _g = ENV_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path, [0u8; 32]).unwrap();

    let bytes = b"identical png bytes".to_vec();
    let id1 = store.add_image_clip(ImageClipInput {
        bytes: bytes.clone(), mime: "image/png".into(),
        source_app: None, window_title: None,
    }).unwrap();
    let id2 = store.add_image_clip(ImageClipInput {
        bytes: bytes.clone(), mime: "image/png".into(),
        source_app: None, window_title: None,
    }).unwrap();

    assert_eq!(id1, id2, "second call should return the same row id");
    let item = store.get_item(id1).unwrap().unwrap();
    assert_eq!(item.copy_count, 2);

    std::env::remove_var("CLIPBOARD_DATA_DIR");
}

#[test]
fn delete_image_clip_removes_blob_file() {
    let _g = ENV_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path, [0u8; 32]).unwrap();

    let id = store.add_image_clip(ImageClipInput {
        bytes: b"to be deleted".to_vec(), mime: "image/png".into(),
        source_app: None, window_title: None,
    }).unwrap();
    let item_before = store.get_item(id).unwrap().unwrap();
    let blob_abs = tmp.path().join("blobs").join(item_before.blob_path.unwrap());
    assert!(blob_abs.exists());

    store.delete(id).unwrap();
    assert!(store.get_item(id).unwrap().is_none());
    assert!(!blob_abs.exists(), "delete should remove the blob file");

    std::env::remove_var("CLIPBOARD_DATA_DIR");
}

#[test]
fn list_with_pinned_only_returns_only_pinned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = tmp.path().join("t.db");
    let store = Store::open(&db, [0u8; 32]).unwrap();

    let pinned_id = store.add_clip(ClipInput {
        text: "pinned".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    let _unpinned_id = store.add_clip(ClipInput {
        text: "unpinned".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    store.pin(pinned_id, true).unwrap();

    let pinned = store.list_with(None, 100, 0, true).unwrap();
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].id, pinned_id);

    let all = store.list_with(None, 100, 0, false).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn prune_oldest_never_deletes_pinned_even_when_pinned_exceeds_keep() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = tmp.path().join("t.db");
    let store = Store::open(&db, [0u8; 32]).unwrap();

    // Pin 5 items, leave 5 unpinned. keep=2.
    let mut pinned_ids = Vec::new();
    for i in 0..5 {
        let id = store.add_clip(ClipInput {
            text: format!("pinned {}", i),
            primary_kind: "text".into(),
            kinds: vec!["text".into()],
            source_app: None, window_title: None,
        }).unwrap();
        store.pin(id, true).unwrap();
        pinned_ids.push(id);
    }
    for i in 0..5 {
        store.add_clip(ClipInput {
            text: format!("unpinned {}", i),
            primary_kind: "text".into(),
            kinds: vec!["text".into()],
            source_app: None, window_title: None,
        }).unwrap();
    }

    // Prune to keep=2. Today this would drop 3 pinned items (degenerate case).
    // After the fix, `keep` applies only to the unpinned pool (5 unpinned → keep
    // 2 newest → drop 3); pinned are excluded from the deletion candidates.
    let removed = store.prune_oldest(2).unwrap();
    assert_eq!(removed, 3, "expected exactly 3 unpinned items removed, got {}", removed);

    // All 5 pinned should still be there.
    for id in &pinned_ids {
        assert!(
            store.get_item(*id).unwrap().is_some(),
            "pinned item {} was deleted by prune_oldest",
            id
        );
    }
}

fn make_pinned_and_unpinned(store: &Store) -> (i64, i64) {
    let pinned_id = store.add_clip(ClipInput {
        text: "pinned".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    let unpinned_id = store.add_clip(ClipInput {
        text: "unpinned".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    store.pin(pinned_id, true).unwrap();
    (pinned_id, unpinned_id)
}

#[test]
fn clear_all_preserves_pinned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("t.db"), [0u8; 32]).unwrap();
    let (pinned_id, unpinned_id) = make_pinned_and_unpinned(&store);

    let removed = store.clear_all().unwrap();
    assert_eq!(removed, 1, "only the unpinned item should be removed");
    assert!(store.get_item(pinned_id).unwrap().is_some(), "pinned should survive");
    assert!(store.get_item(unpinned_id).unwrap().is_none(), "unpinned should be gone");
}

#[test]
fn clear_older_than_days_preserves_pinned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("t.db"), [0u8; 32]).unwrap();
    let (pinned_id, _unpinned_id) = make_pinned_and_unpinned(&store);

    // -1 days = future cutoff so EVERY clip is "older than" — only pinning saves the pinned one.
    let removed = store.clear_older_than_days(-1).unwrap();
    assert_eq!(removed, 1);
    assert!(store.get_item(pinned_id).unwrap().is_some());
}

#[test]
fn clear_kind_preserves_pinned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("t.db"), [0u8; 32]).unwrap();
    let (pinned_id, _unpinned_id) = make_pinned_and_unpinned(&store);

    let removed = store.clear_kind("text").unwrap();
    assert_eq!(removed, 1, "only the unpinned text item should be removed");
    assert!(store.get_item(pinned_id).unwrap().is_some());
}

#[test]
fn clear_kind_preserves_pinned_via_kinds_table_match() {
    // Locks down the SQL precedence: clear_kind's WHERE is_pinned = 0 AND
    // (primary_kind = ?1 OR id IN (kinds subquery)) needs the parens —
    // without them, the kinds-table branch would bypass the pin guard.
    // This test creates a pinned clip whose primary_kind is "url" but
    // whose kinds table also lists "text"; calling clear_kind("text")
    // matches the right branch of the OR. The pin guard must still hold.
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("t.db"), [0u8; 32]).unwrap();

    let pinned_id = store.add_clip(ClipInput {
        text: "https://example.com".into(),
        primary_kind: "url".into(),
        kinds: vec!["url".into(), "text".into()],  // both kinds
        source_app: None, window_title: None,
    }).unwrap();
    let _unpinned_id = store.add_clip(ClipInput {
        text: "plain text".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    store.pin(pinned_id, true).unwrap();

    // clear_kind("text") matches the unpinned clip via primary_kind (left
    // branch) AND the pinned clip via kinds table (right branch).
    // With pinned guard via parens, only the unpinned should be removed.
    let removed = store.clear_kind("text").unwrap();
    assert_eq!(removed, 1, "only the unpinned text clip should be removed");
    assert!(
        store.get_item(pinned_id).unwrap().is_some(),
        "pinned clip with kinds-table 'text' must survive clear_kind('text')"
    );
}
