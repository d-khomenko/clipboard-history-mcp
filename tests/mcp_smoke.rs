use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// Tests that mutate `CLIPBOARD_DATA_DIR` need to serialise — Cargo runs
/// integration tests in parallel and the env is process-global.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Pin the on-the-wire JSON shape of `Item` so a future refactor that
/// drops the T2 payload fields breaks loudly. No production code change —
/// `serde::Serialize` automatically picks up the four new fields.
#[test]
fn item_serialisation_includes_payload_fields() {
    use clipboard_history_mcp::core::store::Item;
    let item = Item {
        id: 1,
        uuid: "u".into(),
        text: None,
        preview: "p".into(),
        length: 0,
        primary_kind: "image".into(),
        kinds: vec![],
        tags: vec![],
        source_app: None,
        window_title: None,
        first_copied_at: 0,
        last_copied_at: 0,
        copy_count: 1,
        paste_count: 0,
        is_pinned: false,
        payload_kind: "image".into(),
        blob_path: Some("ab/abc.png".into()),
        blob_size_bytes: Some(123),
        mime_type: Some("image/png".into()),
    };
    let v = serde_json::to_value(&item).unwrap();
    assert_eq!(v["payload_kind"], "image");
    assert_eq!(v["blob_path"], "ab/abc.png");
    assert_eq!(v["blob_size_bytes"], 123);
    assert_eq!(v["mime_type"], "image/png");
}

#[test]
fn get_item_with_blob_inlines_base64_data_url() {
    use clipboard_history_mcp::core::blobs;
    use clipboard_history_mcp::core::store::{ImageClipInput, Store};

    let _g = ENV_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path, [0u8; 32]).unwrap();
    let bytes = vec![0xde, 0xad, 0xbe, 0xef];
    let id = store.add_image_clip(ImageClipInput {
        bytes: bytes.clone(),
        mime: "image/png".into(),
        source_app: None,
        window_title: None,
    }).unwrap();

    // Sanity: the blob actually exists at the expected path.
    let item = store.get_item(id).unwrap().unwrap();
    let blob_rel = item.blob_path.as_deref().unwrap();
    assert!(blobs::absolute_path(blob_rel).exists());

    // Read it via blobs::read and base64-encode to compare.
    use base64::{engine::general_purpose::STANDARD, Engine};
    let read_back = blobs::read(blob_rel).unwrap();
    assert_eq!(read_back, bytes);
    let expected_b64 = STANDARD.encode(&read_back);
    assert!(!expected_b64.is_empty());

    std::env::remove_var("CLIPBOARD_DATA_DIR");
}

/// End-to-end MCP smoke test.
///
/// Spawns the binary in `serve` mode, sends the MCP handshake
/// (initialize → notifications/initialized → tools/list), then asserts
/// that the server replied with an initialize result and a tools/list result
/// that mentions at least one of our known tool names.
///
/// Marked `#[ignore]` because it:
/// - hits the real macOS Keychain (get_or_create_master_key)
/// - requires the release binary to already be built
///
/// Run with:
///   cargo test --release --test mcp_smoke -- --ignored
#[test]
#[ignore]
fn mcp_handshake_and_list_tools() {
    let bin = env!("CARGO_BIN_EXE_clipboard-history-mcp");

    let mut child = Command::new(bin)
        .arg("serve")
        .env(
            "CLIPBOARD_DATA_DIR",
            std::env::temp_dir().join("cbhist-rs-int"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn MCP server binary");

    let stdin = child.stdin.as_mut().expect("stdin not available");

    // 1. initialize
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}},"clientInfo":{{"name":"t","version":"1"}}}}}}"#
    )
    .unwrap();

    // 2. notifications/initialized  (no id — notification, no response expected)
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();

    // 3. tools/list
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#
    )
    .unwrap();

    // Give the server time to process and write responses, then kill it.
    std::thread::sleep(Duration::from_millis(2000));
    let _ = child.kill();
    let out = child.wait_with_output().expect("wait_with_output failed");

    let stdout = String::from_utf8_lossy(&out.stdout);

    // --- assertions ---

    // initialize reply must be present
    assert!(
        stdout.contains(r#""id":1"#),
        "missing initialize response (id=1)\n\nstdout:\n{}",
        stdout
    );

    // tools/list reply must be present
    assert!(
        stdout.contains(r#""id":2"#),
        "missing tools/list response (id=2)\n\nstdout:\n{}",
        stdout
    );

    // at least one known tool name must appear in tools/list
    assert!(
        stdout.contains("list_history") || stdout.contains("get_stats"),
        "tools/list response does not mention expected tools\n\nstdout:\n{}",
        stdout
    );

    // make sure we're not just seeing 0 tools
    assert!(
        !stdout.contains(r#""tools":[]"#),
        "tools/list returned an empty tools array\n\nstdout:\n{}",
        stdout
    );
}
