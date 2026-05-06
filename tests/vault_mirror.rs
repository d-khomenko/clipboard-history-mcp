//! Integration tests for the Obsidian vault mirror.
//!
//! Tests use a `tempfile::TempDir` as the vault root and exercise the public
//! `VaultMirror::write` API. Internal helpers are unit-tested inside
//! `src/core/vault.rs`.

use chrono::TimeZone;
use clipboard_history_mcp::core::vault::{MirrorItem, VaultMirror};

fn captured() -> chrono::DateTime<chrono::Local> {
    chrono::Local
        .with_ymd_and_hms(2026, 5, 6, 4, 32, 3)
        .single()
        .unwrap()
}

#[test]
fn sidecar_url_clip_round_trip() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mirror = VaultMirror::new(tmp.path().to_path_buf());

    let item = MirrorItem {
        id: 142,
        primary_kind: "url",
        source_app: Some("Safari"),
        window_title: Some("API Keys — OpenAI Platform"),
        text: "https://platform.openai.com/api-keys",
        captured: captured(),
    };
    mirror.write(&item).unwrap();

    let path = tmp
        .path()
        .join("clipboard/2026-05/142-url-https-platform-openai-com-api-keys.md");
    let body = std::fs::read_to_string(&path).expect("sidecar exists");

    assert!(body.contains("id: 142"));
    assert!(body.contains("kind: url"));
    assert!(body.contains("source: Safari"));
    assert!(body.contains("https://platform.openai.com/api-keys"));
}

#[test]
fn sidecar_code_clip_is_fenced() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mirror = VaultMirror::new(tmp.path().to_path_buf());

    let item = MirrorItem {
        id: 144,
        primary_kind: "code:python",
        source_app: Some("Cursor"),
        window_title: None,
        text: "def authenticate(token: str):\n    pass",
        captured: captured(),
    };
    mirror.write(&item).unwrap();

    let entries: Vec<_> = std::fs::read_dir(tmp.path().join("clipboard/2026-05"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    assert_eq!(entries.len(), 1);
    let body =
        std::fs::read_to_string(tmp.path().join("clipboard/2026-05").join(&entries[0])).unwrap();
    assert!(body.contains("```python\ndef authenticate"));
    assert!(body.contains("\n```\n"));
}

#[test]
fn lazy_dir_creation_under_nonexistent_root() {
    // Vault parent doesn't exist at install time; write should still succeed.
    let tmp = tempfile::TempDir::new().unwrap();
    let nonexistent = tmp.path().join("not/yet/created/vault");
    let mirror = VaultMirror::new(nonexistent.clone());

    let item = MirrorItem {
        id: 1,
        primary_kind: "text",
        source_app: None,
        window_title: None,
        text: "x",
        captured: captured(),
    };
    mirror.write(&item).unwrap();
    assert!(nonexistent.join("clipboard/2026-05").is_dir());
}

#[test]
fn writes_sidecar_and_daily_for_same_clip() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mirror = VaultMirror::new(tmp.path().to_path_buf());

    let item = MirrorItem {
        id: 142,
        primary_kind: "url",
        source_app: Some("Safari"),
        window_title: None,
        text: "https://example.com",
        captured: captured(),
    };
    mirror.write(&item).unwrap();

    assert!(tmp
        .path()
        .join("clipboard/2026-05/142-url-https-example-com.md")
        .exists());
    let daily = std::fs::read_to_string(tmp.path().join("daily/2026-05-06.md")).unwrap();
    assert!(daily.contains("## Clipboard captures · 2026-05-06"));
    assert!(daily.contains("[[142-url-https-example-com|url from Safari]]"));
}

#[test]
fn second_clip_appends_bullet_to_same_daily() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mirror = VaultMirror::new(tmp.path().to_path_buf());

    let mut item = MirrorItem {
        id: 1,
        primary_kind: "url",
        source_app: Some("Safari"),
        window_title: None,
        text: "https://a.example",
        captured: captured(),
    };
    mirror.write(&item).unwrap();
    item.id = 2;
    item.text = "https://b.example";
    mirror.write(&item).unwrap();

    let daily = std::fs::read_to_string(tmp.path().join("daily/2026-05-06.md")).unwrap();
    assert!(daily.contains("[[1-url-https-a-example|url from Safari]]"));
    assert!(daily.contains("[[2-url-https-b-example|url from Safari]]"));
    // Only ONE captures header
    assert_eq!(daily.matches("## Clipboard captures").count(), 1);
}

#[test]
fn store_then_mirror_writes_files() {
    use clipboard_history_mcp::core::store::{ClipInput, Store};

    let tmp_db = tempfile::TempDir::new().unwrap();
    let tmp_vault = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp_db.path().join("t.db"), [0u8; 32]).unwrap();
    let mirror = VaultMirror::new(tmp_vault.path().to_path_buf());

    let id = store
        .add_clip(ClipInput {
            text: "https://example.com".into(),
            primary_kind: "url".into(),
            kinds: vec!["url".into()],
            source_app: Some("Safari".into()),
            window_title: None,
        })
        .unwrap();

    // Mirror with the same item shape the watcher hook builds:
    let item = MirrorItem {
        id,
        primary_kind: "url",
        source_app: Some("Safari"),
        window_title: None,
        text: "https://example.com",
        captured: captured(),
    };
    mirror.write(&item).unwrap();

    assert!(tmp_vault.path().join("clipboard/2026-05").is_dir());
    assert!(tmp_vault.path().join("daily/2026-05-06.md").exists());
}
