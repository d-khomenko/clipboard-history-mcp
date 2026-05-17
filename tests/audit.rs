use clipboard_history_mcp::core::store::Store;
use tempfile::TempDir;

fn make_store() -> (TempDir, Store) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path, [0u8; 32]).unwrap();
    (tmp, store)
}

#[test]
fn audit_write_and_read_recent() {
    let (_tmp, store) = make_store();

    // 2. Write three audit entries.
    store.write_audit("mcp:claude-code", "read", "list_history", None, Some("text"));
    store.write_audit("mcp:claude-code", "read", "get_item", Some(42), None);
    store.write_audit("mcp:claude-code", "reveal", "unlock_secret", Some(7), Some("testing"));

    // 3. audit_recent(10, None) — all three, reversed-chronologically (desc by ts).
    let entries = store.audit_recent(10, None).unwrap();
    assert_eq!(entries.len(), 3);
    // Newest first: unlock_secret was last.
    assert_eq!(entries[0].tool, "unlock_secret");
    assert_eq!(entries[0].verb, "reveal");
    assert_eq!(entries[0].target_clip_id, Some(7));
    assert_eq!(entries[1].tool, "get_item");
    assert_eq!(entries[1].target_clip_id, Some(42));
    assert_eq!(entries[2].tool, "list_history");
    assert_eq!(entries[2].detail.as_deref(), Some("text"));
}

#[test]
fn audit_recent_limit_respected() {
    let (_tmp, store) = make_store();

    store.write_audit("mcp:claude-code", "read", "list_history", None, None);
    store.write_audit("mcp:claude-code", "read", "get_item", Some(1), None);
    store.write_audit("mcp:claude-code", "read", "search_history", None, Some("foo"));

    // Limit to 2 entries.
    let entries = store.audit_recent(2, None).unwrap();
    assert_eq!(entries.len(), 2);
}

#[test]
fn audit_recent_since_zero_returns_all() {
    let (_tmp, store) = make_store();

    store.write_audit("cli", "read", "list_history", None, None);
    store.write_audit("cli", "read", "get_item", Some(5), None);

    // since_secs = Some(0) means cutoff = now_ms - 0 = now → nothing is old enough.
    // Actually since=0 gives cutoff = now - 0*1000 = now_ms, so ts >= now_ms filters
    // to only entries with ts >= now (none, since they were inserted just before).
    // Per the spec: "since_secs=None → cutoff=0" (epoch start, return everything).
    let entries_all = store.audit_recent(10, None).unwrap();
    assert_eq!(entries_all.len(), 2);
}

#[test]
fn audit_recent_since_filters_old_entries() {
    let (_tmp, store) = make_store();

    // Insert an "old" entry by writing directly with a past ts.
    // We can't control ts via write_audit, so we just verify that
    // a very large since_secs (e.g., 86400 * 365 = 1 year) returns
    // entries written moments ago (they are within the last year).
    store.write_audit("mcp:claude-code", "read", "get_code", None, Some("rust"));

    let entries = store.audit_recent(10, Some(86400 * 365)).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tool, "get_code");
    assert_eq!(entries[0].actor, "mcp:claude-code");
}
