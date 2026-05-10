use clipboard_history_mcp::core::store::{Store, ClipInput, SecretInput};
use tempfile::TempDir;

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
