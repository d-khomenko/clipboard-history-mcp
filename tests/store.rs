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
