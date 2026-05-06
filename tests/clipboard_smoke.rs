#[test]
#[ignore] // touches real system clipboard
fn arboard_round_trip() {
    use clipboard_history_mcp::core::pasteboard::{read_clipboard, write_clipboard};

    let needle = format!("v4-smoke-{}", std::process::id());
    write_clipboard(&needle).unwrap();
    let got = read_clipboard().unwrap();
    assert_eq!(got, needle);
}

#[test]
#[ignore] // touches real system clipboard + spawns watcher thread
fn watcher_captures_a_clip() {
    use clipboard_history_mcp::core::store::Store;
    use clipboard_history_mcp::daemon::watcher::{run_watcher, WatcherOptions};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let key = [99u8; 32];

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let db_path_clone = db_path.clone();
    let opts = WatcherOptions {
        poll_ms: 200,
        max_items: 50,
        ..Default::default()
    };
    let watcher_handle = thread::spawn(move || run_watcher(db_path_clone, key, opts, stop_clone));

    thread::sleep(Duration::from_millis(300));

    let needle = format!("watcher-smoke-{}", std::process::id());
    clipboard_history_mcp::core::pasteboard::write_clipboard(&needle).unwrap();

    thread::sleep(Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);
    let _ = watcher_handle.join();

    let store = Store::open(&db_path, key).unwrap();
    let items = store.list(50).unwrap();
    assert!(
        items.iter().any(|i| i.text.as_deref() == Some(needle.as_str())),
        "expected to find {needle} in DB; got {} items",
        items.len()
    );
}
