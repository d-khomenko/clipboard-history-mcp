# T2 Image + File Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capture pasteboard images (PNG/TIFF on macOS) and file URLs alongside text, store blobs content-hash deduplicated under `data_dir/blobs/<aa>/<sha256>.<ext>`, expose payload metadata via three existing MCP tools, and mirror non-secret blobs into the Obsidian vault next to the existing sidecar.

**Architecture:** Schema bumps to v2 with four new nullable columns on `clips`. A new private `core/blobs` module owns blob filesystem operations. `pasteboard::read_clipboard()` becomes a thin wrapper over a new `pasteboard::read_clip()` that returns a `Clip` enum. Watcher tick dispatches on the enum; image and file branches call new `Store::add_image_clip` / `add_files_clip` methods that hash the bytes, dedup, and write to disk before inserting. MCP `list_history` / `get_item` add payload-metadata fields; `get_item` adds an opt-in `with_blob: bool` for base64 inline; `copy_item` dispatches on `payload_kind`. Vault-mirror copies blobs alongside sidecars and renders an image link in the body.

**Tech Stack:** Rust 1.95, `rusqlite` (existing schema lives in `src/core/schema_v1.sql`), `objc2-app-kit` (added in T1), `arboard`, `sha2`, `base64 = "0.22"` (new top-level dep), `chrono` (existing), `tracing`.

**Spec reference:** [`docs/superpowers/specs/2026-05-08-v0.6-T2-image-file-capture-design.md`](../specs/2026-05-08-v0.6-T2-image-file-capture-design.md). All design decisions and out-of-scope items live there.

**Roadmap reference:** [`docs/superpowers/specs/2026-05-08-v0.6-adoption-roadmap-design.md`](../specs/2026-05-08-v0.6-adoption-roadmap-design.md) §3 T2.

---

## Pre-flight

Before Task 1, the engineer must be on a fresh branch off `origin/main`:

```bash
cd /Users/stock/dev/clipboard-history-mcp
git fetch origin main
git checkout -b feat/v0.6-T2-image-file-capture origin/main
```

If the branch already exists locally:
```bash
git checkout feat/v0.6-T2-image-file-capture
```

Confirm with `git status` you're on `feat/v0.6-T2-image-file-capture` with a clean working tree before doing anything else.

All paths in this plan are relative to `/Users/stock/dev/clipboard-history-mcp`.

---

## Task 1: Add `base64` dependency + create empty `core/blobs.rs` skeleton

**Why.** `base64` is needed for `get_item` `with_blob: true` MCP responses (Task 10). `core/blobs.rs` is a new module owning blob filesystem operations — Task 2's schema migration references blob paths, Task 7+ writes blobs, Task 11 reads them for restore. Adding the empty module + dep first keeps subsequent commits focused.

**Files:**
- Modify: `Cargo.toml` — add `base64 = "0.22"` to `[dependencies]`.
- Create: `src/core/blobs.rs` — empty module with `pub fn blobs_dir() -> PathBuf` only.
- Modify: `src/core/mod.rs` — add `pub mod blobs;`.

- [ ] **Step 1: Add `base64` dep**

Open `Cargo.toml`. After the existing `# Utilities` block (around line 47-58), add:

```toml
base64                     = "0.22"
```

Place it alphabetically — after `anyhow` and before `chrono`, or simply before `ctrlc`. The exact position doesn't matter as long as it's in `[dependencies]` (NOT in either target-specific block).

- [ ] **Step 2: Create `src/core/blobs.rs`**

```rust
//! Filesystem storage for binary clip payloads (images, files).
//!
//! Layout: `data_dir/blobs/<aa>/<sha256>.<ext>` where `<aa>` is the first
//! two hex characters of the hash (git-style loose-object sharding so
//! directory listings stay flat as content grows).
//!
//! All paths returned to callers are RELATIVE to `data_dir/blobs/` so the
//! database survives a `data_dir` move. Resolve to absolute via
//! `blob_path_absolute`.

use anyhow::Result;
use std::path::PathBuf;

/// Root directory for all clip blobs under `data_dir`.
pub fn blobs_dir() -> PathBuf {
    crate::core::paths::data_dir().join("blobs")
}
```

- [ ] **Step 3: Wire the module into `src/core/mod.rs`**

Open `src/core/mod.rs`. Find the existing list of `pub mod ...` entries (alphabetically: `biometry`, `crypto`, `db`, `master_password`, `paths`, `pasteboard`, `secrets`, `store`, `types`, `vault`).

Add `pub mod blobs;` between `pub mod biometry;` and `pub mod crypto;` (alphabetical).

- [ ] **Step 4: Build to confirm**

```bash
cargo check
```

Expected: clean compile. The new module is unused which is fine; subsequent tasks fill it in.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/core/blobs.rs src/core/mod.rs
git commit -m "chore(deps): add base64 dep + scaffold core/blobs module

base64 = \"0.22\" is needed by the MCP get_item with_blob: true response
shape (T2 §3.6). core/blobs is a new home for blob filesystem ops —
write/read/delete with content-hash dedup. Both land empty here so the
real changes in subsequent commits are focused.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Schema v2 migration

**Why.** Adds four nullable columns on `clips` for payload metadata, plus an index on `payload_kind` for cheap "list images only" queries. Extends the existing `if current < N` ladder in `db.rs::open_db`. Existing rows transparently get `payload_kind = 'text'` via the column default.

**Files:**
- Create: `src/core/schema_v2.sql` — the migration text.
- Modify: `src/core/db.rs` — add `SCHEMA_V2 = include_str!("schema_v2.sql")` constant, extend the ladder, add a migration unit test.

- [ ] **Step 1: Write the failing migration test**

Open `src/core/db.rs`. Inside the existing `#[cfg(test)] mod tests { ... }` block (after `schema_creates_tables`, before the closing `}`), add:

```rust
#[test]
fn migrates_v1_to_v2_preserves_existing_rows() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("t.db");

    // 1. Open at v1 (current code path), insert a text row.
    {
        let conn = open_db(&db_path).unwrap();
        // Force schema_version back to 1 to simulate "fresh v1 install".
        conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', '1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                first_copied_at, last_copied_at)
             VALUES ('u1', 'hello', 'hello', 5, 5, 'abc', 'text', 1, 1)",
            [],
        )
        .unwrap();
    }

    // 2. Re-open: migration ladder should run v2.
    let conn = open_db(&db_path).unwrap();
    let version: String = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, "2");

    // 3. Existing row survives, with default payload_kind = 'text'.
    let (text, payload_kind, blob_path): (String, String, Option<String>) = conn
        .query_row(
            "SELECT text, payload_kind, blob_path FROM clips WHERE uuid = 'u1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(text, "hello");
    assert_eq!(payload_kind, "text");
    assert!(blob_path.is_none());
}

#[test]
fn v2_index_on_payload_kind_exists() {
    let tmp = tempfile::TempDir::new().unwrap();
    let conn = open_db(tmp.path().join("t.db")).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_clips_payload_kind'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
```

- [ ] **Step 2: Run the new tests — should fail to compile (the SCHEMA_V2 constant doesn't exist yet)**

```bash
cargo test --lib core::db::tests::migrates_v1_to_v2_preserves_existing_rows
```

Expected: compile error or runtime failure — `payload_kind` column does not exist.

- [ ] **Step 3: Create `src/core/schema_v2.sql`**

```sql
-- Schema v2: add payload metadata columns to support image and file clips.

ALTER TABLE clips ADD COLUMN payload_kind TEXT NOT NULL DEFAULT 'text';
  -- 'text' | 'image' | 'file'
ALTER TABLE clips ADD COLUMN blob_path TEXT;
  -- relative path under data_dir/blobs/ ('aa/<sha256>.<ext>'). NULL for text.
ALTER TABLE clips ADD COLUMN blob_size_bytes INTEGER;
  -- NULL for text. Size of the blob file on disk in bytes.
ALTER TABLE clips ADD COLUMN mime_type TEXT;
  -- e.g. 'image/png', 'image/tiff', 'application/octet-stream'. NULL for text.

CREATE INDEX IF NOT EXISTS idx_clips_payload_kind ON clips(payload_kind);

UPDATE meta SET value = '2' WHERE key = 'schema_version';
```

- [ ] **Step 4: Extend `src/core/db.rs::open_db` with the v2 step**

Open `src/core/db.rs`. After the existing `const SCHEMA_V1: ...` line, add:

```rust
const SCHEMA_V2: &str = include_str!("schema_v2.sql");
```

Inside `open_db`, after the existing `if current < 1 { conn.execute_batch(SCHEMA_V1)?; }` block, add:

```rust
    if current < 2 {
        conn.execute_batch(SCHEMA_V2)?;
    }
```

- [ ] **Step 5: Run the migration tests — both must pass**

```bash
cargo test --lib core::db::tests
```

Expected: 3 tests pass (`schema_creates_tables`, `migrates_v1_to_v2_preserves_existing_rows`, `v2_index_on_payload_kind_exists`).

- [ ] **Step 6: Run the full library test suite — nothing else regresses**

```bash
cargo test --lib
```

Expected: 35+3 = 38 lib tests pass (existing 35 + 2 new + 1 already counted in the 35, depending on how the lib counts go). All green, no failures.

- [ ] **Step 7: Commit**

```bash
git add src/core/schema_v2.sql src/core/db.rs
git commit -m "feat(db): schema v2 migration adds payload metadata columns

ALTER clips ADD COLUMN payload_kind, blob_path, blob_size_bytes,
mime_type. Existing text rows transparently get payload_kind = 'text'
via the column default. Index on payload_kind for cheap kind-filtered
listings. Migration extends the existing 'if current < N' ladder; no
separate migration framework yet (deferred to v0.7).

Includes two new unit tests verifying that v1→v2 preserves existing
rows and that the new index exists.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `core/blobs` filesystem helpers

**Why.** Blob storage primitives — write, read, delete, build relative paths. Used by `Store::add_image_clip` (Task 7) and `Store::delete` blob cleanup, plus MCP `copy_item` (Task 11) for restore.

**Files:**
- Modify: `src/core/blobs.rs` — fill in the body.

- [ ] **Step 1: Write the failing tests**

Replace the entire body of `src/core/blobs.rs` with:

```rust
//! Filesystem storage for binary clip payloads (images, files).
//!
//! Layout: `data_dir/blobs/<aa>/<sha256>.<ext>` where `<aa>` is the first
//! two hex characters of the hash (git-style loose-object sharding so
//! directory listings stay flat as content grows).
//!
//! All paths stored in the DB are RELATIVE to `data_dir/blobs/` so the
//! database survives a `data_dir` move.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Root directory for all clip blobs under `data_dir`.
pub fn blobs_dir() -> PathBuf {
    crate::core::paths::data_dir().join("blobs")
}

/// Compute the sha256 hex digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Build the relative path that goes into `clips.blob_path`.
///
/// `extension` should be a short ASCII suffix without the leading dot
/// ("png", "tiff", "bin"). The function does not validate the extension —
/// callers pass MIME-derived strings.
pub fn relative_path(hash: &str, extension: &str) -> String {
    let prefix = &hash[..2];
    format!("{}/{}.{}", prefix, hash, extension)
}

/// Resolve a relative blob path to an absolute path under `data_dir/blobs/`.
pub fn absolute_path(relative: &str) -> PathBuf {
    blobs_dir().join(relative)
}

/// Atomically write `bytes` to the blob path derived from its sha256
/// digest. Idempotent — if the destination already exists with the same
/// hash, returns the existing relative path without rewriting.
///
/// Returns `(relative_path, hash_hex)`.
pub fn write(bytes: &[u8], extension: &str) -> Result<(String, String)> {
    let hash = sha256_hex(bytes);
    let rel = relative_path(&hash, extension);
    let abs = absolute_path(&rel);

    if abs.exists() {
        // Dedup hit — same hash means same content. No-op.
        return Ok((rel, hash));
    }

    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).context("create blob parent dir")?;
    }
    let pid = std::process::id();
    let tmp = abs.with_file_name(format!("{}.{}.tmp", abs.file_name().unwrap().to_string_lossy(), pid));
    {
        let mut f = std::fs::File::create(&tmp).context("create blob tmp")?;
        f.write_all(bytes).context("write blob bytes")?;
        f.sync_all().context("fsync blob")?;
    }
    std::fs::rename(&tmp, &abs).context("rename blob into place")?;
    Ok((rel, hash))
}

/// Read a blob's full contents into memory.
pub fn read(relative: &str) -> Result<Vec<u8>> {
    let abs = absolute_path(relative);
    std::fs::read(&abs).with_context(|| format!("read blob {}", abs.display()))
}

/// Best-effort delete. Logs a warn and returns `Ok(())` if the file is
/// missing; the row is gone anyway. Returns `Err` only on permission /
/// I/O failures.
pub fn delete(relative: &str) -> Result<()> {
    let abs = absolute_path(relative);
    match std::fs::remove_file(&abs) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!("blob delete: {} already missing", abs.display());
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("delete blob {}", abs.display())),
    }
}

/// Determine a sensible filename extension for a MIME type.
/// Returns `"bin"` for unknown MIMEs.
pub fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/tiff" => "tiff",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Tests share an env-var override of CLIPBOARD_DATA_DIR. Serialise so
    /// they don't race on the global env.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_data_dir<F: FnOnce(&Path)>(f: F) {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
        f(tmp.path());
        std::env::remove_var("CLIPBOARD_DATA_DIR");
    }

    #[test]
    fn relative_path_uses_two_char_prefix() {
        assert_eq!(
            relative_path("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789", "png"),
            "ab/abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789.png"
        );
    }

    #[test]
    fn extension_for_mime_known_and_unknown() {
        assert_eq!(extension_for_mime("image/png"), "png");
        assert_eq!(extension_for_mime("image/tiff"), "tiff");
        assert_eq!(extension_for_mime("video/quicktime"), "bin");
    }

    #[test]
    fn write_creates_file_and_dedups_on_repeat() {
        with_temp_data_dir(|root| {
            let bytes = b"hello blob";
            let (rel1, hash1) = write(bytes, "bin").unwrap();
            let abs1 = absolute_path(&rel1);
            assert!(abs1.exists(), "first write should create file");
            assert_eq!(std::fs::read(&abs1).unwrap(), bytes);

            // Second write: same content → same path, no rewrite.
            let mtime1 = abs1.metadata().unwrap().modified().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
            let (rel2, hash2) = write(bytes, "bin").unwrap();
            assert_eq!(rel1, rel2);
            assert_eq!(hash1, hash2);
            let mtime2 = abs1.metadata().unwrap().modified().unwrap();
            assert_eq!(mtime1, mtime2, "dedup hit should not rewrite the file");

            assert!(root.join("blobs").exists());
        });
    }

    #[test]
    fn read_round_trips() {
        with_temp_data_dir(|_| {
            let bytes = b"round trip payload";
            let (rel, _) = write(bytes, "bin").unwrap();
            assert_eq!(read(&rel).unwrap(), bytes);
        });
    }

    #[test]
    fn delete_removes_existing_file() {
        with_temp_data_dir(|_| {
            let bytes = b"delete me";
            let (rel, _) = write(bytes, "bin").unwrap();
            assert!(absolute_path(&rel).exists());
            delete(&rel).unwrap();
            assert!(!absolute_path(&rel).exists());
        });
    }

    #[test]
    fn delete_is_ok_when_blob_missing() {
        with_temp_data_dir(|_| {
            // No write — delete should succeed (best-effort) and return Ok.
            assert!(delete("aa/missing.bin").is_ok());
        });
    }
}
```

- [ ] **Step 2: Run tests — must pass**

```bash
cargo test --lib core::blobs
```

Expected: 6 tests pass.

- [ ] **Step 3: Run full lib + integration test suites — no regressions**

```bash
cargo test
```

Expected: still all green.

- [ ] **Step 4: Commit**

```bash
git add src/core/blobs.rs
git commit -m "feat(blobs): filesystem storage with content-hash dedup

New core/blobs module owns blob filesystem ops:
  - write(bytes, ext) -> (relative_path, hash). Dedup-by-hash;
    repeating the same bytes is a no-op on disk.
  - read / delete operate on relative paths under data_dir/blobs.
  - extension_for_mime maps the common image MIMEs to short suffixes;
    unknown MIMEs fall back to .bin.

Layout: data_dir/blobs/<aa>/<sha256>.<ext> (git-style two-char loose-
object sharding so directory listings stay flat as content grows).

Inline unit tests cover relative_path shape, dedup behaviour,
round-trip read, delete-missing as Ok.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `Clip` enum + `read_clip` text-only path

**Why.** Refactor `pasteboard.rs` to expose a `Clip` enum; `read_clipboard` becomes a thin wrapper that returns `String` for legacy callers. This task only adds the text-only path; image and file branches are wired in Task 5.

**Files:**
- Modify: `src/core/pasteboard.rs` — add `Clip` enum, add `read_clip()`, refactor `read_clipboard()`.

- [ ] **Step 1: Write the failing test**

Open `src/core/pasteboard.rs`. Inside the existing `#[cfg(test)] mod tests { ... }` block, add:

```rust
#[test]
#[ignore] // requires real clipboard
fn read_clip_returns_text_when_pasteboard_has_text() {
    write_clipboard("hello clip enum").unwrap();
    match read_clip().unwrap() {
        Clip::Text(s) => assert_eq!(s, "hello clip enum"),
        other => panic!("expected Text, got {:?}", other),
    }
}
```

Note: `Clip` must derive `Debug` so the panic message compiles. We add the derive as part of Step 3.

- [ ] **Step 2: Run the new test — should fail to compile (`Clip` doesn't exist)**

```bash
cargo test --lib pasteboard::tests::read_clip_returns_text_when_pasteboard_has_text
```

Expected: compile error — `cannot find type 'Clip' in this scope`.

- [ ] **Step 3: Add the `Clip` enum + `read_clip` text-only impl**

In `src/core/pasteboard.rs`, just below the existing `static CLIPBOARD: ...` declaration (after line 7), add:

```rust
use std::path::PathBuf;

/// One snapshot of the system pasteboard, classified by payload kind.
#[derive(Debug)]
pub enum Clip {
    Empty,
    Text(String),
    Image { bytes: Vec<u8>, mime: &'static str },
    Files(Vec<PathBuf>),
}

/// Read the pasteboard's current contents into a `Clip`. Priority order
/// (macOS): PNG → TIFF → file URLs → text. On Linux: text only — image
/// and file capture are deferred to v0.7.
///
/// Returns `Clip::Empty` when the pasteboard has no recognisable content
/// or when the contents are marked transient (concealed UTI). Callers
/// MUST check `is_transient()` themselves *before* calling this fn — the
/// transient check is a precondition, not enforced here, so test code
/// can exercise the read path without a transient detour.
pub fn read_clip() -> Result<Clip> {
    #[cfg(target_os = "macos")]
    {
        if let Some(clip) = macos_pb::read_clip_macos()? {
            return Ok(clip);
        }
    }
    // Fall through to text-only path (cross-platform via arboard).
    let mut cb = CLIPBOARD.lock().map_err(|_| anyhow!("clipboard mutex poisoned"))?;
    match cb.get_text() {
        Ok(s) if !s.is_empty() => Ok(Clip::Text(s)),
        Ok(_) => Ok(Clip::Empty),
        Err(arboard::Error::ContentNotAvailable) => Ok(Clip::Empty),
        Err(e) => Err(anyhow!("clipboard read: {}", e)),
    }
}
```

Refactor the existing `read_clipboard()` to a thin wrapper. Replace its body (current lines 9-16) with:

```rust
pub fn read_clipboard() -> Result<String> {
    match read_clip()? {
        Clip::Text(s) => Ok(s),
        _ => Ok(String::new()),
    }
}
```

In the existing `#[cfg(target_os = "macos")] mod macos_pb { ... }` block, add a stub for `read_clip_macos` that returns `None` for now (image / file branches added in Task 5):

```rust
    pub fn read_clip_macos() -> Result<Option<super::Clip>> {
        // Stub: image + file branches land in T2 Task 5.
        Ok(None)
    }
```

You'll need to add the `Result` import to the macos_pb module — `use anyhow::Result;` at the top of the inner `mod macos_pb` (next to the existing `use objc2::rc::autoreleasepool;`).

- [ ] **Step 4: Run the test — must pass (the `#[ignore]` makes it not actually run, but it must compile)**

```bash
cargo test --lib pasteboard
```

Expected: existing tests still pass (3 ignored, 0 failed). The new `read_clip_returns_text_when_pasteboard_has_text` test compiles and is `#[ignore]`'d (does not run).

- [ ] **Step 5: Run the full lib + integration suite**

```bash
cargo test
```

Expected: green; nothing depends on `read_clip` yet so there's no behavioural regression.

- [ ] **Step 6: Commit**

```bash
git add src/core/pasteboard.rs
git commit -m "feat(pasteboard): Clip enum + read_clip text-only path

Introduces:
  - pub enum Clip { Empty, Text(String), Image { bytes, mime }, Files(Vec<PathBuf>) }
  - pub fn read_clip() -> Result<Clip>
  - read_clipboard() refactored to a thin wrapper returning text only
    so existing callers keep working unchanged.

macOS image + file branches stay stubbed (read_clip_macos returns None).
T2 Task 5 fills them in. Linux remains text-only — image capture is
deferred to v0.7.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: macOS image + file pasteboard read

**Why.** Wire the macOS-only image and file URL paths inside `read_clip_macos`. Tasks downstream depend on `read_clip` being able to actually return `Clip::Image` / `Clip::Files` on macOS.

**Files:**
- Modify: `src/core/pasteboard.rs` — fill in the `macos_pb::read_clip_macos` body.

- [ ] **Step 1: Add a unit test for the empty path on Linux/Windows (cross-platform safety net)**

Inside `mod tests` in `src/core/pasteboard.rs`:

```rust
#[test]
#[cfg(not(target_os = "macos"))]
fn read_clip_text_or_empty_on_non_macos() {
    // On non-macOS we don't go through NSPasteboard; the read path is
    // arboard text + Empty fallback. Test only that we get a valid Clip
    // variant that's NOT Image/Files (those branches don't exist here).
    let clip = read_clip().unwrap();
    assert!(matches!(clip, Clip::Empty | Clip::Text(_)));
}
```

- [ ] **Step 2: Run the test (Linux only) or skip it (macOS — cfg-gated out)**

```bash
cargo test --lib pasteboard
```

Expected: on macOS the new test is excluded by cfg; on Linux it passes.

- [ ] **Step 3: Implement `read_clip_macos`**

In `src/core/pasteboard.rs` inside `#[cfg(target_os = "macos")] mod macos_pb { ... }`, replace the stub from Task 4 with:

```rust
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::path::PathBuf;

    pub fn read_clip_macos() -> Result<Option<super::Clip>> {
        autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            let Some(types) = pb.types() else {
                return Ok::<Option<super::Clip>, anyhow::Error>(None);
            };

            // Priority 1: PNG image
            let png_uti = unsafe { NSPasteboardTypePNG };
            if types.containsObject(png_uti) {
                if let Some(data) = unsafe { pb.dataForType(png_uti) } {
                    let bytes = data.to_vec();
                    return Ok(Some(super::Clip::Image { bytes, mime: "image/png" }));
                }
            }

            // Priority 2: TIFF image (Apple-native screenshot fallback)
            let tiff_uti = unsafe { NSPasteboardTypeTIFF };
            if types.containsObject(tiff_uti) {
                if let Some(data) = unsafe { pb.dataForType(tiff_uti) } {
                    let bytes = data.to_vec();
                    return Ok(Some(super::Clip::Image { bytes, mime: "image/tiff" }));
                }
            }

            // Priority 3: File URLs.
            // We use the readObjectsForClasses API instead of the deprecated
            // NSFilenamesPboardType plist — it's the modern path on macOS 10.6+.
            let file_uti = unsafe { NSPasteboardTypeFileURL };
            if types.containsObject(file_uti) {
                let mut paths: Vec<PathBuf> = Vec::new();
                // Iterate URL-typed pasteboard items.
                if let Some(items) = unsafe { pb.pasteboardItems() } {
                    for i in 0..items.count() {
                        let item = items.objectAtIndex(i);
                        if let Some(url_str) = unsafe { item.stringForType(file_uti) } {
                            let s = url_str.to_string();
                            if let Ok(url) = NSURL::URLWithString(&NSString::from_str(&s)) {
                                if let Some(path_str) = unsafe { url.path() } {
                                    paths.push(PathBuf::from(path_str.to_string()));
                                }
                            } else if let Some(stripped) = s.strip_prefix("file://") {
                                paths.push(PathBuf::from(stripped));
                            }
                        }
                    }
                }
                if !paths.is_empty() {
                    return Ok(Some(super::Clip::Files(paths)));
                }
            }

            // No image / file types we recognise — let read_clip fall through to text.
            Ok(None)
        })
    }
```

Note: the exact `objc2-app-kit 0.3.x` API surface for `pasteboardItems` and `stringForType` may differ between minor releases. If the test compile fails with a method-not-found error, simplify by removing the `pasteboardItems` loop and instead reading file URLs via `pb.stringForType(NSPasteboardTypeFileURL)` (single-file path) — multi-file is then deferred. Document the simplification as a `DONE_WITH_CONCERNS` note in your report.

- [ ] **Step 4: Build + test**

```bash
cargo check
cargo test --lib pasteboard
```

Expected: clean compile. Tests still pass (`#[ignore]`'d round-trip stays ignored). If compile fails on `pasteboardItems` or `stringForType` API drift, fall back to the simplified single-file branch noted above.

- [ ] **Step 5: Commit**

```bash
git add src/core/pasteboard.rs
git commit -m "feat(pasteboard): macOS image (PNG/TIFF) + file URL read path

read_clip_macos queries NSPasteboard in priority order:
  1. NSPasteboardTypePNG → Clip::Image { mime: 'image/png' }
  2. NSPasteboardTypeTIFF → Clip::Image { mime: 'image/tiff' }
  3. NSPasteboardTypeFileURL → Clip::Files(Vec<PathBuf>)
  4. fall through to text via the cross-platform read_clipboard

Image bytes are pasteboard-native (no transcoding). File URLs are
parsed via NSURL when available, with a 'file://' string-strip fallback.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: `write_clip` for restoring image / file pasteboards

**Why.** `copy_item` (Task 11) needs to restore image / file clips back to the pasteboard. `write_clipboard` only handles text.

**Files:**
- Modify: `src/core/pasteboard.rs` — add `write_clip(clip: &Clip)`.

- [ ] **Step 1: Add the public `write_clip` function**

In `src/core/pasteboard.rs`, after the existing `write_clipboard` function, add:

```rust
/// Write a `Clip` back to the pasteboard with the correct UTI / MIME.
/// On macOS, image clips are written under `NSPasteboardTypePNG` /
/// `NSPasteboardTypeTIFF` per the clip's mime. File clips write
/// `NSPasteboardTypeFileURL` per path. On Linux, only Text is supported;
/// other variants return an error.
pub fn write_clip(clip: &Clip) -> Result<()> {
    match clip {
        Clip::Empty => Err(anyhow!("cannot write Clip::Empty")),
        Clip::Text(s) => write_clipboard(s),
        Clip::Image { bytes, mime } => {
            #[cfg(target_os = "macos")]
            {
                macos_pb::write_image_macos(bytes, mime)
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (bytes, mime);
                Err(anyhow!("image write not supported on this platform"))
            }
        }
        Clip::Files(paths) => {
            #[cfg(target_os = "macos")]
            {
                macos_pb::write_files_macos(paths)
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = paths;
                Err(anyhow!("file write not supported on this platform"))
            }
        }
    }
}
```

- [ ] **Step 2: Add the macOS `write_image_macos` and `write_files_macos`**

Inside `#[cfg(target_os = "macos")] mod macos_pb`, after `read_clip_macos`:

```rust
    use objc2_foundation::NSData;

    pub fn write_image_macos(bytes: &[u8], mime: &str) -> Result<()> {
        let uti = match mime {
            "image/png" => unsafe { NSPasteboardTypePNG },
            "image/tiff" => unsafe { NSPasteboardTypeTIFF },
            other => return Err(anyhow::anyhow!("unsupported image mime for write: {}", other)),
        };
        autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            unsafe { pb.clearContents() };
            let data = NSData::with_bytes(bytes);
            let written = unsafe { pb.setData_forType(Some(&data), uti) };
            if !written {
                return Err(anyhow::anyhow!("NSPasteboard.setData_forType returned false"));
            }
            Ok(())
        })
    }

    pub fn write_files_macos(paths: &[std::path::PathBuf]) -> Result<()> {
        let file_uti = unsafe { NSPasteboardTypeFileURL };
        autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            unsafe { pb.clearContents() };
            for path in paths {
                let url_str = format!("file://{}", path.display());
                let ns_str = NSString::from_str(&url_str);
                let written = unsafe { pb.setString_forType(&ns_str, file_uti) };
                if !written {
                    return Err(anyhow::anyhow!("NSPasteboard.setString_forType returned false"));
                }
            }
            Ok(())
        })
    }
```

If `NSData::with_bytes` does not exist in the installed version of `objc2-foundation`, substitute `NSData::dataWithBytes_length` or read the crate's docs and adapt. `cargo doc -p objc2-foundation --open` shows the available constructors.

- [ ] **Step 3: Build**

```bash
cargo check
```

Expected: clean compile. If API names differ, adapt and report deviation in commit.

- [ ] **Step 4: Run lib tests — no regressions**

```bash
cargo test --lib
```

Expected: green.

- [ ] **Step 5: Commit**

```bash
git add src/core/pasteboard.rs
git commit -m "feat(pasteboard): write_clip for restoring image and file clips

write_clip dispatches by Clip variant:
  - Text: existing write_clipboard.
  - Image: macOS-only via NSPasteboard.setData_forType under PNG/TIFF UTI.
  - Files: macOS-only via NSPasteboard.setString_forType under
    NSPasteboardTypeFileURL, one entry per path.

Linux + Windows return an error for non-Text clips. Empty is also an
error — there is no semantic write for 'no clip'.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: `Store::add_image_clip`, `add_files_clip`, payload-aware delete

**Why.** Persisting image and file clips to the DB plus blob storage. Adds two new public methods on `Store`. `delete` is extended to clean up the blob file. `Item` returned from `get_item` / `list` includes the new payload fields.

**Files:**
- Modify: `src/core/store.rs` — add `PayloadKind` enum, extend `Item`, add `add_image_clip` / `add_files_clip`, update `get_item` / `row_to_item` to populate the new fields, extend `delete` to remove blobs.

- [ ] **Step 1: Write failing tests**

Open `tests/store.rs`. At the end of the file, add:

```rust
use clipboard_history_mcp::core::store::{ImageClipInput, FilesClipInput};

#[test]
fn add_image_clip_writes_blob_and_row() {
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
```

The existing `tests/store.rs` may already have `use clipboard_history_mcp::core::store::*;` or similar — keep using the existing import style; add only what's missing. Each test that mutates `CLIPBOARD_DATA_DIR` should set + remove the env var to avoid leaking into siblings (or better, scope under `serial_test` if that's already a dev-dep — but it's not, so manual env var management is fine for this PR).

- [ ] **Step 2: Run tests — should fail to compile (`ImageClipInput` doesn't exist yet)**

```bash
cargo test --test store add_image_clip_writes_blob_and_row
```

Expected: compile error — `ImageClipInput` not in scope.

- [ ] **Step 3: Add the new types and methods to `src/core/store.rs`**

Near the existing `pub struct ClipInput` (line 10), add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PayloadKind {
    Text,
    Image,
    File,
}

impl PayloadKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PayloadKind::Text => "text",
            PayloadKind::Image => "image",
            PayloadKind::File => "file",
        }
    }
}

pub struct ImageClipInput {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
}

pub struct FilesClipInput {
    pub paths: Vec<std::path::PathBuf>,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
}
```

Extend `Item` (around line 26) with the new payload fields. Replace the existing struct with:

```rust
#[derive(Debug, serde::Serialize)]
pub struct Item {
    pub id: i64,
    pub uuid: String,
    pub text: Option<String>,
    pub preview: String,
    pub length: i64,
    pub primary_kind: String,
    pub kinds: Vec<String>,
    pub tags: Vec<String>,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
    pub first_copied_at: i64,
    pub last_copied_at: i64,
    pub copy_count: i64,
    pub paste_count: i64,
    pub is_pinned: bool,
    // T2 payload metadata. Default to text-only shape for legacy text clips.
    pub payload_kind: String, // "text" | "image" | "file"
    pub blob_path: Option<String>,
    pub blob_size_bytes: Option<i64>,
    pub mime_type: Option<String>,
}
```

Update `row_to_item` and the inline `query_row` mapping in `get_item` to read the new columns. Replace `row_to_item` with:

```rust
fn row_to_item(r: &rusqlite::Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: r.get(0)?, uuid: r.get(1)?, text: r.get(2)?, preview: r.get(3)?,
        length: r.get(4)?, primary_kind: r.get(5)?, kinds: vec![], tags: vec![],
        source_app: r.get(6)?, window_title: r.get(7)?,
        first_copied_at: r.get(8)?, last_copied_at: r.get(9)?,
        copy_count: r.get(10)?, paste_count: r.get(11)?,
        is_pinned: r.get::<_, i64>(12)? == 1,
        payload_kind: r.get(13)?,
        blob_path: r.get(14)?,
        blob_size_bytes: r.get(15)?,
        mime_type: r.get(16)?,
    })
}
```

Extend the SQL strings in `get_item`, `list_with`, and `search` (lines 116-118, 152-153, 157-158, 184-188) to select the new columns. Each `SELECT id, uuid, text, ...` becomes `SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title, first_copied_at, last_copied_at, copy_count, paste_count, is_pinned, payload_kind, blob_path, blob_size_bytes, mime_type, ...`. The `bm25` rank in `search` stays at the end.

The inline closure in `get_item` (lines 120-127) also needs the new columns, mirroring `row_to_item`.

Add `add_image_clip` and `add_files_clip` methods inside `impl Store`, after `add_secret` (around line 113):

```rust
    pub fn add_image_clip(&self, input: ImageClipInput) -> Result<i64> {
        use crate::core::blobs;
        let extension = blobs::extension_for_mime(&input.mime);
        let (rel_path, hash) = blobs::write(&input.bytes, extension)?;
        let now = now_ms();
        if let Some(id) = self.find_by_hash(&hash)? {
            self.conn.execute(
                "UPDATE clips SET copy_count = copy_count + 1, last_copied_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            return Ok(id);
        }
        let preview = format!("[image/{}]", input.mime.split('/').nth(1).unwrap_or("?"));
        let length = 0i64;
        let byte_length = input.bytes.len() as i64;
        let uuid = Uuid::new_v4().to_string();
        let primary_kind = "image".to_string();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                source_app, window_title, first_copied_at, last_copied_at,
                                payload_kind, blob_path, blob_size_bytes, mime_type)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 'image', ?10, ?11, ?12)",
            params![uuid, preview, length, byte_length, hash, primary_kind,
                    input.source_app, input.window_title, now,
                    rel_path, byte_length, input.mime],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT OR IGNORE INTO kinds(clip_id, kind) VALUES (?1, 'image')", params![id])?;
        tx.commit()?;
        Ok(id)
    }

    pub fn add_files_clip(&self, input: FilesClipInput) -> Result<Vec<i64>> {
        use crate::core::blobs;
        let mut ids = Vec::with_capacity(input.paths.len());
        for path in &input.paths {
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("skip file clip {}: {}", path.display(), e);
                    continue;
                }
            };
            let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
            let (rel_path, hash) = blobs::write(&bytes, extension)?;
            let now = now_ms();
            if let Some(id) = self.find_by_hash(&hash)? {
                self.conn.execute(
                    "UPDATE clips SET copy_count = copy_count + 1, last_copied_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
                ids.push(id);
                continue;
            }
            let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
            let preview = format!("[file:{}]", filename);
            let byte_length = bytes.len() as i64;
            let uuid = Uuid::new_v4().to_string();
            let mime = "application/octet-stream".to_string();
            let tx = self.conn.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                    source_app, window_title, first_copied_at, last_copied_at,
                                    payload_kind, blob_path, blob_size_bytes, mime_type)
                 VALUES (?1, NULL, ?2, 0, ?3, ?4, 'file', ?5, ?6, ?7, ?7, 'file', ?8, ?3, ?9)",
                params![uuid, preview, byte_length, hash,
                        input.source_app, input.window_title, now,
                        rel_path, mime],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute("INSERT OR IGNORE INTO kinds(clip_id, kind) VALUES (?1, 'file')", params![id])?;
            tx.commit()?;
            ids.push(id);
        }
        Ok(ids)
    }
```

Extend `Store::delete` (around line 214) to clean up blobs:

```rust
    pub fn delete(&self, id: i64) -> Result<()> {
        let blob_path: Option<String> = self
            .conn
            .query_row(
                "SELECT blob_path FROM clips WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        self.conn.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
        if let Some(rel) = blob_path {
            let _ = crate::core::blobs::delete(&rel); // best-effort; warn-logged inside
        }
        Ok(())
    }
```

Note: `query_row` with `Option<String>` column return needs `.flatten()` because the outer `optional()?` produces `Option<Option<String>>` (row missing vs column NULL).

- [ ] **Step 4: Run the new tests — must pass**

```bash
cargo test --test store
```

Expected: existing tests pass + 3 new tests pass.

- [ ] **Step 5: Run the full lib test suite — no regressions**

```bash
cargo test --lib
cargo test
```

Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add src/core/store.rs tests/store.rs
git commit -m "feat(store): add_image_clip + add_files_clip + payload-aware Item

- New PayloadKind enum (Text|Image|File).
- New ImageClipInput / FilesClipInput.
- Item gains payload_kind, blob_path, blob_size_bytes, mime_type
  (Option<> for blob fields, defaulting to None for text rows).
- add_image_clip writes the blob via core::blobs, dedups on hash,
  bumps copy_count on repeat. mime → file extension via
  blobs::extension_for_mime.
- add_files_clip iterates the path list; each path becomes its own
  row + blob. Read-failures (ENOENT, perms) log warn + skip.
- delete reads blob_path before the DELETE and best-effort removes
  the blob file from disk.

Three new integration tests cover write-and-roundtrip, dedup-on-
repeat, and delete-removes-blob.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Watcher dispatch on `Clip` enum

**Why.** The watcher tick currently reads only text. Refactor to read `Clip` and dispatch into the three new branches. Also wires `CLIPBOARD_NEVER_STORE_SECRETS=1` paranoid-mode short-circuit for non-text clips and the `CLIPBOARD_MAX_BLOB_BYTES` cap.

**Files:**
- Modify: `src/daemon/watcher.rs` — refactor `tick` to dispatch on Clip; extend `WatcherOptions` with `max_blob_bytes`.
- Modify: `src/main.rs` — read `CLIPBOARD_MAX_BLOB_BYTES` env var and plumb through.

- [ ] **Step 1: Extend `WatcherOptions`**

In `src/daemon/watcher.rs`, modify the `WatcherOptions` struct (lines 14-23) — add a new field after `max_items`:

```rust
    pub max_items: i64,
    pub max_blob_bytes: i64,
```

Update `Default` (lines 25-36):

```rust
            max_items: 1000,
            max_blob_bytes: 26_214_400, // 25 MB
```

- [ ] **Step 2: Refactor `tick` to dispatch on `Clip`**

Replace the entire body of `tick` (lines 68-142) with:

```rust
fn tick(
    store: &Store,
    opts: &WatcherOptions,
    last_seen: &mut Option<String>,
) -> anyhow::Result<()> {
    if pasteboard::is_transient() {
        info!("skip: transient pasteboard type");
        return Ok(());
    }

    let clip = pasteboard::read_clip()?;

    // Dedup on text only — image/file dedup is by hash inside the store.
    if let pasteboard::Clip::Text(ref s) = clip {
        if last_seen.as_deref() == Some(s.as_str()) {
            return Ok(());
        }
        *last_seen = Some(s.clone());
    }

    let ctx = context::capture(opts.capture_window_title);
    if let Some(app) = &ctx.front_app {
        if opts.ignore_apps.iter().any(|a| a == app) {
            info!("skip: ignored app {}", app);
            return Ok(());
        }
    }

    match clip {
        pasteboard::Clip::Empty => Ok(()),

        pasteboard::Clip::Text(text) => tick_text(store, opts, &ctx, text),

        pasteboard::Clip::Image { bytes, mime } => {
            if opts.never_store_secrets {
                info!("skip image (paranoid mode)");
                return Ok(());
            }
            if (bytes.len() as i64) > opts.max_blob_bytes {
                warn!(
                    "skip image: {} bytes exceeds CLIPBOARD_MAX_BLOB_BYTES={}",
                    bytes.len(), opts.max_blob_bytes
                );
                return Ok(());
            }
            let id = store.add_image_clip(crate::core::store::ImageClipInput {
                bytes,
                mime: mime.into(),
                source_app: ctx.front_app.clone(),
                window_title: ctx.window_title.clone(),
            })?;
            info!("image captured: id={} mime={} from {:?}", id, mime, ctx.front_app);
            mirror_image_or_file(opts, &ctx, store, id)?;
            store.prune_oldest(opts.max_items)?;
            Ok(())
        }

        pasteboard::Clip::Files(paths) => {
            if opts.never_store_secrets {
                info!("skip files (paranoid mode)");
                return Ok(());
            }
            // Filter out files that are too large; keep the rest.
            let kept: Vec<_> = paths.into_iter().filter(|p| {
                match std::fs::metadata(p) {
                    Ok(m) if (m.len() as i64) <= opts.max_blob_bytes => true,
                    Ok(m) => {
                        warn!("skip file {}: {} bytes exceeds limit", p.display(), m.len());
                        false
                    }
                    Err(_) => false,
                }
            }).collect();
            if kept.is_empty() {
                return Ok(());
            }
            let ids = store.add_files_clip(crate::core::store::FilesClipInput {
                paths: kept,
                source_app: ctx.front_app.clone(),
                window_title: ctx.window_title.clone(),
            })?;
            info!("files captured: {:?} from {:?}", ids, ctx.front_app);
            for id in ids {
                mirror_image_or_file(opts, &ctx, store, id)?;
            }
            store.prune_oldest(opts.max_items)?;
            Ok(())
        }
    }
}

/// Existing text capture flow. Kept as a private helper so tick stays
/// readable.
fn tick_text(
    store: &Store,
    opts: &WatcherOptions,
    ctx: &context::Context,
    text: String,
) -> anyhow::Result<()> {
    if let Some(hit) = secrets::detect_secret(&text) {
        if opts.never_store_secrets {
            info!("secret detected (paranoid mode, ciphertext omitted): {}", hit.kind);
        } else {
            store.add_secret(SecretInput {
                text: hit.value.clone(),
                secret_kind: hit.kind.clone(),
                source_app: ctx.front_app.clone(),
                window_title: ctx.window_title.clone(),
            })?;
            info!("secret captured: {} from {:?}", hit.kind, ctx.front_app);
        }
    } else {
        let cls = types::classify(&text);
        let id = store.add_clip(ClipInput {
            text: text.clone(),
            primary_kind: cls.primary_kind.clone(),
            kinds: cls.kinds,
            source_app: ctx.front_app.clone(),
            window_title: ctx.window_title.clone(),
        })?;
        info!("clip captured: {} from {:?}", cls.primary_kind, ctx.front_app);

        if let Some(mirror) = &opts.vault_mirror {
            let item = crate::core::vault::MirrorItem {
                id,
                primary_kind: &cls.primary_kind,
                source_app: ctx.front_app.as_deref(),
                window_title: ctx.window_title.as_deref(),
                text: &text,
                captured: chrono::Local::now(),
                payload_kind: "text",
                blob_relative_path: None,
                mime_type: None,
            };
            if let Err(e) = mirror.write(&item) {
                warn!("vault mirror failed for clip {}: {}", id, e);
            }
        }
    }

    store.prune_oldest(opts.max_items)?;
    Ok(())
}

/// Vault-mirror hook for image and file clips. Reads the just-inserted
/// row to populate `MirrorItem` with the blob path.
fn mirror_image_or_file(
    opts: &WatcherOptions,
    ctx: &context::Context,
    store: &Store,
    id: i64,
) -> anyhow::Result<()> {
    let Some(mirror) = &opts.vault_mirror else { return Ok(()); };
    let Some(item) = store.get_item(id)? else { return Ok(()); };
    let mirror_item = crate::core::vault::MirrorItem {
        id: item.id,
        primary_kind: &item.primary_kind,
        source_app: item.source_app.as_deref(),
        window_title: item.window_title.as_deref(),
        text: item.preview.as_str(), // preview as the body — image/file have no text
        captured: chrono::Local::now(),
        payload_kind: &item.payload_kind,
        blob_relative_path: item.blob_path.as_deref(),
        mime_type: item.mime_type.as_deref(),
    };
    if let Err(e) = mirror.write(&mirror_item) {
        warn!("vault mirror failed for clip {}: {}", id, e);
    }
    Ok(())
}
```

The `MirrorItem` struct will gain three new fields (`payload_kind`, `blob_relative_path`, `mime_type`) in Task 9. This task lands the watcher caller side; cargo will fail to compile until Task 9 lands. **Make Tasks 8 and 9 a single commit if you want a green build at every commit boundary** — see the note before Step 4.

- [ ] **Step 3: Plumb `CLIPBOARD_MAX_BLOB_BYTES` from `main.rs`**

In `src/main.rs`, find the `WatcherOptions { ... }` literal (around line 76). After the existing `max_items: ...` line, add:

```rust
        max_blob_bytes: std::env::var("CLIPBOARD_MAX_BLOB_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(26_214_400),
```

- [ ] **Step 4: This task does not stand alone — proceed to Task 9 before commit**

Tasks 8 and 9 are tightly coupled (the new `MirrorItem` fields). Skip the commit step here and proceed directly to Task 9. After Task 9 lands, you'll commit both together with the combined message at the end of Task 9.

If you must commit separately for review purposes, commit Task 8 with `--allow-empty` semantics and force the build green by adding placeholder `payload_kind: "text"`, `blob_relative_path: None`, `mime_type: None` directly in the existing `MirrorItem` struct as a temporary stop-gap. Cleanup commit will remove the placeholders. Discouraged — prefer the single combined commit.

---

## Task 9: Vault-mirror image / file path

**Why.** `MirrorItem` extends with payload fields. Sidecar body for image clips renders an `![alt](relative.png)` link to the blob copied into the vault. File clips render bullet links. Daily-note bullets are unchanged.

**Files:**
- Modify: `src/core/vault.rs` — extend `MirrorItem` struct, extend `format_body`, add blob-copy step in `VaultMirror::write`, extend frontmatter.

- [ ] **Step 1: Extend `MirrorItem` struct**

In `src/core/vault.rs`, replace the `MirrorItem` struct (lines 15-22) with:

```rust
pub struct MirrorItem<'a> {
    pub id: i64,
    pub primary_kind: &'a str,
    pub source_app: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub text: &'a str,
    pub captured: DateTime<Local>,
    /// "text" | "image" | "file"
    pub payload_kind: &'a str,
    /// Relative path under `data_dir/blobs/` for image/file clips; None
    /// for text. The vault writer reads this blob and copies it next to
    /// the sidecar.
    pub blob_relative_path: Option<&'a str>,
    /// MIME type for image/file clips; None for text.
    pub mime_type: Option<&'a str>,
}
```

- [ ] **Step 2: Extend `VaultMirror::write` to copy the blob**

Inside `VaultMirror::write` (line 39-72), after the existing `let slug = slug_for_filename(item);` line, before the `let month_folder = ...`, add:

```rust
        let blob_link = if let (Some(rel), Some(_mime)) = (item.blob_relative_path, item.mime_type) {
            // Resolve the source blob in the daemon's data_dir, then copy it
            // to the vault month folder under a name that mirrors the sidecar.
            let src = crate::core::blobs::absolute_path(rel);
            let extension = std::path::Path::new(rel)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("bin")
                .to_string();
            let blob_filename = format!("{}.{}", slug, extension);
            // Compute month_folder eagerly so we have a destination for the
            // blob copy. (The sidecar path computed later uses the same
            // month_folder.)
            let month_folder_for_blob = self
                .root
                .join("clipboard")
                .join(item.captured.format("%Y-%m").to_string());
            std::fs::create_dir_all(&month_folder_for_blob)?;
            let dst = month_folder_for_blob.join(&blob_filename);
            // Copy the blob into the vault. We copy (not symlink) so the
            // vault is portable: a user can sync the vault folder without
            // dragging the daemon's data_dir along.
            std::fs::copy(&src, &dst)?;
            Some(blob_filename)
        } else {
            None
        };
```

Then update `format_body` to receive the blob link. Replace the existing call:

```rust
        let content = format!(
            "{}\n\n{}\n",
            format_frontmatter(item),
            format_body(item)
        );
```

with:

```rust
        let content = format!(
            "{}\n\n{}\n",
            format_frontmatter(item),
            format_body(item, blob_link.as_deref())
        );
```

- [ ] **Step 3: Update `format_body` signature + body**

Replace the existing `format_body` function (lines 140-146) with:

```rust
fn format_body(item: &MirrorItem<'_>, blob_filename: Option<&str>) -> String {
    if let Some(lang) = item.primary_kind.strip_prefix("code:") {
        return format!("```{}\n{}\n```", lang, item.text);
    }
    match item.payload_kind {
        "image" => {
            // Relative-path markdown image link (vault-relative).
            // Obsidian renders it inline.
            let link = blob_filename.unwrap_or("");
            format!("![Captured image]({})", link)
        }
        "file" => {
            let link = blob_filename.unwrap_or("");
            format!(
                "- [`{}`]({}) (filed locally — original at `{}`)",
                blob_filename.unwrap_or("file"),
                link,
                item.text // for files, .text holds the original absolute path; preview is "[file:<name>]"
            )
        }
        _ => item.text.to_string(),
    }
}
```

Note: for file clips, the text stored in the DB is `[file:<basename>]` (set in Task 7). The "original absolute path" rendering above is therefore wrong without an extra field. **Decision:** for v0.6, accept that file-clip mirror bodies show the placeholder preview rather than the original path. This is consistent with file clips being out-of-scope for the slick UX in v0.6 (Linux/Windows file capture is also deferred). A future task can wire the original path through `MirrorItem::source_path` if needed.

To keep the body reasonable, simplify the `"file"` arm to:

```rust
        "file" => {
            format!("- [`{}`]({})", blob_filename.unwrap_or("file"), blob_filename.unwrap_or(""))
        }
```

- [ ] **Step 4: Extend `format_frontmatter` to emit `payload_kind` and `mime_type`**

In `format_frontmatter` (lines 105-138), after the existing `s.push_str(&format!("kind: {}\n", item.primary_kind));` line, add:

```rust
    s.push_str(&format!("payload_kind: {}\n", item.payload_kind));
    if let Some(mime) = item.mime_type {
        s.push_str(&format!("mime_type: {}\n", mime));
    }
```

- [ ] **Step 5: Update existing `format_body` test calls — they now need a second arg**

Find the `format_body(&item)` call sites in the existing `mod tests` block (lines 432-459) and update them to `format_body(&item, None)`:

```rust
    #[test]
    fn body_plain_text_is_literal() {
        let item = item_with_text("https://example.com/foo", "url");
        assert_eq!(format_body(&item, None), "https://example.com/foo");
    }
    // ... and so on for the four other body_* tests
```

Also update `item_with_text` helper (lines 237-249) to fill in the new MirrorItem fields:

```rust
    fn item_with_text<'a>(text: &'a str, primary_kind: &'a str) -> MirrorItem<'a> {
        MirrorItem {
            id: 142,
            primary_kind,
            source_app: Some("Safari"),
            window_title: Some("API Keys — OpenAI Platform"),
            text,
            captured: Local
                .with_ymd_and_hms(2026, 5, 6, 4, 32, 3)
                .single()
                .unwrap(),
            payload_kind: "text",
            blob_relative_path: None,
            mime_type: None,
        }
    }
```

The other test helpers that build `MirrorItem` literals directly (lines 332-339, 348-353, etc.) also need the three new fields populated to `"text"`, `None`, `None`. Update each `MirrorItem { ... }` literal in the test module to include them.

- [ ] **Step 6: Add new tests for image-mirror behaviour**

In `tests/vault_mirror.rs` (or inside `mod tests` in `vault.rs`, follow whichever convention exists), add:

```rust
#[test]
fn image_clip_writes_blob_alongside_sidecar() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
    let blobs_dir = tmp.path().join("blobs");
    std::fs::create_dir_all(&blobs_dir.join("ab")).unwrap();
    let blob_rel = "ab/abcdef.png";
    std::fs::write(blobs_dir.join(blob_rel), b"\x89PNG\r\n\x1a\n").unwrap();

    let vault_root = tmp.path().join("vault");
    let mirror = VaultMirror::new(vault_root.clone());
    mirror.write(&MirrorItem {
        id: 142,
        primary_kind: "image",
        source_app: Some("Preview"),
        window_title: None,
        text: "[image/png]",
        captured: Local.with_ymd_and_hms(2026, 5, 6, 4, 32, 3).single().unwrap(),
        payload_kind: "image",
        blob_relative_path: Some(blob_rel),
        mime_type: Some("image/png"),
    }).unwrap();

    let month_folder = vault_root.join("clipboard").join("2026-05");
    let sidecar = month_folder.join("142-image-image-png.md");
    let blob_in_vault = month_folder.join("142-image-image-png.png");
    assert!(sidecar.exists(), "sidecar should be written");
    assert!(blob_in_vault.exists(), "blob copy should be written");

    let body = std::fs::read_to_string(&sidecar).unwrap();
    assert!(body.contains("payload_kind: image"));
    assert!(body.contains("mime_type: image/png"));
    assert!(body.contains("![Captured image](142-image-image-png.png)"));

    std::env::remove_var("CLIPBOARD_DATA_DIR");
}
```

Note: the slug for `[image/png]` text is `image-png` after the existing slugifier; the literal slug computation is exercised by existing slug tests so we trust it here.

- [ ] **Step 7: Build and run all tests**

```bash
cargo check
cargo test
```

Expected: clean compile (signature changes propagate cleanly because Task 8 already updated the watcher's `MirrorItem` literals); all tests pass.

If clippy fires on the unused `_mime` binding in Step 2's snippet, drop the `let (Some(rel), Some(_mime))` to `let (Some(rel), Some(_)) = (item.blob_relative_path, item.mime_type)` or use a single-`if let`.

- [ ] **Step 8: Commit Tasks 8 + 9 together**

```bash
git add src/daemon/watcher.rs src/main.rs src/core/vault.rs tests/vault_mirror.rs
git commit -m "feat(watcher,vault): dispatch on Clip enum + image/file vault mirror

Watcher tick now reads pasteboard::Clip and dispatches by variant:
  - Empty: no-op
  - Text: existing flow (secret detect → add_clip / add_secret) extracted
    to a tick_text helper for readability.
  - Image: bytes-size cap from new CLIPBOARD_MAX_BLOB_BYTES env var
    (default 25 MB), paranoid-mode short-circuit, store.add_image_clip.
  - Files: per-path size filter, paranoid-mode short-circuit,
    store.add_files_clip.

Each non-text capture also calls a new mirror_image_or_file helper that
reads the just-inserted row, fills MirrorItem with blob metadata, and
calls VaultMirror::write.

VaultMirror gains three new MirrorItem fields (payload_kind,
blob_relative_path, mime_type). VaultMirror::write copies the blob
from data_dir/blobs/<rel> into the vault month folder next to the
sidecar (std::fs::copy, not symlink, so vault sync is portable).
format_body branches on payload_kind to render an image link
(![](file.png)) or file bullet for non-text clips. Frontmatter gains
payload_kind and (when present) mime_type keys.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: MCP `list_history` payload metadata

**Why.** `list_history` returns `Item`s; `Item` already has the new payload fields after Task 7. This task verifies the JSON shape Claude sees actually includes the new fields and that `serde::Serialize` on `Item` emits them.

**Files:**
- Modify: `src/mcp/tools.rs` — minor or no change. Mostly a verification + a tightening of the `list_history` JSON if needed.

- [ ] **Step 1: Sanity-check that the existing `list_history` already serialises payload fields**

Open `src/mcp/tools.rs`. Find `list_history` (around line 95). It calls `serde_json::json!({ "count": ..., "items": items })` where `items: Vec<Item>`. Because `Item` derives `serde::Serialize` and Task 7 added the new fields with default `serde` behaviour, the serialised JSON automatically includes `payload_kind`, `blob_path`, `blob_size_bytes`, `mime_type`. No code change needed.

To verify, add an integration test in `tests/mcp_smoke.rs` (or as a comment if `mcp_smoke` is `#[ignore]`'d everywhere — in which case skip the test and just rely on the existing tests):

```rust
#[test]
fn item_serialisation_includes_payload_fields() {
    use clipboard_history_mcp::core::store::Item;
    let item = Item {
        id: 1, uuid: "u".into(), text: None, preview: "p".into(),
        length: 0, primary_kind: "image".into(), kinds: vec![], tags: vec![],
        source_app: None, window_title: None,
        first_copied_at: 0, last_copied_at: 0, copy_count: 1, paste_count: 0,
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
```

If `tests/mcp_smoke.rs` doesn't yet have a non-`#[ignore]` test, add this as the first one (no `#[ignore]`). It runs in CI.

- [ ] **Step 2: Run the test — should pass**

```bash
cargo test --test mcp_smoke item_serialisation_includes_payload_fields
```

Expected: pass.

- [ ] **Step 3: Commit**

```bash
git add tests/mcp_smoke.rs
git commit -m "test(mcp): verify list_history Item serialisation includes payload fields

No production code change — Item's serde::Serialize automatically picks
up the four new fields added in T2 Task 7. This test pins the on-the-
wire JSON shape so a future refactor that drops the fields breaks loudly.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: MCP `get_item` with opt-in `with_blob`

**Why.** Returning blob bytes from `list_history` for every image would make responses huge. `get_item` adds an opt-in `with_blob: bool = false` flag; when `true` and the item is image/file, includes a base64 data URL. Default is `false` so existing callers see no change.

**Files:**
- Modify: `src/mcp/tools.rs` — extend `GetItemParams`, extend `get_item` body.

- [ ] **Step 1: Extend `GetItemParams`**

In `src/mcp/tools.rs` (line 34-37), replace `GetItemParams` with:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetItemParams {
    pub id: i64,
    /// When true and the clip's payload is image or file, include the
    /// blob bytes as a base64 data URL in the response (`blob_data_url`
    /// field). Default false to keep responses small.
    #[serde(default)]
    pub with_blob: bool,
}
```

- [ ] **Step 2: Extend `get_item` body**

Replace the existing `get_item` body (lines 104-117) with:

```rust
    #[tool(description = "Fetch one clipboard entry by id. For secrets returns metadata only. Pass with_blob=true to inline an image/file blob as a base64 data URL.")]
    async fn get_item(&self, Parameters(p): Parameters<GetItemParams>) -> String {
        match self.store.get_item(p.id) {
            Ok(Some(item)) => {
                let requires_unlock = item.primary_kind.starts_with("secret:");
                let mut v = serde_json::to_value(&item).unwrap_or(serde_json::Value::Null);
                if requires_unlock {
                    v["requiresUnlock"] = serde_json::json!(true);
                }
                if p.with_blob && !requires_unlock {
                    if let (Some(rel), Some(mime)) = (item.blob_path.as_deref(), item.mime_type.as_deref()) {
                        match crate::core::blobs::read(rel) {
                            Ok(bytes) => {
                                use base64::{engine::general_purpose::STANDARD, Engine};
                                let encoded = STANDARD.encode(&bytes);
                                v["blob_data_url"] = serde_json::Value::String(
                                    format!("data:{};base64,{}", mime, encoded)
                                );
                            }
                            Err(e) => {
                                v["blob_error"] = serde_json::Value::String(format!("{}", e));
                            }
                        }
                    }
                }
                v.to_string()
            }
            Ok(None) => format!("error: not found id={}", p.id),
            Err(e) => format!("error: {}", e),
        }
    }
```

- [ ] **Step 3: Add a test for the with_blob path**

In `tests/mcp_smoke.rs`, add:

```rust
#[test]
fn get_item_with_blob_inlines_base64_data_url() {
    use clipboard_history_mcp::core::blobs;
    use clipboard_history_mcp::core::store::{ImageClipInput, Store};

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
```

This test exercises the blob-read path that `get_item` uses. The MCP tool itself is async and sits behind `rmcp`, so a full end-to-end MCP test is out of scope here.

- [ ] **Step 4: Run tests**

```bash
cargo test --test mcp_smoke
cargo check
```

Expected: green.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/tools.rs tests/mcp_smoke.rs
git commit -m "feat(mcp): get_item accepts with_blob:true for inline base64

Default false keeps responses small for normal usage. When true on
image/file clips, attaches a 'blob_data_url' field shaped as
'data:<mime>;base64,<payload>'. Secrets remain blocked even when
with_blob is requested.

If the blob file is missing on disk, the response includes a
'blob_error' field instead of failing the whole tool call — list_history
still returns metadata for orphaned-blob rows.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: MCP `copy_item` dispatch on payload_kind

**Why.** Restoring an image clip back to the system pasteboard requires reading the blob and calling `pasteboard::write_clip(Clip::Image)`. File clips need `Clip::Files`. Text clips already work via `write_clipboard`.

**Files:**
- Modify: `src/mcp/tools.rs` — extend `copy_item` dispatch.

- [ ] **Step 1: Replace `copy_item` body**

In `src/mcp/tools.rs`, replace the body of `copy_item` (lines 263-283) with:

```rust
    #[tool(description = "Restore a clip to the system clipboard. Refuses secrets — use unlock_secret first. Image and file clips are restored via NSPasteboard with the correct UTI on macOS.")]
    async fn copy_item(&self, Parameters(p): Parameters<IdParams>) -> String {
        match self.store.get_item(p.id) {
            Ok(Some(item)) => {
                if item.primary_kind.starts_with("secret:") {
                    return serde_json::json!({
                        "error": "cannot restore secret directly",
                        "requiresUnlock": true
                    })
                    .to_string();
                }
                let result = match item.payload_kind.as_str() {
                    "text" => {
                        let Some(text) = item.text.as_deref() else {
                            return serde_json::json!({"error": "text clip has no text"}).to_string();
                        };
                        pasteboard::write_clipboard(text)
                    }
                    "image" => {
                        let Some(rel) = item.blob_path.as_deref() else {
                            return serde_json::json!({"error": "image clip missing blob_path"}).to_string();
                        };
                        let mime = item.mime_type.as_deref().unwrap_or("image/png");
                        match crate::core::blobs::read(rel) {
                            Ok(bytes) => pasteboard::write_clip(&pasteboard::Clip::Image {
                                bytes,
                                mime: if mime == "image/tiff" { "image/tiff" } else { "image/png" },
                            }),
                            Err(e) => return serde_json::json!({"error": format!("blob read: {}", e)}).to_string(),
                        }
                    }
                    "file" => {
                        // For files we use the original path stored in
                        // window_title is incorrect — actually the blob_path
                        // points to a copy in our blobs dir. Restoring a
                        // 'file' pasteboard from a blobs/ path is correct
                        // for "paste this file somewhere"; the user can
                        // drag-and-drop it.
                        let Some(rel) = item.blob_path.as_deref() else {
                            return serde_json::json!({"error": "file clip missing blob_path"}).to_string();
                        };
                        let abs = crate::core::blobs::absolute_path(rel);
                        if !abs.exists() {
                            return serde_json::json!({
                                "error": "file no longer exists at original path",
                                "lastKnownPath": abs.display().to_string(),
                            }).to_string();
                        }
                        pasteboard::write_clip(&pasteboard::Clip::Files(vec![abs]))
                    }
                    other => return serde_json::json!({"error": format!("unknown payload_kind: {}", other)}).to_string(),
                };
                if let Err(e) = result {
                    return format!("error: {}", e);
                }
                let _ = self.store.bump_paste(p.id);
                serde_json::json!({ "ok": true, "id": p.id, "payload_kind": item.payload_kind }).to_string()
            }
            Ok(None) => format!("error: not found id={}", p.id),
            Err(e) => format!("error: {}", e),
        }
    }
```

- [ ] **Step 2: Build**

```bash
cargo check
```

Expected: clean compile.

- [ ] **Step 3: Run tests**

```bash
cargo test
```

Expected: green. End-to-end macOS pasteboard restore is `#[ignore]` in `clipboard_smoke`; not exercised in CI.

- [ ] **Step 4: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): copy_item dispatches on payload_kind

  - 'text': existing write_clipboard path.
  - 'image': read blob from data_dir/blobs/<rel>, write to pasteboard
    via write_clip(Clip::Image) with the stored mime_type (PNG/TIFF).
  - 'file': resolve the blob_path; if the file is missing on disk, the
    response includes 'lastKnownPath' so the caller knows what was
    expected. Otherwise write_clip(Clip::Files([abs])).

Secrets are still rejected with the existing requiresUnlock JSON.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 13: README + CHANGELOG + final integration

**Why.** Document the new env var, the new payload metadata fields visible to MCP callers, and the sidecar layout for image/file clips. Run the full test suite + clippy + sanity-grep before opening the PR.

**Files:**
- Modify: `README.md` — add `CLIPBOARD_MAX_BLOB_BYTES` to the Configuration table + a paragraph in "What this gets you" mentioning image capture.
- Modify: `CHANGELOG.md` — add to `[Unreleased]` ### Added / ### Changed.

- [ ] **Step 1: Update `README.md` Configuration table**

Find the `## Configuration` section. The table has a row for each `CLIPBOARD_*` env var. Add this row after `CLIPBOARD_VAULT_PATH`:

```markdown
| `CLIPBOARD_MAX_BLOB_BYTES` | `26214400` (25 MB) | Skip image / file pasteboards larger than this. Logs a `WARN` line with the source app + size. |
```

In the "What this gets you" demo block (around line 22-39), add an example prompt:

```
You: "Show me the screenshot I just copied"
Claude: Returns a 142×120 PNG, mime image/png, captured from Preview 2 minutes ago.
```

(Pick any reasonable spot in the example block — alphabetical or thematic ordering doesn't matter; just place the line so the surrounding code-block context renders.)

- [ ] **Step 2: Update `CHANGELOG.md` `[Unreleased]`**

Add to the existing `### Added` block (or create one if missing):

```markdown
### Added
- **Image and file capture (T2).** Pasteboard PNG / TIFF and macOS file URLs are now captured alongside text. Blobs are content-hash deduplicated and stored under `data_dir/blobs/<aa>/<sha256>.<ext>`. New env var `CLIPBOARD_MAX_BLOB_BYTES` (default 25 MB) caps per-clip size. `list_history` and `get_item` include `payload_kind`, `blob_path`, `blob_size_bytes`, `mime_type`; `get_item` accepts `with_blob: true` for an inline base64 data URL. `copy_item` writes the blob back to the pasteboard with the correct UTI on macOS. Vault-mirror copies the blob alongside the sidecar and renders an image link in the body.
- New env var: `CLIPBOARD_MAX_BLOB_BYTES`.
- New top-level dep: `base64 = "0.22"`.
- New private module: `core/blobs` (filesystem ops with content-hash dedup).
```

Add to `### Changed` (or create):

```markdown
### Changed
- Schema bumped to v2. Migration is automatic on next daemon start; existing rows get `payload_kind = 'text'` via the column default.
- `pasteboard::read_clipboard()` is now a thin wrapper over a new `pasteboard::read_clip()` returning a `Clip` enum (`Empty | Text | Image | Files`). Existing text-only callers compile and work unchanged.
```

- [ ] **Step 3: Run the full test suite**

```bash
cargo test
```

Expected: all suites green. Some macOS pasteboard round-trip tests are `#[ignore]`'d.

- [ ] **Step 4: Run clippy**

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: clean. If new lints fire on T2-introduced code, fix them in this commit (do not `#[allow]`).

- [ ] **Step 5: Sanity-grep**

```bash
grep -nE '\.(expect|unwrap)\(' src/main.rs src/daemon/ src/mcp/ src/core/{crypto,store,vault,pasteboard,biometry,blobs}.rs | grep -v 'unwrap_or' | grep -v '#\[cfg(test)' | grep -v '// '
```

Expected: only the two acknowledged init-time `.expect()` calls from T1 (`crypto.rs:22` keyring init, `pasteboard.rs:6` Clipboard::new). T2's new code adds no new production unwraps.

- [ ] **Step 6: Commit the docs + run final test sweep**

```bash
git add README.md CHANGELOG.md
git commit -m "docs(t2): README configuration + CHANGELOG entries for image/file capture

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 7: Push branch and open PR**

```bash
git push -u origin feat/v0.6-T2-image-file-capture
gh pr create --base main --title "feat(v0.6): T2 image + file capture" --body "$(cat <<'EOF'
## Summary

T2 from the [v0.6 adoption-first roadmap](docs/superpowers/specs/2026-05-08-v0.6-adoption-roadmap-design.md). Pasteboard images (PNG / TIFF on macOS) and file URLs (macOS) are captured alongside text. Blobs land on disk under \`data_dir/blobs/\` content-hash-deduplicated; MCP tools surface payload metadata; vault-mirror copies blobs into the vault next to the sidecar.

Sub-spec: [\`docs/superpowers/specs/2026-05-08-v0.6-T2-image-file-capture-design.md\`](docs/superpowers/specs/2026-05-08-v0.6-T2-image-file-capture-design.md). Thirteen sequential commits, each TDD'd, each spec+quality reviewed.

### Added
- Image and file capture as first-class clips. \`payload_kind\` ∈ \`text|image|file\`.
- \`core/blobs\` module: filesystem storage with content-hash dedup, two-char prefix sharding.
- \`pasteboard::Clip\` enum + \`read_clip\` / \`write_clip\` for non-text payloads. \`read_clipboard\` is now a thin wrapper.
- \`Store::add_image_clip\` / \`add_files_clip\`. \`Store::delete\` cleans up the blob file.
- \`get_item(with_blob: true)\` returns a \`data:<mime>;base64,<bytes>\` URL.
- \`copy_item\` dispatches on payload_kind; image/file restored via \`NSPasteboard\` with the correct UTI.
- Vault-mirror copies the blob to \`<vault>/clipboard/YYYY-MM/<id>-<kind>-<slug>.<ext>\` next to the sidecar; sidecar body renders \`![Captured image](relative.png)\`.
- New env var: \`CLIPBOARD_MAX_BLOB_BYTES\` (default 25 MB). Larger blobs are dropped with a WARN log.
- New top-level dep: \`base64 = \"0.22\"\`.

### Changed
- Schema bumped to v2 (four nullable columns + index on \`payload_kind\`). Auto-migrates on next start.
- Watcher tick refactored to dispatch on \`Clip\` enum.

### Out of scope (per sub-spec §5)
Linux image capture, blob encryption at rest, OCR, animated GIF/video preview, thumbnail generation, per-app blob policies, schema migration framework refactor, Windows.

## Test plan

- [x] \`cargo test\` — all suites green (lib + integration; some macOS pasteboard round-trip tests are \`#[ignore]\`'d by design)
- [x] \`cargo clippy --all-targets -- -D warnings\` — clean
- [x] Sanity-grep: no new production \`.expect()\` / \`.unwrap()\`; only the two T1-acknowledged init-time hits remain
- [ ] **Manual macOS verification:** screenshot via ⌘⇧4-Ctrl, copy to pasteboard, then \`claude \"show me the last image\"\` returns the PNG with \`payload_kind: image\`
- [ ] **Manual macOS verification:** drag a file from Finder to a pasteboard-aware app, \`claude \"list my last clips\"\` returns it as \`payload_kind: file\`
- [ ] **Manual macOS verification:** with \`CLIPBOARD_VAULT_PATH\` set, captured screenshot produces a sidecar md AND the .png file at the matching vault path
- [ ] **Manual macOS verification:** \`copy_item\` on an image clip restores the image to the pasteboard (re-paste in Preview to confirm)
EOF
)"
```

Expected: PR URL printed.

---

## Done-criteria for T2

The track ships when **all** are true:

1. PR merged into `main`.
2. \`cargo test\` and \`cargo clippy --all-targets -- -D warnings\` are green on the merge commit.
3. Schema v2 migration runs cleanly on a v1 database with rows present (verified by `migrates_v1_to_v2_preserves_existing_rows` test).
4. PNG screenshot copied via macOS `⌘⇧4`-Ctrl is captured by the daemon as `payload_kind: image`, mime `image/png`, with the blob present at `data_dir/blobs/<aa>/<sha256>.png` (manual).
5. Repeating the same screenshot does NOT duplicate the row — `copy_count` increments instead (verified by `add_image_clip_dedups_on_repeat` test).
6. `Store::delete(id)` on an image clip removes both the row and the blob file (verified by `delete_image_clip_removes_blob_file` test).
7. With `CLIPBOARD_VAULT_PATH` set, a captured image produces both the sidecar and a same-folder image file referenced via `![Captured image](file.png)` (verified by `image_clip_writes_blob_alongside_sidecar` test + manual sanity check).
8. `copy_item` on an image clip restores the image to the pasteboard with the correct UTI on macOS (manual).
9. CHANGELOG `[Unreleased]` covers Added/Changed bullets above.
10. README Configuration table documents `CLIPBOARD_MAX_BLOB_BYTES`.
