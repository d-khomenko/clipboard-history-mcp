# clipboard-history-mcp v3 (Rust) — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace v2 (Node.js + Swift helper) with a single self-contained Rust binary that captures macOS clipboard history into SQLite + FTS5, encrypts secrets via Keychain-backed AES-256-GCM with biometric ACL (Touch ID), and exposes 15 MCP tools — packaged as `.mcpb` for one-click install in Claude Desktop.

**Architecture:** One Rust binary, multiple subcommands (`daemon`, `serve`, `install`, `uninstall`, `status`, `vault`, `doctor`, `migrate-v2`). The watcher runs sync on a pinned thread; the MCP server uses `rmcp` over `tokio` stdio. Both processes share a single SQLite (WAL mode) database file.

**Tech Stack:** Rust 1.95+, `rmcp` 1.6 (MCP), `objc2` + `objc2-app-kit` (NSPasteboard), `objc2-local-authentication` (Touch ID), `accessibility` 0.1 (window titles), `security-framework` 3.7 (Keychain + biometric ACL), `rusqlite` 0.39 with bundled FTS5, `aes-gcm` 0.10, `clap` 4.6 (CLI), `tracing` 0.1 (logging), `toml` 1.1 (gitleaks rule loader).

**Reference spec:** [`docs/superpowers/specs/2026-05-05-clipboard-history-mcp-v3-rust-design.md`](../specs/2026-05-05-clipboard-history-mcp-v3-rust-design.md)

**Important:** crate APIs evolve. Before writing code that touches `rmcp`, `objc2-*`, or `security-framework`, agents should fetch the current crates.io docs URL to confirm method signatures. The plan shows an example shape, not a guaranteed-current API.

---

## File map (locked in)

```
src/
├── main.rs               # binary entry; clap subcommand dispatch
├── lib.rs                # re-exports for tests
├── core/
│   ├── mod.rs
│   ├── db.rs             # SQLite open + WAL + migrations + FTS5
│   ├── store.rs          # Store struct: addClip, addSecret, list, search, etc.
│   ├── crypto.rs         # AES-GCM + Keychain (biometric ACL)
│   ├── types.rs          # type classifier (URL/JSON/SQL/code:*/...)
│   ├── secrets.rs        # gitleaks rules loader + Luhn + JWT
│   ├── pasteboard.rs     # NSPasteboard read/write/types via objc2
│   └── biometry.rs       # LAContext + cached unlock
├── daemon/
│   ├── mod.rs
│   ├── watcher.rs        # poll loop on pinned thread
│   └── context.rs        # frontmost app + window title
├── mcp/
│   ├── mod.rs            # rmcp service entry
│   └── tools.rs          # all 15 tools as one impl block
└── cli/
    ├── mod.rs
    ├── install.rs        # write launchd plist + load
    ├── uninstall.rs
    ├── status.rs
    ├── vault.rs
    ├── doctor.rs
    └── migrate_v2.rs

vendor/gitleaks.toml      # carry over from v2

mcpb/
├── manifest.json
└── server/               # populated at build time

scripts/
├── pack-mcpb.sh
├── launchd.plist.template
└── refresh-gitleaks.sh

tests/
├── store.rs
├── types.rs
├── secrets.rs
└── crypto.rs

.github/workflows/{ci.yml,release.yml}
README.md
CHANGELOG.md
LICENSE                  (carry over)
```

---

## Milestones

| # | Range | Outcome |
|---|---|---|
| **M1** | Tasks 1–8 | Cargo project + core modules (db, store, crypto, types, secrets) all unit-tested |
| **M2** | Tasks 9–13 | Pasteboard + context capture + watcher + daemon binary; manual smoke test captures clips |
| **M3** | Tasks 14–17 | MCP server with 15 tools; Claude Desktop talks to the binary |
| **M4** | Tasks 18–22 | CLI subcommands + Touch ID gate on `unlock_secret` |
| **M5** | Tasks 23–26 | MCPB packaging + GitHub Actions + tag v0.3.0-alpha.0 |

---

# M1 — Foundation

## Task 1: Cargo project + workspace setup

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`, `src/lib.rs`
- Create: `.cargo/config.toml`
- Modify: `.gitignore`

- [ ] **Step 1: Create `Cargo.toml`**

```toml
[package]
name = "clipboard-history-mcp"
version = "0.3.0-alpha.0"
edition = "2021"
rust-version = "1.95"
description = "Type-aware, secret-safe macOS clipboard history exposed to Claude via MCP."
license = "MIT"
authors = ["Dmytro Khomenko"]
repository = "https://github.com/d-khomenko/clipboard-history-mcp"

[lib]
path = "src/lib.rs"

[[bin]]
name = "clipboard-history-mcp"
path = "src/main.rs"

[dependencies]
rmcp                       = { version = "1.6", features = ["server", "macros", "transport-io"] }
tokio                      = { version = "1.52", features = ["full"] }
objc2                      = "0.6"
objc2-app-kit              = { version = "0.3", features = ["NSPasteboard", "NSWorkspace", "NSRunningApplication"] }
objc2-foundation           = "0.3"
objc2-local-authentication = "0.3"
security-framework         = { version = "3.7", features = ["OSX_10_15"] }
security-framework-sys     = "2.12"
accessibility              = "0.1"
rusqlite                   = { version = "0.39", features = ["bundled"] }
aes-gcm                    = "0.10"
serde                      = { version = "1", features = ["derive"] }
serde_json                 = "1"
toml                       = "1.1"
clap                       = { version = "4.6", features = ["derive"] }
tracing                    = "0.1"
tracing-subscriber         = { version = "0.3", features = ["env-filter"] }
anyhow                     = "1"
thiserror                  = "2"
schemars                   = "0.9"
sha2                       = "0.10"
uuid                       = { version = "1", features = ["v4"] }
hex                        = "0.4"

[dev-dependencies]
tempfile = "3"

[profile.release]
opt-level = 3
lto = "fat"
strip = "symbols"
codegen-units = 1
```

- [ ] **Step 2: Create `src/main.rs` placeholder**

```rust
fn main() {
    println!("clipboard-history-mcp v0.3.0-alpha.0");
}
```

- [ ] **Step 3: Create `src/lib.rs`**

```rust
pub mod core;
pub mod cli;
pub mod daemon;
pub mod mcp;
```

- [ ] **Step 4: Create `.cargo/config.toml`**

```toml
[build]
target-dir = "target"
```

- [ ] **Step 5: Update `.gitignore`** (append, don't replace; v2 entries stay):

```
/target/
Cargo.lock.bak
```

- [ ] **Step 6: Verify it builds**

```bash
. "$HOME/.cargo/env"
cargo build 2>&1 | tail -10
```

Expected: builds successfully (will fail because `core`/`cli`/`daemon`/`mcp` modules don't exist yet — fine for now, replace `lib.rs` with `pub fn placeholder() {}` to make it build).

Adjust `src/lib.rs` to:
```rust
pub fn placeholder() {}
```

Re-run `cargo build`. Expected: clean build, ~3 minutes for first compile of all deps.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/lib.rs .cargo/config.toml .gitignore
git commit -m "chore: bootstrap Rust project with all v3 deps"
```

---

## Task 2: Carry over `vendor/gitleaks.toml`

The v2 vendored TOML works as-is for v3.

- [ ] **Step 1: Verify file exists**

```bash
ls -la vendor/gitleaks.toml
```

If it doesn't (because we're on a fresh branch), recreate via:

```bash
mkdir -p vendor
curl -fsSL https://raw.githubusercontent.com/gitleaks/gitleaks/master/config/gitleaks.toml -o vendor/gitleaks.toml
```

- [ ] **Step 2: No commit needed** if file already in repo from v2 history. If you re-downloaded, commit:

```bash
git add vendor/gitleaks.toml
git commit -m "chore: vendor gitleaks rules"
```

---

## Task 3: `core::crypto` — AES-256-GCM + Keychain

**Files:**
- Create: `src/core/mod.rs`
- Create: `src/core/crypto.rs`
- Create: `tests/crypto.rs`

- [ ] **Step 1: Create `src/core/mod.rs`**

```rust
pub mod crypto;
pub mod db;
pub mod pasteboard;
pub mod secrets;
pub mod store;
pub mod types;
pub mod biometry;
```

(Modules will be filled in subsequent tasks; commit each as it lands.)

- [ ] **Step 2: Create empty stub modules so `lib.rs` builds**

For `db.rs`, `pasteboard.rs`, `secrets.rs`, `store.rs`, `types.rs`, `biometry.rs` — empty file or `// stub`. Add later.

- [ ] **Step 3: Update `src/lib.rs`**

```rust
pub mod core;
pub mod cli;
pub mod daemon;
pub mod mcp;
```

Create empty `src/cli/mod.rs`, `src/daemon/mod.rs`, `src/mcp/mod.rs` as well.

- [ ] **Step 4: Write failing test `tests/crypto.rs`**

```rust
use clipboard_history_mcp::core::crypto::{encrypt, decrypt};

#[test]
fn roundtrip_utf8() {
    let key = [0u8; 32];
    let plaintext = "sk-proj-abcXYZ123";
    let (ciphertext, nonce) = encrypt(plaintext, &key);
    assert_eq!(nonce.len(), 12);
    assert!(ciphertext.len() > plaintext.len());
    let recovered = decrypt(&ciphertext, &nonce, &key).unwrap();
    assert_eq!(recovered, plaintext);
}

#[test]
fn rejects_tampered_ciphertext() {
    let key = [0u8; 32];
    let (mut ct, nonce) = encrypt("hello", &key);
    ct[0] ^= 0xFF;
    assert!(decrypt(&ct, &nonce, &key).is_err());
}

#[test]
fn rejects_wrong_key() {
    let k1 = [0u8; 32];
    let k2 = [1u8; 32];
    let (ct, nonce) = encrypt("hello", &k1);
    assert!(decrypt(&ct, &nonce, &k2).is_err());
}
```

Run: `cargo test --test crypto`
Expected: FAIL — `core::crypto::encrypt` not found.

- [ ] **Step 5: Implement `src/core/crypto.rs`**

```rust
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use rand::RngCore;
use security_framework::passwords::{set_generic_password, get_generic_password};

const KEYCHAIN_SERVICE: &str = "clipboard-history-mcp";
const KEYCHAIN_ACCOUNT: &str = "master-key-v1";

pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> (Vec<u8>, [u8; 12]) {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let n = Nonce::from_slice(&nonce);
    let ct = cipher.encrypt(n, plaintext.as_bytes()).expect("encrypt");
    (ct, nonce)
}

pub fn decrypt(ciphertext: &[u8], nonce: &[u8], key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let n = Nonce::from_slice(nonce);
    let pt = cipher.decrypt(n, ciphertext).map_err(|_| anyhow!("AEAD authentication failed"))?;
    Ok(String::from_utf8(pt)?)
}

pub fn get_or_create_master_key() -> Result<[u8; 32]> {
    if let Ok(bytes) = get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        if bytes.len() == 32 {
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            return Ok(k);
        }
    }
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, &k)
        .map_err(|e| anyhow!("Keychain write failed: {}", e))?;
    Ok(k)
}
```

Add to `Cargo.toml` `[dependencies]`:
```toml
rand = "0.8"
```

- [ ] **Step 6: Run tests**

```bash
cargo test --test crypto
```

Expected: 3 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/core/ src/lib.rs src/main.rs src/cli/ src/daemon/ src/mcp/ tests/crypto.rs Cargo.toml Cargo.lock
git commit -m "feat(core): AES-256-GCM crypto with Keychain-backed master key"
```

---

## Task 4: `core::types` — type classifier

**Files:**
- Create: `src/core/types.rs`
- Create: `tests/types.rs`

- [ ] **Step 1: Failing test `tests/types.rs`**

```rust
use clipboard_history_mcp::core::types::classify;

#[test]
fn detects_url() {
    let r = classify("https://api.openai.com/v1");
    assert_eq!(r.primary_kind, "url");
    assert!(r.kinds.contains(&"url".to_string()));
}

#[test]
fn detects_email() {
    assert_eq!(classify("hello@example.com").primary_kind, "email");
}

#[test]
fn detects_json() {
    assert_eq!(classify(r#"{"a":1}"#).primary_kind, "json");
}

#[test]
fn detects_sql() {
    assert_eq!(classify("SELECT * FROM users").primary_kind, "sql");
}

#[test]
fn detects_shell() {
    assert_eq!(classify("git checkout main").primary_kind, "shell");
}

#[test]
fn detects_python_code() {
    let r = classify("def foo():\n    return 42\n");
    assert!(r.primary_kind.starts_with("code:"));
    assert_eq!(r.code.unwrap().language, "python");
}

#[test]
fn falls_back_to_text() {
    assert_eq!(classify("just words").primary_kind, "text");
}
```

Run: `cargo test --test types`. Expected: FAIL.

- [ ] **Step 2: Implement `src/core/types.rs`**

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CodeInfo { pub language: String }

#[derive(Debug, Serialize)]
pub struct Classification {
    pub primary_kind: String,
    pub kinds: Vec<String>,
    pub code: Option<CodeInfo>,
}

pub fn classify(text: &str) -> Classification {
    if text.is_empty() {
        return Classification { primary_kind: "text".into(), kinds: vec!["text".into()], code: None };
    }

    let mut kinds: Vec<String> = Vec::new();

    if is_full_url(text) { kinds.push("url".into()); }
    else if has_url(text) { kinds.push("url".into()); }
    if is_email(text) { kinds.push("email".into()); }
    if is_json(text) { kinds.push("json".into()); }
    if is_sql(text) { kinds.push("sql".into()); }
    if is_shell(text) { kinds.push("shell".into()); }

    let mut code = None;
    if let Some(lang) = detect_language(text) {
        kinds.push(format!("code:{}", lang));
        code = Some(CodeInfo { language: lang });
    }

    let primary_kind = pick_primary(&kinds, text);
    if kinds.is_empty() { kinds.push("text".into()); }

    Classification { primary_kind, kinds, code }
}

fn is_full_url(t: &str) -> bool {
    let t = t.trim();
    (t.starts_with("http://") || t.starts_with("https://")) && !t.contains(char::is_whitespace)
}
fn has_url(t: &str) -> bool { t.contains("http://") || t.contains("https://") }
fn is_email(t: &str) -> bool {
    let t = t.trim();
    t.contains('@') && t.matches('@').count() == 1 && {
        let (l, r) = t.split_once('@').unwrap();
        !l.is_empty() && r.contains('.') && !r.contains(char::is_whitespace) && !l.contains(char::is_whitespace)
    }
}
fn is_json(t: &str) -> bool {
    let t = t.trim();
    if !(t.starts_with('{') || t.starts_with('[')) { return false; }
    serde_json::from_str::<serde_json::Value>(t).is_ok()
}
fn is_sql(t: &str) -> bool {
    let head = t.trim_start().to_uppercase();
    head.starts_with("SELECT ") || head.starts_with("INSERT ") || head.starts_with("UPDATE ")
        || head.starts_with("DELETE ") || head.starts_with("CREATE ") || head.starts_with("ALTER ")
        || head.starts_with("DROP ") || head.starts_with("WITH ")
}
fn is_shell(t: &str) -> bool {
    let head = t.trim_start();
    if head.starts_with('$') { return true; }
    let first = head.split_whitespace().next().unwrap_or("");
    matches!(first, "sudo"|"cd"|"ls"|"git"|"npm"|"pnpm"|"yarn"|"brew"|"docker"|"kubectl"
        |"curl"|"wget"|"cargo"|"go"|"python"|"pip"|"node"|"make"|"bash"|"zsh"|"sh")
}

fn pick_primary(kinds: &[String], _text: &str) -> String {
    for k in kinds { if k == "json" { return "json".into(); } }
    for k in kinds { if k == "sql" { return "sql".into(); } }
    for k in kinds { if k == "shell" { return "shell".into(); } }
    for k in kinds { if k == "email" { return "email".into(); } }
    for k in kinds { if k.starts_with("code:") { return k.clone(); } }
    for k in kinds { if k == "url" { return "url".into(); } }
    "text".into()
}

fn detect_language(text: &str) -> Option<String> {
    if text.starts_with("#!") {
        let first_line = text.lines().next()?;
        if first_line.contains("python") { return Some("python".into()); }
        if first_line.contains("bash") || first_line.contains("zsh") || first_line.contains("/sh") { return None; }
        if first_line.contains("node") { return Some("javascript".into()); }
        if first_line.contains("ruby") { return Some("ruby".into()); }
    }
    let lower = text.to_ascii_lowercase();
    let scores = [
        ("python", count(&lower, &["def ", "import ", "self.", "elif ", "from ", "lambda "])),
        ("javascript", count(&lower, &["function ", "const ", "let ", "=>", "console.log", "require(", "import {"])),
        ("typescript", count(&lower, &[": string", ": number", "interface ", "type ", "enum "])),
        ("rust", count(&lower, &["fn ", "let mut", "impl ", "pub fn", "match ", "::<"])),
        ("go", count(&lower, &["package ", "func ", ":= ", "import (", "interface {"])),
        ("java", count(&lower, &["public class", "private ", "protected ", "static void"])),
        ("html", count(&lower, &["<html", "<div", "<span", "</p>", "<!doctype"])),
        ("css", count(&lower, &["{ ", "} ", "padding:", "margin:", "color:", "display:"])),
    ];
    let (lang, top) = scores.iter().max_by_key(|(_, n)| *n)?;
    if *top >= 2 { Some((*lang).into()) } else { None }
}

fn count(text: &str, needles: &[&str]) -> usize {
    needles.iter().map(|n| text.matches(n).count()).sum()
}
```

- [ ] **Step 3: Add `serde_json` already-listed dep, ensure `serde` derive, run tests**

```bash
cargo test --test types
```

Expected: 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/core/types.rs tests/types.rs
git commit -m "feat(core): type classifier (url/email/json/sql/shell/code:lang heuristic)"
```

---

## Task 5: `core::secrets` — gitleaks loader + JWT + Luhn

**Files:**
- Create: `src/core/secrets.rs`
- Create: `tests/secrets.rs`

- [ ] **Step 1: Failing test `tests/secrets.rs`**

```rust
use clipboard_history_mcp::core::secrets::detect_secret;

#[test]
fn detects_openai() {
    let key = format!("sk-{}T3BlbkFJ{}", "A".repeat(20), "B".repeat(20));
    let r = detect_secret(&key).expect("should detect");
    assert!(r.kind.contains("openai") || r.kind.contains("OpenAI"));
    assert!(r.value.starts_with("sk-"));
}

#[test]
fn detects_jwt() {
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abc-_def123";
    let r = detect_secret(jwt).expect("jwt");
    assert_eq!(r.kind, "jwt");
}

#[test]
fn detects_credit_card_luhn_valid() {
    let r = detect_secret("4242 4242 4242 4242").expect("cc");
    assert_eq!(r.kind, "credit_card");
}

#[test]
fn rejects_credit_card_luhn_invalid() {
    let r = detect_secret("1234 5678 9012 3456");
    if let Some(hit) = r {
        assert_ne!(hit.kind, "credit_card");
    }
}

#[test]
fn returns_none_for_plain_text() {
    assert!(detect_secret("just hello world").is_none());
}
```

Run: `cargo test --test secrets`. Expected: FAIL.

- [ ] **Step 2: Implement `src/core/secrets.rs`**

```rust
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::sync::Mutex;

#[derive(Debug)]
pub struct SecretHit {
    pub kind: String,
    pub value: String,
    pub last_chars: String,
}

#[derive(Debug, Deserialize)]
struct GitleaksConfig {
    rules: Option<Vec<RawRule>>,
}

#[derive(Debug, Deserialize)]
struct RawRule {
    id: String,
    regex: Option<String>,
}

struct CompiledRule { kind: String, re: Regex }

static RULES: Lazy<Mutex<Vec<CompiledRule>>> = Lazy::new(|| {
    let raw = include_str!("../../vendor/gitleaks.toml");
    let cfg: GitleaksConfig = toml::from_str(raw).unwrap_or(GitleaksConfig { rules: None });
    let mut out: Vec<CompiledRule> = cfg.rules.unwrap_or_default().into_iter()
        .filter_map(|r| {
            let pattern = r.regex.as_deref()?;
            let re = Regex::new(pattern).ok()?;
            Some(CompiledRule { kind: r.id, re })
        })
        .collect();
    out.push(CompiledRule {
        kind: "jwt".into(),
        re: Regex::new(r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap(),
    });
    Mutex::new(out)
});

pub fn detect_secret(text: &str) -> Option<SecretHit> {
    if text.is_empty() { return None; }
    let rules = RULES.lock().ok()?;
    for rule in rules.iter() {
        if let Some(m) = rule.re.find(text) {
            let value = m.as_str().to_string();
            let last_chars = value.chars().rev().take(6).collect::<String>().chars().rev().collect();
            return Some(SecretHit { kind: rule.kind.clone(), value, last_chars });
        }
    }
    drop(rules);
    detect_credit_card(text)
}

fn detect_credit_card(text: &str) -> Option<SecretHit> {
    let cc_re = Regex::new(r"\b(?:\d[ -]?){13,19}\b").ok()?;
    let m = cc_re.find(text)?;
    let raw = m.as_str();
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if !(13..=19).contains(&digits.len()) || !luhn(&digits) { return None; }
    let last_chars = digits.chars().rev().take(4).collect::<String>().chars().rev().collect();
    Some(SecretHit { kind: "credit_card".into(), value: digits, last_chars })
}

fn luhn(s: &str) -> bool {
    let mut sum = 0u32;
    let mut alt = false;
    for c in s.chars().rev() {
        let mut n = c.to_digit(10).unwrap_or(0);
        if alt { n *= 2; if n > 9 { n -= 9; } }
        sum += n;
        alt = !alt;
    }
    sum % 10 == 0
}
```

Add to `Cargo.toml`:
```toml
once_cell = "1.20"
regex = "1.11"
```

- [ ] **Step 3: Run tests**

```bash
cargo test --test secrets
```

Expected: 5 tests pass. If gitleaks regex won't compile in Rust's `regex` crate (no lookarounds), filter the failing rules silently — the `Regex::new(pattern).ok()?` already does that. Some gitleaks rules use PCRE features `regex` doesn't support; those rules are dropped. Expected coverage stays in the 200+ rule range.

- [ ] **Step 4: Commit**

```bash
git add src/core/secrets.rs tests/secrets.rs Cargo.toml Cargo.lock
git commit -m "feat(core): secret detector backed by gitleaks rules + JWT + Luhn"
```

---

## Task 6: `core::db` — SQLite + FTS5 + migrations

**Files:**
- Create: `src/core/db.rs`

- [ ] **Step 1: Implement**

```rust
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

const SCHEMA_V1: &str = include_str!("schema_v1.sql");

pub fn open_db<P: AsRef<Path>>(path: P) -> Result<Connection> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent).context("create data dir")?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    let current: i64 = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| {
            r.get::<_, String>(0).map(|s| s.parse().unwrap_or(0))
        })
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(SCHEMA_V1)?;
    }
    Ok(conn)
}
```

- [ ] **Step 2: Create `src/core/schema_v1.sql`**

```sql
CREATE TABLE clips (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uuid TEXT NOT NULL UNIQUE,
  text TEXT,
  preview TEXT NOT NULL,
  length INTEGER NOT NULL,
  byte_length INTEGER NOT NULL,
  hash TEXT NOT NULL,
  primary_kind TEXT NOT NULL,
  source_app TEXT,
  window_title TEXT,
  first_copied_at INTEGER NOT NULL,
  last_copied_at INTEGER NOT NULL,
  copy_count INTEGER NOT NULL DEFAULT 1,
  paste_count INTEGER NOT NULL DEFAULT 0,
  is_pinned INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_clips_hash ON clips(hash);
CREATE INDEX idx_clips_kind ON clips(primary_kind);
CREATE INDEX idx_clips_last_copied ON clips(last_copied_at DESC);

CREATE TABLE kinds (
  clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  PRIMARY KEY (clip_id, kind)
);

CREATE TABLE tags (
  clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
  tag TEXT NOT NULL,
  PRIMARY KEY (clip_id, tag)
);

CREATE TABLE secrets (
  clip_id INTEGER PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
  ciphertext BLOB,
  nonce BLOB,
  secret_kind TEXT NOT NULL,
  last_chars TEXT NOT NULL,
  unlock_count INTEGER NOT NULL DEFAULT 0
);

CREATE VIRTUAL TABLE clips_fts USING fts5(
  preview, window_title, primary_kind,
  content='clips', content_rowid='id', tokenize='porter unicode61'
);

CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN
  INSERT INTO clips_fts(rowid, preview, window_title, primary_kind)
  VALUES (new.id, new.preview, coalesce(new.window_title, ''), new.primary_kind);
END;

CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN
  INSERT INTO clips_fts(clips_fts, rowid, preview, window_title, primary_kind)
  VALUES ('delete', old.id, old.preview, coalesce(old.window_title, ''), old.primary_kind);
END;

CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN
  INSERT INTO clips_fts(clips_fts, rowid, preview, window_title, primary_kind)
  VALUES ('delete', old.id, old.preview, coalesce(old.window_title, ''), old.primary_kind);
  INSERT INTO clips_fts(rowid, preview, window_title, primary_kind)
  VALUES (new.id, new.preview, coalesce(new.window_title, ''), new.primary_kind);
END;

CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
INSERT INTO meta(key, value) VALUES ('schema_version', '1');
```

- [ ] **Step 3: Inline test**

In `src/core/db.rs` append:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_tables() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_db(tmp.path().join("t.db")).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('clips','secrets','kinds','tags','meta')",
            [], |r| r.get(0)).unwrap();
        assert_eq!(count, 5);
        let mode: String = conn.pragma_query_value(None, "journal_mode", |r| r.get(0)).unwrap();
        assert_eq!(mode, "wal");
    }
}
```

Run: `cargo test --lib core::db`. Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src/core/db.rs src/core/schema_v1.sql
git commit -m "feat(core): SQLite + FTS5 schema with WAL and migrations"
```

---

## Task 7: `core::store` — high-level CRUD on top of db

**Files:**
- Create: `src/core/store.rs`
- Create: `tests/store.rs`

- [ ] **Step 1: Failing tests `tests/store.rs`**

```rust
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
```

Run: `cargo test --test store`. Expected: FAIL.

- [ ] **Step 2: Implement `src/core/store.rs`**

```rust
use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;

use super::crypto::{decrypt, encrypt};
use super::db::open_db;

pub struct ClipInput {
    pub text: String,
    pub primary_kind: String,
    pub kinds: Vec<String>,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
}

pub struct SecretInput {
    pub text: String,
    pub secret_kind: String,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
}

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
}

pub struct Store {
    conn: Connection,
    master_key: [u8; 32],
}

impl Store {
    pub fn open<P: AsRef<Path>>(path: P, master_key: [u8; 32]) -> Result<Self> {
        let conn = open_db(path)?;
        Ok(Self { conn, master_key })
    }

    pub fn add_clip(&self, input: ClipInput) -> Result<i64> {
        let hash = sha256_hex(&input.text);
        let now = now_ms();
        if let Some(id) = self.find_by_hash(&hash)? {
            self.conn.execute(
                "UPDATE clips SET copy_count = copy_count + 1, last_copied_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            return Ok(id);
        }
        let preview: String = input.text.chars().take(200).collect();
        let length = input.text.chars().count() as i64;
        let byte_length = input.text.len() as i64;
        let uuid = Uuid::new_v4().to_string();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                source_app, window_title, first_copied_at, last_copied_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![uuid, input.text, preview, length, byte_length, hash, input.primary_kind,
                    input.source_app, input.window_title, now],
        )?;
        let id = tx.last_insert_rowid();
        let kinds = if input.kinds.is_empty() { vec![input.primary_kind.clone()] } else { input.kinds };
        for k in kinds.iter().collect::<std::collections::BTreeSet<_>>() {
            tx.execute("INSERT OR IGNORE INTO kinds(clip_id, kind) VALUES (?1, ?2)", params![id, k])?;
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn add_secret(&self, input: SecretInput) -> Result<i64> {
        let last_chars: String = input.text.chars().rev().take(6).collect::<String>().chars().rev().collect();
        let preview = format!("[REDACTED:{}]", input.secret_kind);
        let hash = sha256_hex(&input.text);
        let now = now_ms();
        let uuid = Uuid::new_v4().to_string();
        let primary_kind = format!("secret:{}", input.secret_kind);
        let length = input.text.chars().count() as i64;
        let byte_length = input.text.len() as i64;
        let (ciphertext, nonce) = encrypt(&input.text, &self.master_key);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                source_app, window_title, first_copied_at, last_copied_at)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![uuid, preview, length, byte_length, hash, primary_kind,
                    input.source_app, input.window_title, now],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT OR IGNORE INTO kinds(clip_id, kind) VALUES (?1, ?2)", params![id, primary_kind])?;
        tx.execute(
            "INSERT INTO secrets(clip_id, ciphertext, nonce, secret_kind, last_chars) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, ciphertext, nonce.to_vec(), input.secret_kind, last_chars],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn get_item(&self, id: i64) -> Result<Option<Item>> {
        let row = self.conn.query_row(
            "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                    first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
             FROM clips WHERE id = ?1",
            params![id],
            |r| Ok(Item {
                id: r.get(0)?, uuid: r.get(1)?, text: r.get(2)?, preview: r.get(3)?,
                length: r.get(4)?, primary_kind: r.get(5)?, kinds: vec![], tags: vec![],
                source_app: r.get(6)?, window_title: r.get(7)?,
                first_copied_at: r.get(8)?, last_copied_at: r.get(9)?,
                copy_count: r.get(10)?, paste_count: r.get(11)?,
                is_pinned: r.get::<_, i64>(12)? == 1,
            }),
        ).optional()?;
        let Some(mut item) = row else { return Ok(None) };
        item.kinds = self.kinds_for(item.id)?;
        item.tags = self.tags_for(item.id)?;
        Ok(Some(item))
    }

    pub fn unlock_secret(&self, id: i64) -> Result<String> {
        let row: Option<(Vec<u8>, Vec<u8>)> = self.conn.query_row(
            "SELECT ciphertext, nonce FROM secrets WHERE clip_id = ?1",
            params![id], |r| Ok((r.get(0)?, r.get(1)?))
        ).optional()?;
        let (ct, nonce) = row.ok_or_else(|| anyhow!("no secret for clip {}", id))?;
        let value = decrypt(&ct, &nonce, &self.master_key)?;
        self.conn.execute("UPDATE secrets SET unlock_count = unlock_count + 1 WHERE clip_id = ?1", params![id])?;
        Ok(value)
    }

    pub fn list(&self, limit: i64) -> Result<Vec<Item>> {
        self.list_with(None, limit, 0)
    }

    pub fn list_with(&self, kind: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Item>> {
        let sql = if kind.is_some() {
            "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                    first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
             FROM clips WHERE id IN (SELECT clip_id FROM kinds WHERE kind = ?1)
             ORDER BY is_pinned DESC, last_copied_at DESC LIMIT ?2 OFFSET ?3"
        } else {
            "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                    first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
             FROM clips ORDER BY is_pinned DESC, last_copied_at DESC LIMIT ?1 OFFSET ?2"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = if let Some(k) = kind {
            stmt.query_map(params![k, limit, offset], row_to_item)?.collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![limit, offset], row_to_item)?.collect::<Result<Vec<_>, _>>()?
        };
        let mut out = Vec::with_capacity(rows.len());
        for mut item in rows {
            item.kinds = self.kinds_for(item.id)?;
            item.tags = self.tags_for(item.id)?;
            out.push(item);
        }
        Ok(out)
    }

    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<Item>> {
        let sanitized: String = query.chars()
            .filter(|c| !"\"*()".contains(*c))
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if sanitized.is_empty() { return Ok(Vec::new()); }
        let mut stmt = self.conn.prepare(
            "SELECT clips.id, clips.uuid, clips.text, clips.preview, clips.length, clips.primary_kind,
                    clips.source_app, clips.window_title, clips.first_copied_at, clips.last_copied_at,
                    clips.copy_count, clips.paste_count, clips.is_pinned, bm25(clips_fts) AS rank
             FROM clips_fts JOIN clips ON clips.id = clips_fts.rowid
             WHERE clips_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        )?;
        let rows: Vec<Item> = stmt.query_map(params![sanitized, limit], row_to_item_with_rank)?
            .collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for mut item in rows {
            item.kinds = self.kinds_for(item.id)?;
            item.tags = self.tags_for(item.id)?;
            out.push(item);
        }
        Ok(out)
    }

    pub fn pin(&self, id: i64, pinned: bool) -> Result<()> {
        self.conn.execute("UPDATE clips SET is_pinned = ?1 WHERE id = ?2", params![pinned as i64, id])?;
        Ok(())
    }
    pub fn tag(&self, id: i64, tag: &str) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO tags(clip_id, tag) VALUES (?1, ?2)", params![id, tag])?;
        Ok(())
    }
    pub fn untag(&self, id: i64, tag: &str) -> Result<()> {
        self.conn.execute("DELETE FROM tags WHERE clip_id = ?1 AND tag = ?2", params![id, tag])?;
        Ok(())
    }
    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
        Ok(())
    }
    pub fn bump_paste(&self, id: i64) -> Result<()> {
        self.conn.execute("UPDATE clips SET paste_count = paste_count + 1 WHERE id = ?1", params![id])?;
        Ok(())
    }
    pub fn clear_all(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM clips", [])?)
    }
    pub fn clear_older_than_days(&self, days: i64) -> Result<usize> {
        let cutoff = now_ms() - days * 86_400_000;
        Ok(self.conn.execute("DELETE FROM clips WHERE last_copied_at < ?1", params![cutoff])?)
    }
    pub fn clear_kind(&self, kind: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM clips WHERE primary_kind = ?1 OR id IN (SELECT clip_id FROM kinds WHERE kind = ?1)",
            params![kind],
        )?)
    }
    pub fn prune_oldest(&self, keep: i64) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM clips WHERE id IN (
               SELECT id FROM clips ORDER BY is_pinned DESC, last_copied_at DESC LIMIT -1 OFFSET ?1
             )",
            params![keep],
        )?)
    }

    pub fn stats(&self) -> Result<Stats> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))?;
        let oldest: Option<i64> = self.conn.query_row("SELECT MIN(first_copied_at) FROM clips", [], |r| r.get(0)).ok().flatten();
        let newest: Option<i64> = self.conn.query_row("SELECT MAX(last_copied_at) FROM clips", [], |r| r.get(0)).ok().flatten();
        Ok(Stats { count, oldest, newest })
    }

    fn find_by_hash(&self, hash: &str) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT id FROM clips WHERE hash = ?1 LIMIT 1", params![hash], |r| r.get(0)).optional()?)
    }
    fn kinds_for(&self, id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT kind FROM kinds WHERE clip_id = ?1")?;
        Ok(stmt.query_map(params![id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?)
    }
    fn tags_for(&self, id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT tag FROM tags WHERE clip_id = ?1")?;
        Ok(stmt.query_map(params![id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct Stats { pub count: i64, pub oldest: Option<i64>, pub newest: Option<i64> }

fn row_to_item(r: &rusqlite::Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: r.get(0)?, uuid: r.get(1)?, text: r.get(2)?, preview: r.get(3)?,
        length: r.get(4)?, primary_kind: r.get(5)?, kinds: vec![], tags: vec![],
        source_app: r.get(6)?, window_title: r.get(7)?,
        first_copied_at: r.get(8)?, last_copied_at: r.get(9)?,
        copy_count: r.get(10)?, paste_count: r.get(11)?,
        is_pinned: r.get::<_, i64>(12)? == 1,
    })
}
fn row_to_item_with_rank(r: &rusqlite::Row) -> rusqlite::Result<Item> { row_to_item(r) }

fn sha256_hex(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    hex::encode(h.finalize())
}
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

use rusqlite::OptionalExtension;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --test store
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/core/store.rs tests/store.rs
git commit -m "feat(core): Store with dedup, secrets vault, FTS5 search"
```

---

## Task 8: `core::biometry` — Touch ID gate (stub for now)

**Files:**
- Create: `src/core/biometry.rs`

- [ ] **Step 1: Implement minimal stub**

For v0.3.0-alpha.0 we wire the function but it always returns `Ok(true)` (no actual biometry). Real Touch ID lands in Task 22.

```rust
use anyhow::Result;

pub struct BiometryGate;

impl BiometryGate {
    pub fn new() -> Self { Self }

    pub fn evaluate(&self, _reason: &str) -> Result<bool> {
        // Stub: always succeeds. Real LocalAuthentication wiring lands in Task 22.
        Ok(true)
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/core/biometry.rs
git commit -m "feat(core): biometry gate stub (Touch ID lands in M4)"
```

---

# M2 — Pasteboard & daemon

## Task 9: `core::pasteboard` — NSPasteboard via objc2

**Files:**
- Create: `src/core/pasteboard.rs`

- [ ] **Step 1: Implement**

```rust
use anyhow::{anyhow, Result};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;

const TRANSIENT_TYPES: &[&str] = &[
    "org.nspasteboard.TransientType",
    "org.nspasteboard.ConcealedType",
    "org.nspasteboard.AutoGeneratedType",
    "com.apple.is-sensitive",
];

pub fn read_clipboard() -> Result<String> {
    autoreleasepool(|_| unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let s = pb.stringForType(NSPasteboardTypeString)
            .ok_or_else(|| anyhow!("clipboard has no string content"))?;
        Ok(s.to_string())
    })
}

pub fn write_clipboard(text: &str) -> Result<()> {
    autoreleasepool(|_| unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let s = NSString::from_str(text);
        pb.setString_forType(&s, NSPasteboardTypeString);
        Ok(())
    })
}

pub fn is_transient() -> bool {
    autoreleasepool(|_| unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let Some(types) = pb.types() else { return false };
        for ty in types.iter() {
            let s = ty.to_string();
            if TRANSIENT_TYPES.contains(&s.as_str()) { return true; }
        }
        false
    })
}

pub fn change_count() -> i64 {
    autoreleasepool(|_| unsafe {
        NSPasteboard::generalPasteboard().changeCount() as i64
    })
}
```

- [ ] **Step 2: Smoke test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // requires real clipboard
    fn round_trip() {
        write_clipboard("hello pasteboard").unwrap();
        let got = read_clipboard().unwrap();
        assert_eq!(got, "hello pasteboard");
    }
}
```

Run: `cargo test --lib core::pasteboard -- --ignored`. Expected: passes (touches your real clipboard).

- [ ] **Step 3: Commit**

```bash
git add src/core/pasteboard.rs
git commit -m "feat(core): NSPasteboard read/write/is_transient via objc2"
```

---

## Task 10: `daemon::context` — frontmost app + window title

**Files:**
- Create: `src/daemon/context.rs`
- Modify: `src/daemon/mod.rs`

- [ ] **Step 1: Implement `src/daemon/context.rs`**

```rust
use objc2::rc::autoreleasepool;
use objc2_app_kit::NSWorkspace;

pub struct CaptureContext { pub front_app: Option<String>, pub window_title: Option<String> }

pub fn capture(with_window_title: bool) -> CaptureContext {
    let front_app = autoreleasepool(|_| unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        ws.frontmostApplication().and_then(|app| app.localizedName().map(|n| n.to_string()))
    });
    let window_title = if with_window_title { capture_window_title(&front_app) } else { None };
    CaptureContext { front_app, window_title }
}

fn capture_window_title(_front_app: &Option<String>) -> Option<String> {
    // accessibility crate API: walk AXUIElement::application(pid) -> .attribute(AXAttribute::main_window()) -> .attribute(AXAttribute::title())
    // For v0.3.0-alpha.0 we return None and wire real Accessibility in v0.3.1.
    // Reason: accessibility 0.1 needs the running app's PID, and we don't currently have it from NSWorkspace
    // without a deeper objc2 traversal. Ship the simpler path first.
    None
}
```

- [ ] **Step 2: `src/daemon/mod.rs`**

```rust
pub mod context;
pub mod watcher;
```

- [ ] **Step 3: Commit**

```bash
git add src/daemon/
git commit -m "feat(daemon): frontmost app via NSWorkspace; window title placeholder"
```

---

## Task 11: `daemon::watcher` — poll loop

**Files:**
- Create: `src/daemon/watcher.rs`

- [ ] **Step 1: Implement**

```rust
use crate::core::{pasteboard, secrets, store::{ClipInput, SecretInput, Store}, types};
use crate::daemon::context;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub struct WatcherOptions {
    pub poll_ms: u64,
    pub capture_window_title: bool,
    pub ignore_apps: Vec<String>,
    pub never_store_secrets: bool,
    pub max_items: i64,
}

impl Default for WatcherOptions {
    fn default() -> Self {
        Self {
            poll_ms: 1500,
            capture_window_title: false,
            ignore_apps: vec![],
            never_store_secrets: false,
            max_items: 1000,
        }
    }
}

pub fn run_watcher(store: Arc<Store>, opts: WatcherOptions, stop: Arc<AtomicBool>) {
    let mut last_seen: Option<String> = None;
    while !stop.load(Ordering::Relaxed) {
        match tick(&store, &opts, &mut last_seen) {
            Ok(_) => {}
            Err(e) => warn!("watcher tick: {}", e),
        }
        std::thread::sleep(Duration::from_millis(opts.poll_ms));
    }
    info!("watcher stopped");
}

fn tick(store: &Store, opts: &WatcherOptions, last_seen: &mut Option<String>) -> anyhow::Result<()> {
    let text = pasteboard::read_clipboard()?;
    if text.is_empty() { return Ok(()); }
    if last_seen.as_ref() == Some(&text) { return Ok(()); }
    *last_seen = Some(text.clone());

    if pasteboard::is_transient() {
        info!("skip: transient pasteboard type");
        return Ok(());
    }

    let ctx = context::capture(opts.capture_window_title);
    if let Some(app) = &ctx.front_app {
        if opts.ignore_apps.iter().any(|a| a == app) {
            info!("skip: ignored app {}", app);
            return Ok(());
        }
    }

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
        store.add_clip(ClipInput {
            text: text.clone(),
            primary_kind: cls.primary_kind.clone(),
            kinds: cls.kinds,
            source_app: ctx.front_app.clone(),
            window_title: ctx.window_title.clone(),
        })?;
        info!("clip captured: {} from {:?}", cls.primary_kind, ctx.front_app);
    }

    store.prune_oldest(opts.max_items)?;
    Ok(())
}
```

- [ ] **Step 2: Commit**

```bash
git add src/daemon/watcher.rs
git commit -m "feat(daemon): watcher poll loop wiring pasteboard + types + secrets + store"
```

---

## Task 12: `main.rs` daemon subcommand

**Files:**
- Modify: `src/main.rs`
- Create: `src/cli/mod.rs` with `Cli` struct

- [ ] **Step 1: Update `src/cli/mod.rs`**

```rust
use clap::{Parser, Subcommand};

pub mod install;
pub mod uninstall;
pub mod status;
pub mod vault;
pub mod doctor;
pub mod migrate_v2;

#[derive(Parser)]
#[command(name = "clipboard-history-mcp", version, about = "macOS clipboard history MCP")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Run the watcher daemon (poll pbpaste, write to DB)
    Daemon,
    /// Run the MCP stdio server
    Serve,
    /// Install launchd agent
    Install { #[arg(long)] window_titles: bool },
    /// Uninstall launchd agent
    Uninstall { #[arg(long)] keep_data: bool },
    /// Show daemon + DB status
    Status,
    /// Doctor diagnostic
    Doctor,
    /// Vault subcommands
    Vault { #[arg(value_name = "SUBCMD")] sub: String, #[arg(value_name = "ID")] id: Option<i64> },
    /// Migrate from v2 SQLite (no-op for matching schema)
    MigrateV2,
}
```

- [ ] **Step 2: Update `src/main.rs`**

```rust
use anyhow::Result;
use clap::Parser;
use clipboard_history_mcp::cli::{Cli, Cmd};
use clipboard_history_mcp::core::{crypto::get_or_create_master_key, store::Store};
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};

fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DATA_DIR") { return PathBuf::from(p); }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Application Support/clipboard-history-mcp")
}

fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DB_PATH") { return PathBuf::from(p); }
    data_dir().join("history.db")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Daemon => run_daemon().await,
        Cmd::Serve => clipboard_history_mcp::mcp::run_server().await,
        Cmd::Install { window_titles } => clipboard_history_mcp::cli::install::install(window_titles),
        Cmd::Uninstall { keep_data } => clipboard_history_mcp::cli::uninstall::uninstall(keep_data),
        Cmd::Status => clipboard_history_mcp::cli::status::status(),
        Cmd::Doctor => clipboard_history_mcp::cli::doctor::doctor(),
        Cmd::Vault { sub, id } => clipboard_history_mcp::cli::vault::vault(&sub, id),
        Cmd::MigrateV2 => clipboard_history_mcp::cli::migrate_v2::migrate_v2(),
    }
}

async fn run_daemon() -> Result<()> {
    use clipboard_history_mcp::daemon::watcher::{run_watcher, WatcherOptions};
    let key = get_or_create_master_key()?;
    let store = Arc::new(Store::open(db_path(), key)?);
    let stop = Arc::new(AtomicBool::new(false));
    let pid_file = data_dir().join("daemon.pid");
    std::fs::create_dir_all(data_dir())?;
    std::fs::write(&pid_file, std::process::id().to_string())?;

    let stop_clone = stop.clone();
    ctrlc::set_handler(move || stop_clone.store(true, std::sync::atomic::Ordering::Relaxed))?;

    let opts = WatcherOptions {
        poll_ms: std::env::var("CLIPBOARD_POLL_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(1500),
        capture_window_title: std::env::var("CLIPBOARD_CAPTURE_WINDOW_TITLE").as_deref() == Ok("1"),
        ignore_apps: std::env::var("CLIPBOARD_IGNORE_APPS").unwrap_or_default()
            .split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        never_store_secrets: std::env::var("CLIPBOARD_NEVER_STORE_SECRETS").as_deref() == Ok("1"),
        max_items: std::env::var("CLIPBOARD_HISTORY_MAX").ok().and_then(|s| s.parse().ok()).unwrap_or(1000),
    };

    tracing::info!("daemon started pid={} db={:?}", std::process::id(), db_path());
    let store_for_watcher = store.clone();
    let stop_for_watcher = stop.clone();
    std::thread::spawn(move || run_watcher(store_for_watcher, opts, stop_for_watcher));

    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let _ = std::fs::remove_file(&pid_file);
    Ok(())
}
```

Add to `Cargo.toml`:
```toml
ctrlc = "3"
```

For now, stub the not-yet-implemented modules:
- `src/cli/install.rs` etc.: each gets a `pub fn install(_: bool) -> Result<()> { Ok(()) }` stub
- `src/mcp/mod.rs`: `pub async fn run_server() -> anyhow::Result<()> { Ok(()) }`

- [ ] **Step 3: Smoke test**

```bash
cargo build --release 2>&1 | tail -5
CLIPBOARD_DATA_DIR=/tmp/cbhist-rs ./target/release/clipboard-history-mcp daemon &
PID=$!
pbcopy <<< "rust clipboard"
sleep 2
kill -TERM $PID
wait $PID 2>/dev/null
sqlite3 /tmp/cbhist-rs/history.db "SELECT primary_kind, preview FROM clips;"
```

Expected: prints `text|rust clipboard`.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs src/cli/mod.rs src/cli/*.rs src/mcp/mod.rs Cargo.toml Cargo.lock
git commit -m "feat: main.rs subcommand dispatch + daemon entry"
```

---

## Task 13: stub remaining cli/mcp module files

To make the build green so subsequent tasks can fill them.

**Files:**
- Create: `src/cli/install.rs` `uninstall.rs` `status.rs` `vault.rs` `doctor.rs` `migrate_v2.rs`
- Create: `src/mcp/tools.rs`

- [ ] **Step 1: Stub each cli file**

Each file: empty function returning `Ok(())`. Example `src/cli/install.rs`:

```rust
use anyhow::Result;

pub fn install(_window_titles: bool) -> Result<()> {
    println!("install: not implemented yet");
    Ok(())
}
```

Repeat shape for `uninstall(_: bool)`, `status()`, `doctor()`, `vault(_: &str, _: Option<i64>)`, `migrate_v2()`.

- [ ] **Step 2: `src/mcp/mod.rs`**

```rust
pub mod tools;

use anyhow::Result;

pub async fn run_server() -> Result<()> {
    eprintln!("mcp server not implemented yet");
    Ok(())
}
```

- [ ] **Step 3: Build**

```bash
cargo build --release 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/cli/*.rs src/mcp/
git commit -m "chore: stub remaining cli/mcp modules so daemon builds"
```

---

# M3 — MCP server

## Task 14: `mcp::tools` — single impl block with all 15 tools

**Files:**
- Create: `src/mcp/tools.rs` (replace stub from Task 13)

- [ ] **Step 1: Write the impl block**

This is the largest single file in the project. ~250 lines. Each `#[tool]` annotation declares a tool.

```rust
use crate::core::{biometry::BiometryGate, pasteboard, store::{ClipInput, SecretInput, Store}};
use anyhow::Result;
use rmcp::{tool, tool_router, handler::server::wrapper::Parameters, schemars};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct ClipboardServer { pub store: Arc<Store>, pub biometry: Arc<BiometryGate>, pub db_path: std::path::PathBuf }

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ListParams {
    pub limit: Option<i64>, pub offset: Option<i64>,
    pub kind: Option<String>, pub source_app: Option<String>,
    pub since: Option<i64>, pub pinned_only: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetItemParams { pub id: i64 }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams { pub query: String, pub limit: Option<i64> }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCodeParams { pub language: Option<String>, pub limit: Option<i64> }

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct LimitParams { pub limit: Option<i64> }

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SecretsIndexParams { pub kind: Option<String> }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnlockParams { pub id: i64, pub reason: String }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdParams { pub id: i64 }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PinParams { pub id: i64, pub pinned: bool }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TagParams { pub id: i64, pub tag: String, pub remove: Option<bool> }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClearParams { pub scope: String }

#[tool_router(server_handler)]
impl ClipboardServer {
    #[tool(description = "List recent clipboard entries, newest first. Secret values are never returned — only metadata.")]
    async fn list_history(&self, Parameters(p): Parameters<ListParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        match self.store.list_with(p.kind.as_deref(), limit, p.offset.unwrap_or(0)) {
            Ok(items) => serde_json::json!({ "count": items.len(), "items": items }).to_string(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Fetch one clipboard entry by id. For secrets returns metadata only.")]
    async fn get_item(&self, Parameters(p): Parameters<GetItemParams>) -> String {
        match self.store.get_item(p.id) {
            Ok(Some(item)) => {
                let requires_unlock = item.primary_kind.starts_with("secret:");
                let mut v = serde_json::to_value(&item).unwrap_or(serde_json::Value::Null);
                if requires_unlock { v["requiresUnlock"] = serde_json::json!(true); }
                v.to_string()
            }
            Ok(None) => format!("error: not found id={}", p.id),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Full-text search clipboard history (FTS5 BM25).")]
    async fn search_history(&self, Parameters(p): Parameters<SearchParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        match self.store.search(&p.query, limit) {
            Ok(items) => serde_json::json!({ "count": items.len(), "items": items }).to_string(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Return URL clips, deduped by host.")]
    async fn get_urls(&self, Parameters(p): Parameters<LimitParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        match self.store.list_with(Some("url"), 200, 0) {
            Ok(items) => {
                let mut by_host: std::collections::HashMap<String, &_> = std::collections::HashMap::new();
                for item in &items {
                    if let Some(t) = &item.text {
                        if let Ok(u) = url::Url::parse(t) {
                            let host = u.host_str().unwrap_or("").to_string();
                            by_host.entry(host).and_modify(|cur: &mut &_| {
                                if item.last_copied_at > cur.last_copied_at { *cur = item; }
                            }).or_insert(item);
                        }
                    }
                }
                let result: Vec<_> = by_host.values().take(limit as usize).collect();
                serde_json::json!({ "count": result.len(), "items": result }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Return code clips, optionally filtered by language.")]
    async fn get_code(&self, Parameters(p): Parameters<GetCodeParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        let kind = p.language.as_ref().map(|l| format!("code:{}", l.to_lowercase()));
        let res = match kind {
            Some(k) => self.store.list_with(Some(&k), limit, 0),
            None => self.store.list(limit).map(|all| all.into_iter().filter(|i| i.primary_kind.starts_with("code:")).collect()),
        };
        match res {
            Ok(items) => serde_json::json!({ "count": items.len(), "items": items }).to_string(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Return JSON clips with parsed structure preview.")]
    async fn get_json(&self, Parameters(p): Parameters<LimitParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        match self.store.list_with(Some("json"), limit, 0) {
            Ok(items) => {
                let parsed: Vec<_> = items.into_iter().map(|i| {
                    let p = i.text.as_ref().and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok());
                    let mut v = serde_json::to_value(&i).unwrap_or(serde_json::Value::Null);
                    v["parsed"] = p.unwrap_or(serde_json::Value::Null);
                    v
                }).collect();
                serde_json::json!({ "count": parsed.len(), "items": parsed }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "List secret clip metadata only — no values.")]
    async fn get_secrets_index(&self, Parameters(p): Parameters<SecretsIndexParams>) -> String {
        let filter = p.kind.as_ref().map(|k| format!("secret:{}", k));
        let res = match filter.as_deref() {
            Some(k) => self.store.list_with(Some(k), 200, 0),
            None => self.store.list(500).map(|all| all.into_iter().filter(|i| i.primary_kind.starts_with("secret:")).collect()),
        };
        match res {
            Ok(items) => {
                let stripped: Vec<_> = items.into_iter().map(|i| serde_json::json!({
                    "id": i.id, "secretKind": i.primary_kind.trim_start_matches("secret:"),
                    "sourceApp": i.source_app, "windowTitle": i.window_title,
                    "firstCopiedAt": i.first_copied_at, "lastCopiedAt": i.last_copied_at,
                })).collect();
                serde_json::json!({ "count": stripped.len(), "items": stripped }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Decrypt and return a stored secret. Reason argument is mandatory and audited. Touch ID gated.")]
    async fn unlock_secret(&self, Parameters(p): Parameters<UnlockParams>) -> String {
        tracing::info!("unlock_secret id={} reason={:?}", p.id, p.reason);
        match self.biometry.evaluate(&format!("Reveal stored secret #{}: {}", p.id, p.reason)) {
            Ok(false) => return "error: biometric authentication failed".into(),
            Err(e) => return format!("error: biometry error: {}", e),
            Ok(true) => {}
        }
        match self.store.unlock_secret(p.id) {
            Ok(value) => {
                let last_chars: String = value.chars().rev().take(6).collect::<String>().chars().rev().collect();
                serde_json::json!({ "value": value, "lastChars": last_chars }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Restore a clip to the system clipboard. Refuses secrets — use unlock_secret first.")]
    async fn copy_item(&self, Parameters(p): Parameters<IdParams>) -> String {
        match self.store.get_item(p.id) {
            Ok(Some(item)) => {
                if item.text.is_none() {
                    return serde_json::json!({ "error": "cannot restore secret directly", "requiresUnlock": true }).to_string();
                }
                if let Err(e) = pasteboard::write_clipboard(item.text.as_deref().unwrap()) { return format!("error: {}", e); }
                let _ = self.store.bump_paste(p.id);
                serde_json::json!({ "ok": true, "id": p.id, "length": item.length }).to_string()
            }
            Ok(None) => format!("error: not found id={}", p.id),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Pin or unpin a clip.")]
    async fn pin_item(&self, Parameters(p): Parameters<PinParams>) -> String {
        match self.store.pin(p.id, p.pinned) { Ok(_) => r#"{"ok":true}"#.into(), Err(e) => format!("error: {}", e) }
    }

    #[tool(description = "Add or remove a tag on a clip.")]
    async fn tag_item(&self, Parameters(p): Parameters<TagParams>) -> String {
        let r = if p.remove.unwrap_or(false) { self.store.untag(p.id, &p.tag) } else { self.store.tag(p.id, &p.tag) };
        match r { Ok(_) => r#"{"ok":true}"#.into(), Err(e) => format!("error: {}", e) }
    }

    #[tool(description = "Hard-delete a clip (and its secrets row if applicable).")]
    async fn delete_item(&self, Parameters(p): Parameters<IdParams>) -> String {
        match self.store.delete(p.id) { Ok(_) => r#"{"ok":true}"#.into(), Err(e) => format!("error: {}", e) }
    }

    #[tool(description = "Clear history. scope: 'all' | 'older_than_days:N' | 'kind:K'")]
    async fn clear_history(&self, Parameters(p): Parameters<ClearParams>) -> String {
        let r = if p.scope == "all" { self.store.clear_all() }
            else if let Some(rest) = p.scope.strip_prefix("older_than_days:") {
                rest.parse::<i64>().map_err(anyhow::Error::from).and_then(|d| Ok(self.store.clear_older_than_days(d)?))
            } else if let Some(rest) = p.scope.strip_prefix("kind:") { self.store.clear_kind(rest) }
            else { return format!("error: unknown scope '{}'", p.scope) };
        match r { Ok(n) => serde_json::json!({"ok": true, "removed": n}).to_string(), Err(e) => format!("error: {}", e) }
    }

    #[tool(description = "Counts by kind, oldest/newest, db size.")]
    async fn get_stats(&self, _: Parameters<()>) -> String {
        match self.store.stats() {
            Ok(s) => {
                let size = std::fs::metadata(&self.db_path).ok().map(|m| m.len()).unwrap_or(0);
                serde_json::json!({ "count": s.count, "oldest": s.oldest, "newest": s.newest, "dbPath": self.db_path, "sizeBytes": size }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Daemon status (running, pid).")]
    async fn daemon_status(&self, _: Parameters<()>) -> String {
        let pid_file = self.db_path.parent().unwrap().join("daemon.pid");
        if !pid_file.exists() { return r#"{"running":false}"#.into(); }
        let Ok(s) = std::fs::read_to_string(&pid_file) else { return r#"{"running":false}"#.into(); };
        let Ok(pid) = s.trim().parse::<i32>() else { return r#"{"running":false}"#.into(); };
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if alive { format!(r#"{{"running":true,"pid":{}}}"#, pid) } else { r#"{"running":false}"#.into() }
    }
}
```

Add to `Cargo.toml`:
```toml
url = "2.5"
libc = "0.2"
```

- [ ] **Step 2: Build**

```bash
cargo build --release 2>&1 | tail -10
```

Expected: builds; rmcp may emit warnings about derive macros — fine.

- [ ] **Step 3: Commit**

```bash
git add src/mcp/tools.rs Cargo.toml Cargo.lock
git commit -m "feat(mcp): all 15 tools as one impl block via rmcp #[tool] macros"
```

---

## Task 15: `mcp::run_server` — wire stdio transport

**Files:**
- Modify: `src/mcp/mod.rs`

- [ ] **Step 1: Implement**

```rust
pub mod tools;

use crate::core::{biometry::BiometryGate, crypto::get_or_create_master_key, store::Store};
use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use std::path::PathBuf;
use std::sync::Arc;

fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DATA_DIR") { return PathBuf::from(p); }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Application Support/clipboard-history-mcp")
}
fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DB_PATH") { return PathBuf::from(p); }
    data_dir().join("history.db")
}

pub async fn run_server() -> Result<()> {
    let key = get_or_create_master_key()?;
    let dbp = db_path();
    let store = Arc::new(Store::open(&dbp, key)?);
    let biometry = Arc::new(BiometryGate::new());
    let server = tools::ClipboardServer { store, biometry, db_path: dbp };
    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}
```

- [ ] **Step 2: Smoke test via stdio**

```bash
cargo build --release
(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0.1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_stats","arguments":{}}}'; \
  sleep 1) | ./target/release/clipboard-history-mcp serve 2>/dev/null
```

Expected: JSON responses, tools/list returns 15 tools, get_stats returns count/oldest/newest/dbPath.

- [ ] **Step 3: Commit**

```bash
git add src/mcp/mod.rs
git commit -m "feat(mcp): wire stdio transport + ClipboardServer construction"
```

---

## Task 16: Re-register MCP with Claude Code (point at Rust binary)

**Files:** none (config change)

- [ ] **Step 1: Re-register**

```bash
claude mcp remove clipboard-history 2>&1 || true
claude mcp add -s user clipboard-history -- /Users/stock/Documents/wezom/clipboard-history-mcp/target/release/clipboard-history-mcp serve
claude mcp list 2>&1 | grep clipboard-history
```

Expected: `✓ Connected`.

- [ ] **Step 2: No commit**

---

## Task 17: Integration test — end-to-end MCP stdio

**Files:**
- Create: `tests/mcp_smoke.rs`

- [ ] **Step 1: Write test**

```rust
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
#[ignore] // hits real Keychain
fn mcp_handshake_and_list_tools() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_clipboard-history-mcp"))
        .arg("serve")
        .env("CLIPBOARD_DATA_DIR", std::env::temp_dir().join("cbhist-rs-int"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}},"clientInfo":{{"name":"t","version":"1"}}}}}}"#).unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let _ = child.kill();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(r#""id":1"#));
    assert!(stdout.contains(r#""id":2"#));
    assert!(stdout.contains("list_history") || stdout.contains("get_stats"));
}
```

Run:
```bash
cargo test --release --test mcp_smoke -- --ignored
```
Expected: passes.

- [ ] **Step 2: Commit**

```bash
git add tests/mcp_smoke.rs
git commit -m "test(integration): MCP stdio handshake + tools/list end-to-end"
```

---

# M4 — CLI

## Task 18: `cli::install` + `uninstall` + `start`/`stop`

**Files:**
- Create: `scripts/launchd.plist.template`
- Modify: `src/cli/install.rs`, `src/cli/uninstall.rs`

- [ ] **Step 1: Template**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>__LABEL__</string>
  <key>ProgramArguments</key>
  <array>
    <string>__BINARY__</string>
    <string>daemon</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>EnvironmentVariables</key>
  <dict>
__ENV_DICT__
  </dict>
  <key>StandardOutPath</key><string>__LOG__</string>
  <key>StandardErrorPath</key><string>__LOG__</string>
</dict>
</plist>
```

- [ ] **Step 2: `src/cli/install.rs`**

```rust
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;

const LABEL: &str = "me.kz.clipboard-history-rs";

pub fn install(window_titles: bool) -> Result<()> {
    let home = std::env::var("HOME")?;
    let plist_path = PathBuf::from(&home).join(format!("Library/LaunchAgents/{}.plist", LABEL));
    let data_dir = PathBuf::from(&home).join("Library/Application Support/clipboard-history-mcp");
    std::fs::create_dir_all(&data_dir)?;
    let log = data_dir.join("daemon.log");
    let bin = std::env::current_exe()?;

    let template = include_str!("../../scripts/launchd.plist.template");
    let env_dict = format!(
        r#"    <key>CLIPBOARD_CAPTURE_WINDOW_TITLE</key>
    <string>{}</string>"#,
        if window_titles { "1" } else { "0" }
    );
    let plist = template
        .replace("__LABEL__", LABEL)
        .replace("__BINARY__", bin.to_str().unwrap())
        .replace("__ENV_DICT__", &env_dict)
        .replace("__LOG__", log.to_str().unwrap());
    std::fs::write(&plist_path, plist)?;

    let _ = Command::new("launchctl").args(["unload", plist_path.to_str().unwrap()]).status();
    let s = Command::new("launchctl").args(["load", "-w", plist_path.to_str().unwrap()]).status()?;
    if !s.success() { return Err(anyhow!("launchctl load failed")); }
    println!("Installed → {}", plist_path.display());
    println!("Logs    → {}", log.display());
    Ok(())
}
```

- [ ] **Step 3: `src/cli/uninstall.rs`**

```rust
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

pub fn uninstall(keep_data: bool) -> Result<()> {
    let home = std::env::var("HOME")?;
    let plist = PathBuf::from(&home).join("Library/LaunchAgents/me.kz.clipboard-history-rs.plist");
    let data_dir = PathBuf::from(&home).join("Library/Application Support/clipboard-history-mcp");
    if plist.exists() {
        let _ = Command::new("launchctl").args(["unload", plist.to_str().unwrap()]).status();
        std::fs::remove_file(&plist)?;
        println!("Removed {}", plist.display());
    }
    if !keep_data && data_dir.exists() {
        std::fs::remove_dir_all(&data_dir)?;
        println!("Removed {}", data_dir.display());
    }
    Ok(())
}
```

- [ ] **Step 4: Build, do NOT actually install**

```bash
cargo build --release
./target/release/clipboard-history-mcp --help
```

Expected: `--help` lists install/uninstall/status/etc.

- [ ] **Step 5: Commit**

```bash
git add src/cli/install.rs src/cli/uninstall.rs scripts/launchd.plist.template
git commit -m "feat(cli): install/uninstall via launchd plist"
```

---

## Task 19: `cli::status`

**Files:**
- Modify: `src/cli/status.rs`

- [ ] **Step 1: Implement**

```rust
use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;

pub fn status() -> Result<()> {
    let home = std::env::var("HOME")?;
    let data_dir = PathBuf::from(&home).join("Library/Application Support/clipboard-history-mcp");
    let pid_file = data_dir.join("daemon.pid");
    let db = data_dir.join("history.db");

    let daemon = if pid_file.exists() {
        let pid = std::fs::read_to_string(&pid_file)?.trim().parse::<i32>().ok();
        match pid {
            Some(p) if unsafe { libc::kill(p, 0) } == 0 => json!({"running": true, "pid": p}),
            Some(p) => json!({"running": false, "stalePid": p}),
            None => json!({"running": false}),
        }
    } else { json!({"running": false}) };

    let db_size = std::fs::metadata(&db).ok().map(|m| m.len());
    println!("{}", json!({
        "dataDir": data_dir, "dbPath": db, "dbExists": db.exists(),
        "dbSizeBytes": db_size, "daemon": daemon,
    }));
    Ok(())
}
```

- [ ] **Step 2: Commit**

```bash
git add src/cli/status.rs
git commit -m "feat(cli): status subcommand"
```

---

## Task 20: `cli::vault` + `cli::doctor`

**Files:**
- Modify: `src/cli/vault.rs`, `src/cli/doctor.rs`

- [ ] **Step 1: `src/cli/vault.rs`**

```rust
use crate::core::{crypto::get_or_create_master_key, store::Store};
use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub fn vault(sub: &str, id: Option<i64>) -> Result<()> {
    let home = std::env::var("HOME")?;
    let db = PathBuf::from(&home).join("Library/Application Support/clipboard-history-mcp/history.db");
    let key = get_or_create_master_key()?;
    let store = Store::open(&db, key)?;
    match sub {
        "list" => {
            let items = store.list(500)?;
            let secrets: Vec<_> = items.into_iter().filter(|i| i.primary_kind.starts_with("secret:")).collect();
            println!("{}", serde_json::json!({"count": secrets.len(), "items": secrets}));
        }
        "unlock" => {
            let id = id.ok_or_else(|| anyhow!("Usage: vault unlock <id>"))?;
            println!("{}", store.unlock_secret(id)?);
        }
        other => return Err(anyhow!("unknown vault subcommand: {}", other)),
    }
    Ok(())
}
```

- [ ] **Step 2: `src/cli/doctor.rs`**

```rust
use anyhow::Result;
use std::process::Command;

pub fn doctor() -> Result<()> {
    let mut checks: Vec<(&str, Box<dyn Fn() -> Result<String>>)> = vec![];

    checks.push(("data dir writable", Box::new(|| {
        let home = std::env::var("HOME")?;
        let p = std::path::PathBuf::from(home).join("Library/Application Support/clipboard-history-mcp");
        std::fs::create_dir_all(&p)?;
        Ok(p.display().to_string())
    })));

    checks.push(("Keychain accessible", Box::new(|| {
        let s = Command::new("security").arg("list-keychains").output()?;
        if !s.status.success() { return Err(anyhow::anyhow!("security CLI failed")); }
        Ok("ok".into())
    })));

    checks.push(("NSPasteboard reachable", Box::new(|| {
        let _ = crate::core::pasteboard::change_count();
        Ok("ok".into())
    })));

    for (label, fn_) in checks {
        match fn_() {
            Ok(out) => println!("✓ {}: {}", label, out),
            Err(e) => println!("✗ {}: {}", label, e),
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Smoke**

```bash
cargo build --release
./target/release/clipboard-history-mcp doctor
./target/release/clipboard-history-mcp vault list
```

Expected: doctor 3 ✓, vault list `{"count":0,"items":[]}`.

- [ ] **Step 4: Commit**

```bash
git add src/cli/vault.rs src/cli/doctor.rs
git commit -m "feat(cli): vault list/unlock + doctor diagnostic"
```

---

## Task 21: `cli::migrate_v2`

**Files:**
- Modify: `src/cli/migrate_v2.rs`

- [ ] **Step 1: Implement**

The v2 schema is byte-identical, so this is a sanity check + optional re-encrypt. v0.3.0-alpha.0 ships a no-op-with-message version.

```rust
use anyhow::Result;
use std::path::PathBuf;

pub fn migrate_v2() -> Result<()> {
    let home = std::env::var("HOME")?;
    let v2_db = PathBuf::from(&home).join("Library/Application Support/clipboard-history-mcp/history.db");
    if !v2_db.exists() {
        println!("No v2 DB found at {} — nothing to migrate.", v2_db.display());
        return Ok(());
    }
    println!("v2 DB at {} — schema is shared with v3, no migration needed.", v2_db.display());
    println!("Run `clipboard-history-mcp status` to see existing rows.");
    Ok(())
}
```

- [ ] **Step 2: Commit**

```bash
git add src/cli/migrate_v2.rs
git commit -m "feat(cli): migrate-v2 sanity-check (schema is shared)"
```

---

## Task 22: Real Touch ID via `objc2-local-authentication`

**Files:**
- Modify: `src/core/biometry.rs`

- [ ] **Step 1: Implement**

```rust
use anyhow::{anyhow, Result};
use objc2::rc::autoreleasepool;
use objc2_foundation::NSString;
use objc2_local_authentication::{LAContext, LAPolicy};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(300);

static AUTH_CACHE: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

pub struct BiometryGate;

impl BiometryGate {
    pub fn new() -> Self { Self }

    pub fn evaluate(&self, reason: &str) -> Result<bool> {
        let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
        if let Ok(g) = cache.lock() {
            if let Some(t) = *g {
                if t.elapsed() < CACHE_TTL { return Ok(true); }
            }
        }

        let ok = autoreleasepool(|_| unsafe {
            let ctx = LAContext::new();
            let policy = LAPolicy::DeviceOwnerAuthentication;
            let mut error: Option<objc2::rc::Retained<objc2_foundation::NSError>> = None;
            let can = ctx.canEvaluatePolicy_error(policy, &mut error);
            if !can {
                tracing::warn!("biometry unavailable: {:?}", error.map(|e| e.localizedDescription().to_string()));
                return false;
            }

            let reason_ns = NSString::from_str(reason);
            let (sender, receiver) = std::sync::mpsc::channel::<bool>();
            let block = block2::RcBlock::new(move |success: bool, _err: *mut objc2_foundation::NSError| {
                let _ = sender.send(success);
            });
            ctx.evaluatePolicy_localizedReason_reply(policy, &reason_ns, &block);
            receiver.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
        });

        if ok {
            if let Ok(mut g) = cache.lock() { *g = Some(Instant::now()); }
        }
        Ok(ok)
    }
}
```

Add to `Cargo.toml`:
```toml
block2 = "0.5"
```

- [ ] **Step 2: Smoke test (manual)**

```bash
cargo build --release
./target/release/clipboard-history-mcp vault list
```

Touch ID prompt should appear on first secret access (vault list itself doesn't unlock — to test biometry, copy a secret, then call `vault unlock <id>`).

If `objc2-local-authentication` API names differ from this snippet, follow the latest docs at https://docs.rs/objc2-local-authentication and adjust. The pattern (canEvaluatePolicy → evaluatePolicy with reply block) stays the same.

- [ ] **Step 3: Commit**

```bash
git add src/core/biometry.rs Cargo.toml Cargo.lock
git commit -m "feat(core): real Touch ID gate via objc2-local-authentication"
```

---

# M5 — Packaging + release

## Task 23: MCPB manifest + pack script

**Files:**
- Create: `mcpb/manifest.json`
- Create: `scripts/pack-mcpb.sh`

- [ ] **Step 1: `mcpb/manifest.json`**

```json
{
  "manifest_version": "0.4",
  "name": "clipboard-history-mcp",
  "version": "0.3.0-alpha.0",
  "description": "Type-aware, secret-safe macOS clipboard history exposed to Claude via MCP.",
  "author": { "name": "Dmytro Khomenko" },
  "server": {
    "type": "binary",
    "entry_point": "server/clipboard-history-mcp",
    "mcp_config": {
      "command": "${__dirname}/server/clipboard-history-mcp",
      "args": ["serve"],
      "env": {}
    }
  },
  "compatibility": {
    "claude_desktop": ">=1.0.0",
    "platforms": ["darwin"]
  }
}
```

- [ ] **Step 2: `scripts/pack-mcpb.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE"

cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
mkdir -p mcpb/server
lipo -create \
  -output mcpb/server/clipboard-history-mcp \
  target/aarch64-apple-darwin/release/clipboard-history-mcp \
  target/x86_64-apple-darwin/release/clipboard-history-mcp
strip mcpb/server/clipboard-history-mcp
codesign --deep -s - mcpb/server/clipboard-history-mcp 2>/dev/null || true

cd mcpb
npx -y @anthropic-ai/mcpb pack . -o ../clipboard-history-mcp.mcpb
ls -la ../clipboard-history-mcp.mcpb
```

```bash
chmod +x scripts/pack-mcpb.sh
```

- [ ] **Step 3: Verify pack manifest**

```bash
npx -y @anthropic-ai/mcpb validate mcpb/
```

Expected: validates clean.

- [ ] **Step 4: Commit**

```bash
git add mcpb/manifest.json scripts/pack-mcpb.sh
git commit -m "feat(release): MCPB manifest + universal-binary pack script"
```

---

## Task 24: GitHub Actions

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`
- Modify: `.github/dependabot.yml`

- [ ] **Step 1: `.github/workflows/ci.yml`** (replace v2's)

```yaml
name: CI
on:
  push: { branches: [main, v3-rust] }
  pull_request:
jobs:
  rust:
    strategy:
      matrix:
        os: [macos-13, macos-14]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --release
```

- [ ] **Step 2: `.github/workflows/release.yml`**

```yaml
name: Release
on:
  push:
    tags: ['v0.3.*']
jobs:
  build:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: aarch64-apple-darwin,x86_64-apple-darwin }
      - uses: Swatinem/rust-cache@v2
      - run: bash scripts/pack-mcpb.sh
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            clipboard-history-mcp.mcpb
            mcpb/server/clipboard-history-mcp
          prerelease: true
          generate_release_notes: true
```

- [ ] **Step 3: Update dependabot**

```yaml
version: 2
updates:
  - package-ecosystem: 'cargo'
    directory: '/'
    schedule: { interval: 'weekly' }
  - package-ecosystem: 'github-actions'
    directory: '/'
    schedule: { interval: 'weekly' }
```

- [ ] **Step 4: Commit**

```bash
git add .github/
git commit -m "ci: switch to cargo CI matrix; add release workflow; cargo dependabot"
```

---

## Task 25: README rewrite for v3

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite**

```markdown
# clipboard-history-mcp

Type-aware, secret-safe macOS clipboard history exposed to Claude via MCP.

> **v0.3.x** ships as a single Rust binary, packaged as `.mcpb` for one-click install in Claude Desktop. v0.2.x (Node.js) is on the [`v0.2.0-alpha.1` tag](https://github.com/d-khomenko/clipboard-history-mcp/releases/tag/v0.2.0-alpha.1).

## Why

- **Maccy** is great as a clipboard manager but doesn't talk to LLMs.
- The other ~20 `clipboard-mcp` repos on GitHub only read the *current* clipboard — no history.
- This project: type classification + secret detection + Touch ID gate + MCPB distribution. None of the alternatives combine these.

## Install

**Drag-and-drop (recommended):**

Download the latest `.mcpb` from the [releases page](https://github.com/d-khomenko/clipboard-history-mcp/releases) and drop it onto Claude Desktop. The launchd daemon starts automatically.

**From source:**

```bash
git clone https://github.com/d-khomenko/clipboard-history-mcp
cd clipboard-history-mcp
cargo build --release
./target/release/clipboard-history-mcp install
claude mcp add -s user clipboard-history -- "$(pwd)/target/release/clipboard-history-mcp" serve
```

**Homebrew (planned for v0.3.1).**

## Features

- Single binary, no Node.js or Swift toolchain required.
- 15 MCP tools: list/get/search/get_urls/get_code/get_json/get_secrets_index/unlock_secret/copy_item/pin_item/tag_item/delete_item/clear_history/get_stats/daemon_status.
- Touch ID gate on `unlock_secret`.
- launchd-managed daemon captures clips even when Claude is closed.
- 252 secret patterns from gitleaks + JWT + Luhn-validated credit cards.
- AES-256-GCM encryption with macOS Keychain biometric ACL.
- SQLite + FTS5 full-text search.

## Configuration

Set as launchd `EnvironmentVariables` (use `clipboard-history-mcp install --window-titles` for the common opt-in):

| Var | Default | Purpose |
|---|---|---|
| `CLIPBOARD_POLL_MS` | `1500` | watcher poll interval |
| `CLIPBOARD_HISTORY_MAX` | `1000` | ring-buffer size |
| `CLIPBOARD_CAPTURE_WINDOW_TITLE` | `0` | capture window titles (Accessibility prompt) |
| `CLIPBOARD_IGNORE_APPS` | `` | comma list of app display names to skip |
| `CLIPBOARD_NEVER_STORE_SECRETS` | `0` | metadata-only mode for secrets |

## License

MIT. See `LICENSE` and `vendor/README.md` for the gitleaks rule catalog (also MIT).

## See also

- [v3 design spec](docs/superpowers/specs/2026-05-05-clipboard-history-mcp-v3-rust-design.md)
- [v2 design spec](docs/superpowers/specs/2026-05-05-clipboard-history-mcp-v2-design.md) (Node.js implementation)
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README for v3 Rust + MCPB distribution"
```

---

## Task 26: Tag and ship v0.3.0-alpha.0

- [ ] **Step 1: Verify clean**

```bash
git status
cargo test --release
cargo clippy -- -D warnings
```

- [ ] **Step 2: Update CHANGELOG**

Prepend to `CHANGELOG.md`:

```markdown
## [0.3.0-alpha.0] — 2026-05-05

### Added
- Full Rust rewrite with single-binary distribution.
- MCPB packaging via `@anthropic-ai/mcpb` — drag-and-drop install in Claude Desktop.
- Native NSPasteboard + NSWorkspace via `objc2-app-kit` (no Swift helper).
- Native macOS Keychain via `security-framework` (no `security` CLI argv exposure — closes v2 critical issue).
- Touch ID gate on `unlock_secret` via `objc2-local-authentication` (closes v2 Phase 1.5).
- GitHub Actions matrix on macos-13/14 with cargo fmt/clippy/test.
- Tag-triggered release workflow building universal binary + MCPB.

### Removed
- Node.js dependency.
- Swift `pasteboard-types` helper.
- `security` CLI shell-out for Keychain.
```

- [ ] **Step 3: Push branch, merge, tag**

```bash
git add CHANGELOG.md
git commit -m "docs: CHANGELOG for 0.3.0-alpha.0"

git push -u origin v3-rust
git checkout main
git merge v3-rust --no-ff -m "Merge v3-rust: Rust rewrite + MCPB distribution"
git tag -a v0.3.0-alpha.0 -m "v0.3.0-alpha.0 — Rust rewrite, MCPB, Touch ID, native Keychain"
git push origin main
git push origin v0.3.0-alpha.0
```

The `release.yml` workflow runs on tag push and uploads the `.mcpb` to the GitHub release page.

- [ ] **Step 4: Verify release**

```bash
gh release view v0.3.0-alpha.0 2>&1 | head -10
```

Expected: prerelease entry exists; assets attached after CI completes.

---

## Self-review

**Spec coverage check:**

- §1 Why → README (Task 25), CHANGELOG (Task 26) ✓
- §3 Architecture → Tasks 6, 12, 14, 15 ✓
- §4 Crate selection → Task 1 (Cargo.toml) ✓
- §5 File structure → matches map at top of plan ✓
- §6 MCPB packaging → Tasks 23, 24 ✓
- §7 Migration from v2 → Task 21 ✓
- §8 Touch ID → Task 22 ✓
- §9 Testing → unit tests in tasks 3, 4, 5, 6, 7; integration in Task 17 ✓
- §10 CI/CD → Task 24 ✓
- §11 Open questions — addressed by stubs (biometry default Ok in Task 8 fixed by Task 22; window title noted as v0.3.1)

**Placeholder scan:** No "TBD"/"TODO"/"implement later" outside the documented v0.3.x roadmap items (window-title accessibility wiring is explicitly deferred with a written reason; biometry stub is replaced in Task 22).

**Type/name consistency:**
- `Store::open(path, master_key)` ✓ used in 3, 7, 15, 20
- `add_clip` / `add_secret` / `get_item` / `unlock_secret` ✓ consistent
- `WatcherOptions` field names match between Task 11 (struct), Task 12 (env var → struct mapping)
- `BiometryGate::evaluate(reason: &str) -> Result<bool>` ✓ same in stub (Task 8) and real (Task 22)
- 15 MCP tool names match v2 spec exactly

---

# Execution

**Plan complete and saved to `docs/superpowers/plans/2026-05-05-clipboard-history-mcp-v3-rust-implementation.md`.**

Two execution options:

1. **Subagent-Driven (recommended)** — same flow as v2: fresh subagent per task, batched where appropriate.
2. **Inline Execution** — execute tasks in this session.

Which approach?
