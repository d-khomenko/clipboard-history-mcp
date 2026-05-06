# Obsidian vault mirror — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Daemon-side auto-mirror of every non-secret clipboard capture into the user's Obsidian vault as a sidecar `.md` file plus a bullet appended to the daily note. Configured via `--vault PATH` at install time.

**Architecture:** Single new module `src/core/vault.rs` holds formatting + atomic-write logic. One hook in `src/daemon/watcher.rs` after `store.add_clip()` calls `mirror.write(item)`. Secrets bypass the mirror **structurally** because the `add_secret` branch never calls `add_clip`. CLI install flag threads `CLIPBOARD_VAULT_PATH` through launchd plist (macOS) or systemd unit (Linux) into the daemon at startup.

**Tech Stack:** Rust 1.95+, `chrono` 0.4 (new dep, `clock + std` features only), `tempfile` 3.x (already a dev-dep), `anyhow` for error plumbing, existing `tracing` for warn-level logging on failures.

**Reference spec:** [`docs/superpowers/specs/2026-05-06-vault-mirror-design.md`](../specs/2026-05-06-vault-mirror-design.md)

**Branch:** `feat/vault-mirror` (already created from `main` post-v4-linux-merge; spec committed as `af319ff`).

---

## File map

### New files

```
src/core/vault.rs                          # VaultMirror + MirrorItem + formatters + atomic write
tests/vault_mirror.rs                      # integration: tempdir vault, sidecar+daily round-trip
```

### Modified files

```
Cargo.toml                                 # +chrono dep
src/core/mod.rs                            # pub mod vault;
src/daemon/watcher.rs                      # WatcherOptions.vault_mirror + hook after add_clip
src/main.rs                                # read CLIPBOARD_VAULT_PATH env, build VaultMirror
src/cli/install.rs                         # InstallOpts.vault: Option<PathBuf>
src/cli/install_macos.rs                   # extend env_dict with CLIPBOARD_VAULT_PATH when set
src/cli/install_linux.rs                   # extend systemd template with vault env line
scripts/systemd.service.template           # add __VAULT_ENV_LINE__ placeholder
src/main.rs (clap subcommand)              # --vault PATH flag on `install`
README.md                                  # add CLIPBOARD_VAULT_PATH to config table + install example
CHANGELOG.md                               # v0.5.0-alpha.0 entry
Cargo.toml                                 # version bump → 0.5.0-alpha.0
```

---

## Milestones

| # | Tasks | Outcome |
|---|---|---|
| **M1** | 1–3 | New `core/vault.rs` module with public types compiles. |
| **M2** | 4–6 | Formatters (slug, frontmatter, body) unit-tested. |
| **M3** | 7–9 | Atomic write + sidecar + daily-note append, all integration-tested in tempdir. |
| **M4** | 10–11 | Daemon hook, `CLIPBOARD_VAULT_PATH` env-var read in main, watcher integration test. |
| **M5** | 12–14 | `--vault PATH` install flag, launchd + systemd env injection. |
| **M6** | 15–17 | README, CHANGELOG, version bump, manual smoke test. |

---

# M1 — Foundation

## Task 1: Add `chrono` dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add chrono under `[dependencies]`**

Find the existing `[dependencies]` block in `Cargo.toml` (it starts around line 16 with `rmcp = ...`). Add this line after `url = "2.5"`:

```toml
chrono                     = { version = "0.4", default-features = false, features = ["clock", "std"] }
```

The `default-features = false` opt-out drops `oldtime` (40 KB compile size) and `wasmbind` (irrelevant on macOS/Linux). `clock` gives us `Local::now()`, `std` gives us `Display` and parsing.

- [ ] **Step 2: Run cargo check**

```bash
. "$HOME/.cargo/env"
cargo check 2>&1 | tail -10
```

Expected: clean compile (no use sites yet — added a dep, didn't import anywhere).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(deps): add chrono for vault timestamp formatting"
```

---

## Task 2: Create `src/core/vault.rs` skeleton

**Files:**
- Create: `src/core/vault.rs`
- Modify: `src/core/mod.rs`

- [ ] **Step 1: Write the module skeleton**

Create `src/core/vault.rs` with the public type surface and `unimplemented!()` bodies. We'll fill the bodies in tasks 4-9, but the types compile NOW so later tasks can call them in tests.

```rust
//! Daemon-side mirror of captured non-secret clips into an Obsidian vault.
//!
//! Hooked from `src/daemon/watcher.rs` after `store.add_clip()` returns.
//! Secrets never reach this module — they take the `add_secret` branch.
//!
//! See `docs/superpowers/specs/2026-05-06-vault-mirror-design.md` for design.

use anyhow::Result;
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};

/// One captured clip, in the shape the vault writer needs.
///
/// Borrowed from the watcher to avoid cloning the clip text just to format it.
pub struct MirrorItem<'a> {
    pub id: i64,
    pub primary_kind: &'a str,
    pub source_app: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub text: &'a str,
    pub captured: DateTime<Local>,
}

/// Configured mirror writer rooted at an Obsidian vault directory.
///
/// `root` is created lazily on first write — install does not require the
/// directory to exist yet.
pub struct VaultMirror {
    root: PathBuf,
}

impl VaultMirror {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Write sidecar + append to daily note. Errors are returned but the
    /// daemon's caller is expected to log + continue, not crash.
    pub fn write(&self, _item: &MirrorItem<'_>) -> Result<()> {
        unimplemented!("Task 10")
    }
}

// --- internal helpers (private — exposed only via #[cfg(test)] in tests below) ---

#[allow(dead_code)]
fn slug_for_filename(_item: &MirrorItem<'_>) -> String {
    unimplemented!("Task 4")
}

#[allow(dead_code)]
fn format_frontmatter(_item: &MirrorItem<'_>) -> String {
    unimplemented!("Task 5")
}

#[allow(dead_code)]
fn format_body(_item: &MirrorItem<'_>) -> String {
    unimplemented!("Task 6")
}

#[allow(dead_code)]
fn atomic_write(_path: &Path, _contents: &str) -> Result<()> {
    unimplemented!("Task 7")
}

#[allow(dead_code)]
fn append_daily(_path: &Path, _line: &str) -> Result<()> {
    unimplemented!("Task 9")
}
```

- [ ] **Step 2: Register the module**

Open `src/core/mod.rs` and add `pub mod vault;` next to the existing `pub mod` lines. Order: alphabetical with the existing entries (it already has `pub mod store;`, `pub mod types;`, etc).

```rust
// somewhere among the existing pub mod lines:
pub mod vault;
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check 2>&1 | tail -5
```

Expected: clean compile. The `unimplemented!()` calls are runtime panics, not compile errors. `#[allow(dead_code)]` silences warnings on the private helpers since they're not called yet.

- [ ] **Step 4: Commit**

```bash
git add src/core/vault.rs src/core/mod.rs
git commit -m "feat(vault): module skeleton with public type surface"
```

---

## Task 3: Define `MirrorItem` use-site shape (no test yet — proven by use in Task 4)

This task is intentionally empty in steps. The shape was committed in Task 2. It exists in the milestone table to flag that task 4 onwards depends on `MirrorItem`/`VaultMirror` being importable. Skip if reading sequentially.

---

# M2 — Formatters

## Task 4: `slug_for_filename` — TDD

**Files:**
- Modify: `src/core/vault.rs` — replace the stub `slug_for_filename` body
- Test: `src/core/vault.rs` (inline `#[cfg(test)] mod tests`)

The slug schema from the spec section 4.3:

> `<id>-<kind-with-dashes>-<preview-slug>.md`
> - `<id>` — SQLite autoincrement id
> - `<kind-with-dashes>` — `:` replaced with `_`
> - `<preview-slug>` — first 60 chars of clip text, lowercased, non-`[a-z0-9]+` collapsed to single `-`, leading/trailing `-` stripped

- [ ] **Step 1: Write the failing tests**

Append at the bottom of `src/core/vault.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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
        }
    }

    #[test]
    fn slug_basic_url() {
        let item = item_with_text("https://platform.openai.com/api-keys", "url");
        assert_eq!(
            slug_for_filename(&item),
            "142-url-https-platform-openai-com-api-keys"
        );
    }

    #[test]
    fn slug_replaces_colon_in_kind() {
        let item = item_with_text("def authenticate():", "code:python");
        // kind `code:python` → `code_python`
        assert!(slug_for_filename(&item).starts_with("142-code_python-"));
    }

    #[test]
    fn slug_collapses_runs_of_special_chars() {
        let item = item_with_text("foo!!!  ???bar", "text");
        assert_eq!(slug_for_filename(&item), "142-text-foo-bar");
    }

    #[test]
    fn slug_caps_preview_at_60_chars() {
        let long = "a".repeat(200);
        let item = item_with_text(&long, "text");
        let s = slug_for_filename(&item);
        // "142-text-" prefix is 9 chars; preview cap is 60 chars; total ≤ 69
        assert!(s.len() <= 69, "got {} chars: {}", s.len(), s);
    }

    #[test]
    fn slug_strips_leading_and_trailing_dashes() {
        let item = item_with_text("---hello---", "text");
        assert_eq!(slug_for_filename(&item), "142-text-hello");
    }

    #[test]
    fn slug_handles_unicode() {
        let item = item_with_text("Привіт світ", "text");
        // non-ASCII drops to `-`; collapses to single dashes; lowercased a-z0-9 only
        let s = slug_for_filename(&item);
        assert!(s.starts_with("142-text-"));
        // post-prefix portion has no spaces, no Cyrillic, only [a-z0-9-]
        let suffix = &s["142-text-".len()..];
        assert!(suffix.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "suffix {:?} contains non-ASCII", suffix);
    }

    #[test]
    fn slug_empty_preview_omits_suffix() {
        // whitespace-only text — daemon shouldn't pass these but defensive
        let item = item_with_text("   \n\t  ", "text");
        // No trailing dash
        assert_eq!(slug_for_filename(&item), "142-text");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib vault::tests::slug 2>&1 | tail -20
```

Expected: 7 failures with `unimplemented` panic from `slug_for_filename`.

- [ ] **Step 3: Implement `slug_for_filename`**

Replace the stub body in `src/core/vault.rs`:

```rust
fn slug_for_filename(item: &MirrorItem<'_>) -> String {
    let kind = item.primary_kind.replace(':', "_");

    // First 60 chars of preview, then ASCII-fold + collapse non-alphanumerics.
    let preview_raw: String = item.text.chars().take(60).collect();
    let mut buf = String::with_capacity(preview_raw.len());
    let mut last_was_dash = false;
    for ch in preview_raw.chars() {
        if ch.is_ascii_alphanumeric() {
            buf.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            buf.push('-');
            last_was_dash = true;
        }
    }
    let preview_slug = buf.trim_matches('-').to_string();

    if preview_slug.is_empty() {
        format!("{}-{}", item.id, kind)
    } else {
        format!("{}-{}-{}", item.id, kind, preview_slug)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --lib vault::tests::slug 2>&1 | tail -10
```

Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/core/vault.rs
git commit -m "feat(vault): slug_for_filename with kind-colon swap + preview slugify"
```

---

## Task 5: `format_frontmatter` — TDD

**Files:**
- Modify: `src/core/vault.rs`

Spec section 4.1 fixes the YAML shape:

```yaml
---
id: 142
kind: url
secondary_kinds: []
source: Safari
window_title: "API Keys — OpenAI Platform"
captured: 2026-05-06T04:32:03+03:00
size: 52
---
```

- [ ] **Step 1: Add tests at bottom of `mod tests`**

```rust
#[test]
fn frontmatter_minimal_shape() {
    let item = item_with_text("hello", "text");
    let fm = format_frontmatter(&item);
    // Bracketed by --- on its own lines.
    assert!(fm.starts_with("---\n"));
    assert!(fm.trim_end().ends_with("\n---"));
    assert!(fm.contains("id: 142"));
    assert!(fm.contains("kind: text"));
    assert!(fm.contains("source: Safari"));
    // Window title gets quoted because it contains spaces and an em-dash
    assert!(fm.contains(r#"window_title: "API Keys — OpenAI Platform""#));
    assert!(fm.contains("size: 5")); // "hello" = 5 bytes
}

#[test]
fn frontmatter_omits_missing_optional_fields() {
    let item = MirrorItem {
        id: 1,
        primary_kind: "text",
        source_app: None,
        window_title: None,
        text: "x",
        captured: Local.with_ymd_and_hms(2026, 5, 6, 4, 32, 3).single().unwrap(),
    };
    let fm = format_frontmatter(&item);
    assert!(!fm.contains("source:"));
    assert!(!fm.contains("window_title:"));
}

#[test]
fn frontmatter_quotes_window_title_with_double_quote() {
    let item = MirrorItem {
        id: 1,
        primary_kind: "text",
        source_app: None,
        window_title: Some(r#"He said "hi""#),
        text: "x",
        captured: Local.with_ymd_and_hms(2026, 5, 6, 4, 32, 3).single().unwrap(),
    };
    let fm = format_frontmatter(&item);
    // YAML escape: " inside double-quoted string becomes \"
    assert!(fm.contains(r#"window_title: "He said \"hi\"""#));
}

#[test]
fn frontmatter_captured_is_rfc3339_with_local_offset() {
    let item = item_with_text("x", "text");
    let fm = format_frontmatter(&item);
    // Look for the date and "T" separator; offset format ±HH:MM
    let captured_line = fm.lines().find(|l| l.starts_with("captured:")).unwrap();
    assert!(captured_line.contains("2026-05-06T04:32:03"));
    // RFC 3339 trailing offset: +HH:MM or -HH:MM (local TZ runtime-dependent)
    let offset = &captured_line[captured_line.len() - 6..];
    assert!(
        (offset.starts_with('+') || offset.starts_with('-')) && offset.contains(':'),
        "expected RFC3339 offset, got {:?}",
        offset
    );
}

#[test]
fn frontmatter_size_is_byte_count_not_char_count() {
    // "Привіт" is 6 chars but 12 bytes in UTF-8.
    let item = item_with_text("Привіт", "text");
    let fm = format_frontmatter(&item);
    assert!(fm.contains("size: 12"), "got: {}", fm);
}
```

- [ ] **Step 2: Run tests, expect failures**

```bash
cargo test --lib vault::tests::frontmatter 2>&1 | tail -10
```

Expected: 5 failures with unimplemented panic.

- [ ] **Step 3: Implement `format_frontmatter`**

Replace the stub:

```rust
fn format_frontmatter(item: &MirrorItem<'_>) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("id: {}\n", item.id));
    s.push_str(&format!("kind: {}\n", item.primary_kind));
    if let Some(app) = item.source_app {
        s.push_str(&format!("source: {}\n", app));
    }
    if let Some(title) = item.window_title {
        // Always double-quote; escape embedded " and \ per YAML 1.2 double-quoted scalar.
        let escaped = title.replace('\\', "\\\\").replace('"', "\\\"");
        s.push_str(&format!("window_title: \"{}\"\n", escaped));
    }
    s.push_str(&format!("captured: {}\n", item.captured.to_rfc3339()));
    s.push_str(&format!("size: {}\n", item.text.len()));
    s.push_str("---\n");
    s
}
```

- [ ] **Step 4: Run tests, expect pass**

```bash
cargo test --lib vault::tests::frontmatter 2>&1 | tail -10
```

Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/core/vault.rs
git commit -m "feat(vault): YAML frontmatter formatter with RFC3339 timestamp"
```

---

## Task 6: `format_body` — TDD

**Files:**
- Modify: `src/core/vault.rs`

Spec 4.1: body is the literal clip text. If `kind` starts with `code:` (e.g. `code:python`), wrap in fenced code block with the language tag.

- [ ] **Step 1: Add tests**

```rust
#[test]
fn body_plain_text_is_literal() {
    let item = item_with_text("https://example.com/foo", "url");
    assert_eq!(format_body(&item), "https://example.com/foo");
}

#[test]
fn body_code_kind_gets_fenced() {
    let item = item_with_text("def f(): pass", "code:python");
    assert_eq!(
        format_body(&item),
        "```python\ndef f(): pass\n```"
    );
}

#[test]
fn body_code_rust_kind() {
    let item = item_with_text("fn main() {}", "code:rust");
    assert_eq!(format_body(&item), "```rust\nfn main() {}\n```");
}

#[test]
fn body_code_unknown_lang_after_colon() {
    // Anything after `code:` becomes the fence info string verbatim.
    let item = item_with_text("foo", "code:zzz");
    assert_eq!(format_body(&item), "```zzz\nfoo\n```");
}

#[test]
fn body_does_not_escape_text() {
    // Markdown special chars in clip stay as-is — vault note isn't expected
    // to render them as markdown unless the user wraps things themselves.
    let item = item_with_text("# header *bold* [link](url)", "text");
    assert_eq!(format_body(&item), "# header *bold* [link](url)");
}
```

- [ ] **Step 2: Run tests, expect failures**

```bash
cargo test --lib vault::tests::body 2>&1 | tail -10
```

Expected: 5 failures.

- [ ] **Step 3: Implement `format_body`**

```rust
fn format_body(item: &MirrorItem<'_>) -> String {
    if let Some(lang) = item.primary_kind.strip_prefix("code:") {
        format!("```{}\n{}\n```", lang, item.text)
    } else {
        item.text.to_string()
    }
}
```

- [ ] **Step 4: Run tests, expect pass**

```bash
cargo test --lib vault::tests::body 2>&1 | tail -10
```

Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/core/vault.rs
git commit -m "feat(vault): body formatter with code-fence wrapping for code:* kinds"
```

---

# M3 — I/O

## Task 7: `atomic_write` — write-then-rename, fsync

**Files:**
- Modify: `src/core/vault.rs`

The atomic primitive: write to `path.<pid>.tmp`, fsync the tmp file, then rename onto the final path. Rename is atomic on POSIX filesystems for same-filesystem operations. Obsidian/iCloud syncs see either the old file or the complete new file, never a half-written one.

- [ ] **Step 1: Add tests**

```rust
#[test]
fn atomic_write_creates_file_with_contents() {
    let tmp = tempfile::TempDir::new().unwrap();
    let target = tmp.path().join("subdir/note.md");
    atomic_write(&target, "hello world").unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello world");
}

#[test]
fn atomic_write_creates_parent_dirs() {
    let tmp = tempfile::TempDir::new().unwrap();
    // 3 levels deep, none exist
    let target = tmp.path().join("a/b/c/note.md");
    atomic_write(&target, "x").unwrap();
    assert!(target.exists());
}

#[test]
fn atomic_write_overwrites_existing_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let target = tmp.path().join("note.md");
    std::fs::write(&target, "old").unwrap();
    atomic_write(&target, "new").unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
}

#[test]
fn atomic_write_does_not_leave_tmp_files_on_success() {
    let tmp = tempfile::TempDir::new().unwrap();
    let target = tmp.path().join("note.md");
    atomic_write(&target, "x").unwrap();
    let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    // Only the final file should remain — no `.tmp` artifacts
    assert_eq!(entries, vec!["note.md".to_string()]);
}
```

- [ ] **Step 2: Run tests, expect failures**

```bash
cargo test --lib vault::tests::atomic 2>&1 | tail -10
```

- [ ] **Step 3: Implement `atomic_write`**

```rust
fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pid = std::process::id();
    let tmp = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("write"),
        pid
    ));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```

- [ ] **Step 4: Run tests, expect pass**

```bash
cargo test --lib vault::tests::atomic 2>&1 | tail -10
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/core/vault.rs
git commit -m "feat(vault): atomic_write helper (tmp + fsync + rename)"
```

---

## Task 8: `VaultMirror::write` — sidecar only

**Files:**
- Modify: `src/core/vault.rs`
- Create: `tests/vault_mirror.rs`

Compose the formatters into the sidecar write. Daily-note append comes in Task 9; this task verifies sidecar shape end-to-end against a real tempdir filesystem.

- [ ] **Step 1: Implement `VaultMirror::write` sidecar half**

Replace the stub `VaultMirror::write` body. Note: `append_daily` is still `unimplemented!()` after this task — we're going to skip the daily call until Task 9 by leaving it commented for now.

```rust
impl VaultMirror {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn write(&self, item: &MirrorItem<'_>) -> Result<()> {
        // Sidecar at <root>/clipboard/YYYY-MM/<id>-<kind>-<slug>.md
        let month_folder = self.root
            .join("clipboard")
            .join(item.captured.format("%Y-%m").to_string());
        let sidecar_path = month_folder.join(format!("{}.md", slug_for_filename(item)));

        let content = format!("{}\n\n{}\n", format_frontmatter(item), format_body(item));
        atomic_write(&sidecar_path, &content)?;

        // TODO Task 9: append_daily(...)
        Ok(())
    }
}
```

- [ ] **Step 2: Add integration test file**

Create `tests/vault_mirror.rs`:

```rust
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
    let body = std::fs::read_to_string(
        tmp.path().join("clipboard/2026-05").join(&entries[0]),
    ).unwrap();
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
```

- [ ] **Step 3: Run integration tests**

```bash
cargo test --test vault_mirror 2>&1 | tail -15
```

Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/core/vault.rs tests/vault_mirror.rs
git commit -m "feat(vault): VaultMirror::write sidecar emit + integration test"
```

---

## Task 9: `append_daily` + complete `VaultMirror::write`

**Files:**
- Modify: `src/core/vault.rs`
- Modify: `tests/vault_mirror.rs`

Spec section 4.2: maintain one H2-bounded "Clipboard captures" section per daily file. Three states:

1. File doesn't exist → create with header + bullet.
2. File exists but no header → append fresh `## Clipboard captures · YYYY-MM-DD` section at end.
3. File exists with header → append bullet at end of that section, preserving anything before/after.

The bullet format (spec 4.2): `- HH:MM:SS [[<filename-without-ext>|<kind> from <source>]]`.

- [ ] **Step 1: Add `append_daily` unit tests in `src/core/vault.rs`**

Append to `mod tests`:

```rust
#[test]
fn daily_create_when_missing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("daily/2026-05-06.md");
    append_daily(&path, "- 04:32:03 [[142-url-foo|url from Safari]]").unwrap();
    let s = std::fs::read_to_string(&path).unwrap();
    assert!(s.contains("## Clipboard captures · 2026-05-06"));
    assert!(s.contains("- 04:32:03 [[142-url-foo|url from Safari]]"));
}

#[test]
fn daily_append_to_existing_section() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("daily/2026-05-06.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "# Today\n\n## Clipboard captures · 2026-05-06\n\n- 04:32:03 [[1-url-a|a]]\n\n## Other section\n\nstuff\n",
    ).unwrap();
    append_daily(&path, "- 04:32:05 [[2-json-b|b]]").unwrap();
    let s = std::fs::read_to_string(&path).unwrap();
    // Both bullets exist
    assert!(s.contains("- 04:32:03 [[1-url-a|a]]"));
    assert!(s.contains("- 04:32:05 [[2-json-b|b]]"));
    // The second bullet is inside the captures section (before "## Other section")
    let captures_idx = s.find("## Clipboard captures").unwrap();
    let other_idx = s.find("## Other section").unwrap();
    let new_bullet_idx = s.find("- 04:32:05").unwrap();
    assert!(captures_idx < new_bullet_idx);
    assert!(new_bullet_idx < other_idx);
    // Manual content preserved
    assert!(s.contains("# Today"));
    assert!(s.contains("stuff"));
}

#[test]
fn daily_append_when_file_exists_without_header() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("daily/2026-05-06.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "# Today\n\nMy notes.\n").unwrap();
    append_daily(&path, "- 04:32:03 [[142-url-foo|url from Safari]]").unwrap();
    let s = std::fs::read_to_string(&path).unwrap();
    assert!(s.contains("## Clipboard captures"));
    assert!(s.contains("My notes."));
    // The captures section comes after the existing content
    assert!(s.find("My notes.").unwrap() < s.find("## Clipboard captures").unwrap());
}
```

The `append_daily` test signature uses an `&str` second argument; the line caller assembles. The function determines the section header from the path's stem.

- [ ] **Step 2: Run tests, expect failures**

```bash
cargo test --lib vault::tests::daily 2>&1 | tail -10
```

- [ ] **Step 3: Implement `append_daily`**

Replace the stub:

```rust
fn append_daily(path: &Path, line: &str) -> Result<()> {
    use std::fmt::Write as _;

    // Parse the date from the filename stem (e.g. "2026-05-06").
    let date_label = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let header = format!("## Clipboard captures · {}", date_label);

    let existing = match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    let new_content = match existing {
        // Case 1: file doesn't exist
        None => format!("{}\n\n{}\n", header, line),

        // Case 3 first because it's the common path: section already there
        Some(s) if s.contains(&header) => {
            // Find the section, then find the next H2 (or end of file).
            // Insert `line` immediately before the next H2 (or at EOF), with
            // a leading newline if needed.
            let section_start = s.find(&header).unwrap();
            // Search for next "\n## " starting after the header line itself.
            let after_header = section_start + header.len();
            let next_h2 = s[after_header..]
                .find("\n## ")
                .map(|i| after_header + i + 1) // position of '#' of next header
                .unwrap_or(s.len());
            // Trim trailing whitespace inside the section so we don't pile up blank lines.
            let mut before = s[..next_h2].trim_end().to_string();
            let after = &s[next_h2..];
            let _ = writeln!(before, "\n{}", line);
            if after.is_empty() {
                before
            } else {
                format!("{}\n{}", before, after)
            }
        }

        // Case 2: file exists but no captures header — append a fresh section
        Some(s) => {
            let mut out = s;
            if !out.ends_with('\n') {
                out.push('\n');
            }
            let _ = writeln!(out, "\n{}\n\n{}", header, line);
            out
        }
    };

    atomic_write(path, &new_content)
}
```

- [ ] **Step 4: Run tests, expect pass**

```bash
cargo test --lib vault::tests::daily 2>&1 | tail -10
```

Expected: 3 tests pass.

- [ ] **Step 5: Wire daily call into `VaultMirror::write`**

Replace the body again to call `append_daily`:

```rust
pub fn write(&self, item: &MirrorItem<'_>) -> Result<()> {
    let slug = slug_for_filename(item);
    let month_folder = self.root
        .join("clipboard")
        .join(item.captured.format("%Y-%m").to_string());
    let sidecar_path = month_folder.join(format!("{}.md", slug));

    let content = format!("{}\n\n{}\n", format_frontmatter(item), format_body(item));
    atomic_write(&sidecar_path, &content)?;

    // Daily note bullet: `- HH:MM:SS [[<slug>|<kind> from <source>]]`
    let daily_path = self
        .root
        .join("daily")
        .join(format!("{}.md", item.captured.format("%Y-%m-%d")));
    let source = item.source_app.unwrap_or("unknown");
    let bullet = format!(
        "- {} [[{}|{} from {}]]",
        item.captured.format("%H:%M:%S"),
        slug,
        item.primary_kind,
        source
    );
    append_daily(&daily_path, &bullet)?;

    Ok(())
}
```

- [ ] **Step 6: Add integration test for combined sidecar + daily**

Append to `tests/vault_mirror.rs`:

```rust
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

    // Sidecar exists
    assert!(tmp.path().join("clipboard/2026-05/142-url-https-example-com.md").exists());
    // Daily note exists with the bullet referencing the sidecar slug
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
```

- [ ] **Step 7: Run all vault tests**

```bash
cargo test --lib vault 2>&1 | tail -5
cargo test --test vault_mirror 2>&1 | tail -10
```

Expected: all pass (~22 tests across unit + integration).

- [ ] **Step 8: Commit**

```bash
git add src/core/vault.rs tests/vault_mirror.rs
git commit -m "feat(vault): daily-note appender + complete VaultMirror::write"
```

---

# M4 — Daemon integration

## Task 10: Add `vault_mirror` to `WatcherOptions` + read env var in `main.rs`

**Files:**
- Modify: `src/daemon/watcher.rs` — `WatcherOptions` struct
- Modify: `src/main.rs` — env-var read at daemon startup

- [ ] **Step 1: Extend `WatcherOptions`**

In `src/daemon/watcher.rs`, find `pub struct WatcherOptions` (around line 14). Add the field and update `Default`:

```rust
pub struct WatcherOptions {
    pub poll_ms: u64,
    pub capture_window_title: bool,
    pub ignore_apps: Vec<String>,
    pub never_store_secrets: bool,
    pub max_items: i64,
    pub vault_mirror: Option<crate::core::vault::VaultMirror>,
}

impl Default for WatcherOptions {
    fn default() -> Self {
        Self {
            poll_ms: 1500,
            capture_window_title: false,
            ignore_apps: vec![],
            never_store_secrets: false,
            max_items: 1000,
            vault_mirror: None,
        }
    }
}
```

- [ ] **Step 2: Read env var in `main.rs`**

In `src/main.rs`, find the `WatcherOptions { ... }` literal in `run_daemon` (around line 58). Add the new field at the bottom of the literal:

```rust
let opts = WatcherOptions {
    poll_ms: std::env::var("CLIPBOARD_POLL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1500),
    capture_window_title: std::env::var("CLIPBOARD_CAPTURE_WINDOW_TITLE").as_deref()
        == Ok("1"),
    ignore_apps: std::env::var("CLIPBOARD_IGNORE_APPS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect(),
    never_store_secrets: std::env::var("CLIPBOARD_NEVER_STORE_SECRETS").as_deref()
        == Ok("1"),
    max_items: std::env::var("CLIPBOARD_HISTORY_MAX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000),
    vault_mirror: std::env::var("CLIPBOARD_VAULT_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            // Resolve relative paths against $HOME so users can pass `~/Vault`-ish.
            let p = std::path::PathBuf::from(&s);
            let resolved = if p.is_absolute() {
                p
            } else if let Ok(home) = std::env::var("HOME") {
                std::path::PathBuf::from(home).join(p)
            } else {
                p
            };
            clipboard_history_mcp::core::vault::VaultMirror::new(resolved)
        }),
};
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check 2>&1 | tail -5
```

Expected: clean compile.

- [ ] **Step 4: Commit**

```bash
git add src/daemon/watcher.rs src/main.rs
git commit -m "feat(daemon): wire CLIPBOARD_VAULT_PATH env into WatcherOptions"
```

---

## Task 11: Hook `VaultMirror::write` after `add_clip` in watcher

**Files:**
- Modify: `src/daemon/watcher.rs`

Spec section 5.2: insert the mirror call immediately after the existing `info!("clip captured: ...")` log line.

- [ ] **Step 1: Add the hook**

In `src/daemon/watcher.rs`, find the `else` branch where `add_clip` is called (around line 106-115). Modify it to capture the returned `id` and call the mirror:

```rust
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

    // Mirror to Obsidian vault if configured. Errors are logged + swallowed
    // so a vault problem (perm denied, disk full, sync race) never breaks
    // capture. Secrets do not reach this branch — they take add_secret above.
    if let Some(mirror) = &opts.vault_mirror {
        let item = crate::core::vault::MirrorItem {
            id,
            primary_kind: &cls.primary_kind,
            source_app: ctx.front_app.as_deref(),
            window_title: ctx.window_title.as_deref(),
            text: &text,
            captured: chrono::Local::now(),
        };
        if let Err(e) = mirror.write(&item) {
            warn!("vault mirror failed for clip {}: {}", id, e);
        }
    }
}
```

- [ ] **Step 2: Add `chrono` import at top of file**

The file already uses `tracing::{info, warn}`. Add at the top imports section:

```rust
// (no new top-level import needed — chrono::Local is referenced as a path)
```

If you prefer the import path, add `use chrono::Local;` and write `Local::now()`. Either works.

- [ ] **Step 3: Run cargo check**

```bash
cargo check 2>&1 | tail -5
```

Expected: clean compile.

- [ ] **Step 4: Add watcher integration test**

Append to `tests/vault_mirror.rs`:

```rust
//! End-to-end watcher → vault mirror test.
//!
//! Constructs a Store + VaultMirror in tempdirs, calls add_clip, then writes
//! through the mirror — verifying the same shape the daemon would produce.
//!
//! Doesn't actually start the watcher loop (that needs a real pasteboard).
//! The hook in watcher.rs is small enough that a smoke test of the wiring
//! is covered by the existing `clipboard_smoke` integration tests.

#[test]
fn store_then_mirror_writes_files() {
    use clipboard_history_mcp::core::store::{ClipInput, Store};

    let tmp_db = tempfile::TempDir::new().unwrap();
    let tmp_vault = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp_db.path().join("t.db"), [0u8; 32]).unwrap();
    let mirror = VaultMirror::new(tmp_vault.path().to_path_buf());

    let id = store.add_clip(ClipInput {
        text: "https://example.com".into(),
        primary_kind: "url".into(),
        kinds: vec!["url".into()],
        source_app: Some("Safari".into()),
        window_title: None,
    }).unwrap();

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
```

- [ ] **Step 5: Run all integration tests**

```bash
cargo test --test vault_mirror 2>&1 | tail -10
cargo test 2>&1 | tail -10
```

Expected: all vault tests pass; full suite remains green.

- [ ] **Step 6: Commit**

```bash
git add src/daemon/watcher.rs tests/vault_mirror.rs
git commit -m "feat(daemon): mirror non-secret clips to vault after add_clip"
```

---

# M5 — Install integration

## Task 12: Add `--vault PATH` to `InstallOpts` + clap parser

**Files:**
- Modify: `src/cli/install.rs` — `InstallOpts` struct
- Modify: `src/main.rs` — clap subcommand argument

- [ ] **Step 1: Extend `InstallOpts`**

In `src/cli/install.rs`:

```rust
use std::path::PathBuf;

pub struct InstallOpts {
    pub window_titles: bool,
    pub linger: bool,
    pub vault: Option<PathBuf>,
}
```

- [ ] **Step 2: Add the clap argument**

Find the `Install` variant of the command enum in `src/main.rs` (search for `Install` case in the match). It's likely a `clap::Subcommand` or args struct. Add a new field:

```rust
// Inside the Install args struct (clap-derived)
/// Auto-mirror non-secret clips to this Obsidian vault path.
#[arg(long, value_name = "PATH")]
vault: Option<std::path::PathBuf>,
```

Then in the match arm where `InstallOpts { ... }` is constructed, forward the value:

```rust
InstallOpts {
    window_titles,
    linger,
    vault,
}
```

If you can't find the exact spot quickly, run:

```bash
grep -n "InstallOpts" src/main.rs
```

- [ ] **Step 3: Cargo check**

```bash
cargo check 2>&1 | tail -5
```

Expected: clean compile.

- [ ] **Step 4: Commit**

```bash
git add src/cli/install.rs src/main.rs
git commit -m "feat(cli): --vault PATH flag on install subcommand"
```

---

## Task 13: Inject `CLIPBOARD_VAULT_PATH` into launchd plist (macOS)

**Files:**
- Modify: `src/cli/install_macos.rs`

The existing `install_macos.rs` builds an `env_dict` string then template-substitutes into `__ENV_DICT__`. We extend the string when a vault path is configured.

- [ ] **Step 1: Extend `env_dict` construction**

Find the existing block in `install_macos.rs` (around lines 17-22):

```rust
let env_dict = format!(
    r#"    <key>CLIPBOARD_CAPTURE_WINDOW_TITLE</key>
    <string>{}</string>"#,
    if opts.window_titles { "1" } else { "0" }
);
```

Replace it with a `let mut` + conditional append:

```rust
let mut env_dict = format!(
    r#"    <key>CLIPBOARD_CAPTURE_WINDOW_TITLE</key>
    <string>{}</string>"#,
    if opts.window_titles { "1" } else { "0" }
);
if let Some(vault) = &opts.vault {
    env_dict.push_str(&format!(
        "\n    <key>CLIPBOARD_VAULT_PATH</key>\n    <string>{}</string>",
        vault.display()
    ));
}
```

- [ ] **Step 2: Cargo check on macOS**

```bash
cargo check 2>&1 | tail -5
```

Expected: clean compile (Linux contributors: `#[cfg(target_os = "macos")]` keeps this code unbuilt for you; CI matrix tests both).

- [ ] **Step 3: Manual smoke test of the plist format**

We don't have a unit test for plist generation (it'd duplicate the template). Run a one-off:

```bash
# (only on macOS — skip on Linux)
cargo run --release -- install --vault /tmp/fake-vault --window-titles
cat ~/Library/LaunchAgents/me.kz.clipboard-history-rs.plist | grep -A1 CLIPBOARD_VAULT_PATH
```

Expected output: a `<key>CLIPBOARD_VAULT_PATH</key>` followed by `<string>/tmp/fake-vault</string>`.

Then immediately uninstall to revert:

```bash
cargo run --release -- uninstall --keep-data
```

- [ ] **Step 4: Commit**

```bash
git add src/cli/install_macos.rs
git commit -m "feat(cli): inject CLIPBOARD_VAULT_PATH into launchd plist when --vault set"
```

---

## Task 14: Inject `CLIPBOARD_VAULT_PATH` into systemd unit (Linux)

**Files:**
- Modify: `scripts/systemd.service.template`
- Modify: `src/cli/install_linux.rs`

The systemd template currently has a single `Environment=` line for `CLIPBOARD_CAPTURE_WINDOW_TITLE`. We add a second placeholder line that gets replaced with either the vault env line or the empty string.

- [ ] **Step 1: Add placeholder to template**

Open `scripts/systemd.service.template` and append `__VAULT_ENV_LINE__` after the existing `Environment=` line:

```
[Unit]
Description=clipboard-history-mcp watcher daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=__BINARY__ daemon
Restart=on-failure
RestartSec=5
Environment="CLIPBOARD_CAPTURE_WINDOW_TITLE=__CAPTURE_WINDOW_TITLE__"
__VAULT_ENV_LINE__
StandardOutput=append:__LOG__
StandardError=append:__LOG__

[Install]
WantedBy=default.target
```

When no vault is configured, the placeholder gets replaced with `""` (empty string), which leaves a blank line. systemd ignores blank lines.

- [ ] **Step 2: Wire substitution in `install_linux.rs`**

In `src/cli/install_linux.rs`, find the `let unit = template.replace(...)` chain (around line 21). Add the vault substitution:

```rust
let vault_env_line = match &opts.vault {
    Some(p) => format!(r#"Environment="CLIPBOARD_VAULT_PATH={}""#, p.display()),
    None => String::new(),
};
let unit = template
    .replace("__BINARY__", bin.to_str().unwrap())
    .replace("__LOG__", log.to_str().unwrap())
    .replace(
        "__CAPTURE_WINDOW_TITLE__",
        if opts.window_titles { "1" } else { "0" },
    )
    .replace("__VAULT_ENV_LINE__", &vault_env_line);
```

- [ ] **Step 3: Cargo check**

```bash
cargo check 2>&1 | tail -5
```

Expected: clean compile. (On macOS, `install_linux.rs` is `#[cfg(target_os = "linux")]`-gated and won't build, so this is satisfied trivially. CI matrix verifies on Linux.)

- [ ] **Step 4: Smoke test on Linux only**

If you're on Linux:

```bash
cargo run --release -- install --vault /tmp/fake-vault
cat ~/.config/systemd/user/clipboard-history-mcp.service | grep CLIPBOARD_VAULT_PATH
cargo run --release -- uninstall --keep-data
```

Expected: an `Environment="CLIPBOARD_VAULT_PATH=/tmp/fake-vault"` line.

- [ ] **Step 5: Commit**

```bash
git add scripts/systemd.service.template src/cli/install_linux.rs
git commit -m "feat(cli): inject CLIPBOARD_VAULT_PATH into systemd unit when --vault set"
```

---

# M6 — Docs and ship

## Task 15: README — Configuration table + install example

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add `CLIPBOARD_VAULT_PATH` row to the Configuration table**

Find the existing Configuration table (search for `CLIPBOARD_POLL_MS`). Add a new row, slotted alphabetically:

```md
| `CLIPBOARD_VAULT_PATH` | *(empty)* | Auto-mirror non-secret clips to this Obsidian vault directory (sidecar `.md` per clip + bullet in `daily/YYYY-MM-DD.md`). Set via `--vault PATH` at install time. |
```

- [ ] **Step 2: Add a vault-mirror snippet under the Install section**

Find the macOS install section (look for `### One-click — Claude Desktop`). After the existing manual-install example, add a new subsection:

```md
### Auto-mirror to Obsidian vault (optional)

If you also use Obsidian, daemon can write a sidecar `.md` for every captured clip plus a daily-note timeline entry. Configure at install time:

\`\`\`bash
clipboard-history-mcp install --vault ~/Documents/Obsidian/MyVault
\`\`\`

Result: every non-secret clip lands as `<vault>/clipboard/YYYY-MM/<id>-<kind>-<slug>.md` with frontmatter (kind, source, captured-at), and a bullet appended to `<vault>/daily/YYYY-MM-DD.md` linking back to the sidecar. Secrets are **never** mirrored — they stay encrypted in the SQLite vault.

Edit-bot disclaimer: the daemon owns the `## Clipboard captures` H2 section in your daily notes — append-only, idempotent. You can write anything else above or below it.
```

(Use real backticks in the actual file — escaped here for readability.)

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README entry for CLIPBOARD_VAULT_PATH + Obsidian mirror"
```

---

## Task 16: CHANGELOG + version bump

**Files:**
- Modify: `Cargo.toml` — version
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Bump version**

In `Cargo.toml`:

```toml
version = "0.5.0-alpha.0"
```

- [ ] **Step 2: Add CHANGELOG entry**

At the top of `CHANGELOG.md` (above the v0.4.0-alpha.0 entry), add:

```md
## v0.5.0-alpha.0 — 2026-05-06

### Added
- **Obsidian vault mirror** — daemon can auto-export every non-secret clip to a configured Obsidian vault. Sidecar `.md` files under `<vault>/clipboard/YYYY-MM/<id>-<kind>-<slug>.md` plus daily-note bullets in `<vault>/daily/YYYY-MM-DD.md`. Configure with `--vault PATH` at install time, or `CLIPBOARD_VAULT_PATH=...` env var. Secrets are never mirrored.
- New env var: `CLIPBOARD_VAULT_PATH`
- New CLI flag: `--vault PATH` on `install`

### Changed
- `chrono` 0.4 added as a dep (lean: `clock + std` features only).

### Notes
- Vault writes are atomic (tmp + fsync + rename) to avoid Obsidian iCloud / Sync race conditions.
- Bidirectional sync, retroactive backfill, custom templates: out of scope. See [vault-mirror design spec](docs/superpowers/specs/2026-05-06-vault-mirror-design.md).
```

- [ ] **Step 3: Run full test suite once more**

```bash
cargo test 2>&1 | tail -10
cargo build --release 2>&1 | tail -5
```

Expected: all tests pass, release build succeeds.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(release): bump to v0.5.0-alpha.0 + CHANGELOG"
```

---

## Task 17: Manual smoke test against a real Obsidian vault

**Files:** None (verification only)

This task is human-driven and not committable. Run after Task 16 to confirm end-to-end behaviour outside the test harness.

- [ ] **Step 1: Build and install pointing at a real vault**

```bash
cargo build --release
./target/release/clipboard-history-mcp install --vault ~/dev/ohnoma --window-titles
```

(Substitute your actual Obsidian vault path. If you don't want to write into your real vault for this test, use `~/tmp/test-vault` — daemon creates it lazily.)

- [ ] **Step 2: Verify daemon picked up the env**

```bash
# macOS
launchctl print gui/$(id -u)/me.kz.clipboard-history-rs | grep CLIPBOARD_VAULT_PATH

# Linux
systemctl --user show clipboard-history-mcp.service | grep CLIPBOARD_VAULT_PATH
```

Expected: a single line showing your vault path.

- [ ] **Step 3: Make some clipboard captures**

In any apps:

1. Copy a URL (browser address bar)
2. Copy a JSON snippet (from any IDE)
3. Copy a Python def (or any code starting with `def`/`fn`/etc)
4. Copy something that **looks like** an OpenAI API key — `sk-proj-` followed by random characters. The exact regex is in `vendor/gitleaks.toml`; `sk-proj-` followed by 40+ alphanumerics is a reliable trigger.

- [ ] **Step 4: Inspect the vault**

```bash
ls ~/dev/ohnoma/clipboard/2026-05/
cat ~/dev/ohnoma/daily/$(date +%Y-%m-%d).md
```

Expected:
- 3 sidecar files (URL, JSON, code) — **not 4**. The OpenAI key didn't mirror.
- Daily note has 3 bullets, all under one `## Clipboard captures · YYYY-MM-DD` section.
- Open the daily note in Obsidian: backlinks panel shows 3 incoming links.

- [ ] **Step 5: Verify secret was still captured to SQLite**

```bash
./target/release/clipboard-history-mcp vault list | grep openai
```

Expected: one row showing the openai_api_key secret in the encrypted vault. Confirms: secret was caught and stored, just not mirrored.

- [ ] **Step 6: Uninstall to revert**

```bash
./target/release/clipboard-history-mcp uninstall --keep-data
```

The vault directory is left intact (we only own `clipboard/` and the H2 section in daily notes — not delete-on-uninstall material).

- [ ] **Step 7: Annotate the spec with any anomalies you found**

If anything didn't match the spec, edit `docs/superpowers/specs/2026-05-06-vault-mirror-design.md` with a "Real-world deltas" section at the bottom. Commit it as `docs(spec): vault mirror real-world deltas` if non-empty, otherwise skip.

---

## Done

After Task 17 you have:

- A working vault-mirror feature on `feat/vault-mirror` (~17 commits).
- All tests green: ~16 unit tests in `vault.rs`, ~5 integration tests in `tests/vault_mirror.rs`, plus the existing suite untouched.
- Release-ready v0.5.0-alpha.0 with version bump + CHANGELOG.
- Manual verification against a real vault.

**To ship:**

```bash
git push origin feat/vault-mirror
gh pr create --title "feat: Obsidian vault auto-mirror" \
  --body "Implements docs/superpowers/specs/2026-05-06-vault-mirror-design.md. Closes the loop between clipboard-history-mcp and the user's PKM vault. Secrets never leave the encrypted SQLite store."
```

After PR merge to `main`, tag and release as `v0.5.0-alpha.0`:

```bash
git checkout main && git pull
git tag -a v0.5.0-alpha.0 -m "v0.5.0-alpha.0 — Obsidian vault mirror"
git push origin v0.5.0-alpha.0
```

`.github/workflows/release.yml` rebuilds the `.mcpb` artifact and uploads to the GitHub release.
