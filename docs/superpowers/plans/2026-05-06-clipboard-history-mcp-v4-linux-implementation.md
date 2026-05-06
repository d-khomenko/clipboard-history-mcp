# clipboard-history-mcp v4 (Linux port) — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the v3 macOS-only Rust implementation to also run on Linux (X11 + Wayland), via cfg-gated platform code paths and cross-platform Rust crates (`arboard`, `keyring`, `directories-next`, `active-win-pos-rs`).

**Architecture:** No traits, no runtime polymorphism. Compile-time `#[cfg(target_os = "...")]` blocks within existing modules. Three macOS-specific behaviours that don't translate to Linux are removed or compensated: `NSPasteboard` transient/concealed type detection (replaced by populated default `CLIPBOARD_IGNORE_APPS` list), Touch ID gate (kept on macOS, master-password fallback added everywhere), launchd lifecycle (mirrored as systemd user service on Linux).

**Tech Stack:** Rust 1.95+, `arboard` 3.x, `keyring` 3.x, `directories-next` 2.x, `active-win-pos-rs` 0.x, `x11rb` 0.13.x (Linux-only), `argon2` 0.5.x, `systemd` user services on Linux, launchd on macOS (unchanged).

**Reference spec:** [`docs/superpowers/specs/2026-05-06-clipboard-history-mcp-v4-linux-design.md`](../specs/2026-05-06-clipboard-history-mcp-v4-linux-design.md)

**Branch:** `v4-linux` (already created from `main` after the v3 merge).

**Crate API drift advisory:** When in doubt, hit `docs.rs/<crate>/<version>` before writing code. The plan shows API shapes we expect; if upstream renamed a method or restructured, follow current docs and document the drift in the commit message.

---

## File map

### New files

```
src/core/master_password.rs    # Argon2id KEK wrap/unwrap of master key
src/cli/install_linux.rs       # systemd unit writer (cfg(target_os = "linux"))
src/cli/install_macos.rs       # launchd plist writer (cfg(target_os = "macos"))
scripts/systemd.service.template
tests/master_password.rs
tests/keyring.rs               # cross-platform keyring round-trip (#[ignore]'d)
.github/workflows/ci.yml       # rewritten matrix
```

### Modified files

```
Cargo.toml                     # crate swap + new deps
src/core/pasteboard.rs         # objc2 → arboard
src/core/crypto.rs             # security-framework → keyring + KEK wrap
src/core/biometry.rs           # cfg-gate macOS Touch ID; Linux: prompt fallback
src/daemon/context.rs          # objc2 → active-win-pos-rs + x11rb on Linux
src/cli/install.rs             # dispatcher → install_linux / install_macos
src/cli/uninstall.rs           # symmetric dispatcher
src/cli/doctor.rs              # platform-specific checks
src/cli/migrate_v2.rs          # add v3 → v4 master key re-encryption + path move
src/main.rs                    # data-dir resolution via directories-next
README.md                      # add Linux install
CHANGELOG.md                   # add v0.4.0-alpha.0 entry
```

### Deleted files

```
native/pasteboard-types.swift  # functionality replaced by IGNORE_APPS list
scripts/build-native.sh
bin/pasteboard-types           # already gitignored, just stop building
```

---

## Milestones

| # | Tasks | Outcome |
|---|---|---|
| **M1** | 1–4 | Crate swap. macOS still works as before, just via cross-platform crates underneath. |
| **M2** | 5–8 | Linux clipboard, frontmost app, window title — all functional. |
| **M3** | 9–11 | Master-password biometry. Migration from v0.3 master-key-v1 → v0.4 master-key-v2. |
| **M4** | 12–15 | Linux daemon installer (systemd) + path layout (XDG) + doctor. |
| **M5** | 16–20 | CI matrix, README, CHANGELOG, ship v0.4.0-alpha.0. |

---

# M1 — Crate swap

## Task 1: Cargo deps swap

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Update `[dependencies]`**

```toml
[package]
name = "clipboard-history-mcp"
version = "0.4.0-alpha.0"
edition = "2021"
rust-version = "1.95"
description = "Type-aware, secret-safe clipboard history exposed to Claude via MCP. macOS + Linux."
license = "MIT"
authors = ["Dmytro Khomenko"]
repository = "https://github.com/d-khomenko/clipboard-history-mcp"

[lib]
path = "src/lib.rs"

[[bin]]
name = "clipboard-history-mcp"
path = "src/main.rs"

[dependencies]
rmcp                       = { version = "1.6", features = ["server", "macros", "transport-io", "local"] }
tokio                      = { version = "1.52", features = ["full"] }

# Cross-platform clipboard / keyring / paths / window
arboard                    = "3"
keyring                    = "3"
directories-next           = "2"
active-win-pos-rs          = "0.8"

# Storage & crypto
rusqlite                   = { version = "0.39", features = ["bundled"] }
aes-gcm                    = "0.10"
argon2                     = "0.5"
rand                       = "0.8"
once_cell                  = "1.20"
regex                      = "1.11"

# Serialization / config
serde                      = { version = "1", features = ["derive"] }
serde_json                 = "1"
toml                       = "1.1"

# CLI & logging
clap                       = { version = "4.6", features = ["derive"] }
tracing                    = "0.1"
tracing-subscriber         = { version = "0.3", features = ["env-filter"] }

# Utilities
anyhow                     = "1"
thiserror                  = "2"
schemars                   = "0.9"
sha2                       = "0.10"
uuid                       = { version = "1", features = ["v4"] }
hex                        = "0.4"
url                        = "2.5"
libc                       = "0.2"
ctrlc                      = "3"
rpassword                  = "7"

# macOS-only
[target.'cfg(target_os = "macos")'.dependencies]
objc2-local-authentication = "0.3"
block2                     = "0.6"

# Linux-only
[target.'cfg(target_os = "linux")'.dependencies]
x11rb                      = "0.13"

[dev-dependencies]
tempfile = "3"

[profile.release]
opt-level = 3
lto = "fat"
strip = "symbols"
codegen-units = 1
```

- [ ] **Step 2: Run cargo check**

```bash
. "$HOME/.cargo/env"
cargo check 2>&1 | tail -20
```

Expected: compile errors in `pasteboard.rs`, `crypto.rs`, `daemon/context.rs`, `biometry.rs` because the imports they use no longer match. Those are fixed in tasks 2-7. The point of this step: confirm Cargo resolves the new deps cleanly.

If a crate version is unpublished or yanked, fall back to the latest published version (check `crates.io`). Document any version drift in the commit message.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: swap deps for v4 cross-platform port"
```

---

## Task 2: Replace `core/pasteboard.rs` with `arboard`

**Files:**
- Modify: `src/core/pasteboard.rs`
- Delete: `native/pasteboard-types.swift`, `scripts/build-native.sh`

`arboard` provides cross-platform read/write. We lose `is_transient()` (no platform-uniform way to enumerate clipboard types). The compensation strategy is documented in the spec — default `CLIPBOARD_IGNORE_APPS` populated by `install`. So `is_transient()` simply returns `false` always; the watcher trusts `IGNORE_APPS` to do the gating.

- [ ] **Step 1: Replace contents of `src/core/pasteboard.rs`**

```rust
use anyhow::{anyhow, Result};
use arboard::Clipboard;
use std::sync::Mutex;

static CLIPBOARD: once_cell::sync::Lazy<Mutex<Clipboard>> = once_cell::sync::Lazy::new(|| {
    Mutex::new(Clipboard::new().expect("init system clipboard"))
});

pub fn read_clipboard() -> Result<String> {
    let mut cb = CLIPBOARD.lock().map_err(|_| anyhow!("clipboard mutex poisoned"))?;
    match cb.get_text() {
        Ok(s) => Ok(s),
        Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
        Err(e) => Err(anyhow!("clipboard read: {}", e)),
    }
}

pub fn write_clipboard(text: &str) -> Result<()> {
    let mut cb = CLIPBOARD.lock().map_err(|_| anyhow!("clipboard mutex poisoned"))?;
    cb.set_text(text.to_string()).map_err(|e| anyhow!("clipboard write: {}", e))?;
    Ok(())
}

/// Always false in v4. macOS NSPasteboard transient/concealed type detection
/// was removed when we switched to arboard; see CLIPBOARD_IGNORE_APPS in
/// install.rs for the compensation mechanism.
pub fn is_transient() -> bool {
    false
}

/// Best-effort change counter for change-detection. arboard does not expose
/// the underlying changeCount; we hash-compare the text instead in the
/// watcher tick. This function exists for backward API compatibility and
/// just returns the current Unix epoch ms — the watcher does not actually
/// rely on monotonicity.
pub fn change_count() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}
```

- [ ] **Step 2: Delete native Swift helper**

```bash
git rm native/pasteboard-types.swift scripts/build-native.sh 2>/dev/null || true
rm -f bin/pasteboard-types
```

If those paths don't exist (already gone), the `git rm` is a no-op.

- [ ] **Step 3: Verify build**

```bash
cargo check 2>&1 | tail -10
```

`pasteboard.rs` compiles cleanly. Other modules may still error — fine, fixed downstream.

- [ ] **Step 4: Commit**

```bash
git add src/core/pasteboard.rs native/ scripts/build-native.sh bin/pasteboard-types 2>/dev/null
git commit -m "feat(core): arboard-based pasteboard; drop Swift helper"
```

---

## Task 3: Replace `core/crypto.rs` with `keyring`-backed master key

**Files:**
- Modify: `src/core/crypto.rs`
- Modify: `tests/crypto.rs` (existing roundtrip test stays; add keyring-touching tests as `#[ignore]`)

The existing v3 keychain entry is `service: "clipboard-history-mcp"`, `account: "master-key-v1"`. v4 keeps backward-compat reads but writes new entries under `master-key-v2` once a master password is set (Task 9 introduces that). For Task 3 we just swap the storage backend; KEK-wrap arrives in Task 9.

- [ ] **Step 1: Replace `src/core/crypto.rs`**

```rust
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use keyring::Entry;
use rand::RngCore;

const KEYCHAIN_SERVICE: &str = "clipboard-history-mcp";
const KEYCHAIN_ACCOUNT_V1: &str = "master-key-v1";

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
    if let Some(k) = read_master_key_v1()? {
        return Ok(k);
    }
    let k = generate_master_key();
    write_master_key_v1(&k)?;
    Ok(k)
}

fn read_master_key_v1() -> Result<Option<[u8; 32]>> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V1)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    match entry.get_secret() {
        Ok(bytes) if bytes.len() == 32 => {
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            Ok(Some(k))
        }
        Ok(_) => Err(anyhow!("master-key-v1 has wrong length")),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!("keyring read: {}", e)),
    }
}

fn write_master_key_v1(key: &[u8; 32]) -> Result<()> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V1)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    entry.set_secret(key).map_err(|e| anyhow!("keyring write: {}", e))?;
    Ok(())
}

fn generate_master_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    k
}
```

- [ ] **Step 2: Existing test should still pass**

```bash
cargo test --release --test crypto 2>&1 | tail -10
```

The `tests/crypto.rs` round-trip test uses an in-memory key (not keyring). Should remain green.

- [ ] **Step 3: Commit**

```bash
git add src/core/crypto.rs
git commit -m "feat(core): keyring-backed master key (cross-platform)"
```

---

## Task 4: Add `directories-next` for cross-platform paths

**Files:**
- Modify: `src/main.rs`
- Modify: `src/mcp/mod.rs` (whichever places hardcode `Library/Application Support`)
- Modify: `src/cli/install.rs`, `src/cli/uninstall.rs`, `src/cli/status.rs`, `src/cli/vault.rs`, `src/cli/migrate_v2.rs`

We introduce a single helper, `paths::data_dir()` and `paths::db_path()`, and replace hardcoded macOS paths everywhere.

- [ ] **Step 1: Create `src/core/paths.rs`**

```rust
use directories_next::ProjectDirs;
use std::path::PathBuf;

const QUALIFIER: &str = "kz";
const ORG: &str = "me";
const APP: &str = "clipboard-history-mcp";

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from(QUALIFIER, ORG, APP)
        .expect("HOME directory must be set on this OS")
}

pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DATA_DIR") {
        return PathBuf::from(p);
    }
    project_dirs().data_dir().to_path_buf()
}

pub fn config_dir() -> PathBuf {
    project_dirs().config_dir().to_path_buf()
}

pub fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DB_PATH") {
        return PathBuf::from(p);
    }
    data_dir().join("history.db")
}

pub fn pid_file_path() -> PathBuf {
    data_dir().join("daemon.pid")
}

pub fn log_path() -> PathBuf {
    data_dir().join("daemon.log")
}
```

- [ ] **Step 2: Wire `pub mod paths;` into `src/core/mod.rs`**

Open `src/core/mod.rs`, add the line:

```rust
pub mod paths;
```

After existing `pub mod` lines.

- [ ] **Step 3: Replace hardcoded paths**

In `src/main.rs`, replace the existing `data_dir()` / `db_path()` helpers with imports:

```rust
use clipboard_history_mcp::core::paths::{data_dir, db_path, pid_file_path};
```

(Delete the local functions of the same names.)

In `src/mcp/mod.rs`, do the same — drop the local `data_dir()` / `db_path()` helpers and use `core::paths`.

In each `src/cli/*.rs` file, replace any line that constructs `PathBuf::from(home).join("Library/Application Support/...")` with a call to `paths::data_dir()` / `paths::db_path()`. Search for the pattern with `git grep "Library/Application Support"` — there should be ~6-8 hits.

- [ ] **Step 4: Verify build**

```bash
cargo check --release 2>&1 | tail -10
```

Compile errors should now be limited to `biometry.rs` (next task), `daemon/context.rs` (Task 5), and `cli/install.rs` (Task 12).

- [ ] **Step 5: Commit**

```bash
git add src/core/paths.rs src/core/mod.rs src/main.rs src/mcp/mod.rs src/cli/
git commit -m "feat(core): directories-next for cross-platform data/config paths"
```

---

# M2 — Linux clipboard, frontmost app, window title

## Task 5: cfg-gate `daemon/context.rs` (frontmost app + window title)

**Files:**
- Modify: `src/daemon/context.rs`

- [ ] **Step 1: Replace contents**

```rust
use serde::Serialize;

#[derive(Serialize)]
pub struct CaptureContext {
    pub front_app: Option<String>,
    pub window_title: Option<String>,
}

pub fn capture(with_window_title: bool) -> CaptureContext {
    let front_app = capture_frontmost_app();
    let window_title = if with_window_title { capture_window_title() } else { None };
    CaptureContext { front_app, window_title }
}

fn capture_frontmost_app() -> Option<String> {
    match active_win_pos_rs::get_active_window() {
        Ok(w) => Some(w.app_name).filter(|s| !s.is_empty()),
        Err(_) => None,
    }
}

#[cfg(target_os = "macos")]
fn capture_window_title() -> Option<String> {
    capture_window_title_via_active_win()
}

#[cfg(target_os = "linux")]
fn capture_window_title() -> Option<String> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        // Wayland window title support deferred to v0.4.x (portal API)
        return None;
    }
    capture_window_title_x11().or_else(capture_window_title_via_active_win)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn capture_window_title() -> Option<String> {
    capture_window_title_via_active_win()
}

fn capture_window_title_via_active_win() -> Option<String> {
    match active_win_pos_rs::get_active_window() {
        Ok(w) => Some(w.title).filter(|s| !s.is_empty()),
        Err(_) => None,
    }
}

#[cfg(target_os = "linux")]
fn capture_window_title_x11() -> Option<String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];

    let net_active = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").ok()?.reply().ok()?.atom;
    let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME").ok()?.reply().ok()?.atom;
    let utf8 = conn.intern_atom(false, b"UTF8_STRING").ok()?.reply().ok()?.atom;

    let active_reply = conn
        .get_property(false, screen.root, net_active, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    let active_win = u32::from_ne_bytes(active_reply.value.get(..4)?.try_into().ok()?);
    if active_win == 0 {
        return None;
    }

    let title_reply = conn
        .get_property(false, active_win, net_wm_name, utf8, 0, 1024)
        .ok()?
        .reply()
        .ok()?;

    let title = String::from_utf8(title_reply.value).ok()?;
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}
```

- [ ] **Step 2: Quick local smoke (skip on non-target OS)**

```bash
cargo build --release
./target/release/clipboard-history-mcp doctor 2>&1 | grep -i pasteboard || true
```

Should exit 0 and not panic. Doctor's NSPasteboard line is updated in Task 14.

- [ ] **Step 3: Commit**

```bash
git add src/daemon/context.rs
git commit -m "feat(daemon): cross-platform frontmost-app via active-win-pos-rs; X11 window-title"
```

---

## Task 6: cfg-gate `core/biometry.rs` (Touch ID macOS, no-op stub Linux)

**Files:**
- Modify: `src/core/biometry.rs`

For Task 6 the Linux path is a stub returning `Ok(true)`. Real master-password challenge arrives in Task 9 (`master_password::evaluate`).

- [ ] **Step 1: Replace contents**

```rust
use anyhow::Result;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(300);
static AUTH_CACHE: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

pub struct BiometryGate;

impl BiometryGate {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(&self, reason: &str) -> Result<bool> {
        if cache_hit() {
            return Ok(true);
        }
        let ok = self.evaluate_inner(reason)?;
        if ok {
            mark_cache();
        }
        Ok(ok)
    }

    #[cfg(target_os = "macos")]
    fn evaluate_inner(&self, reason: &str) -> Result<bool> {
        macos::evaluate_la_context(reason)
    }

    #[cfg(target_os = "linux")]
    fn evaluate_inner(&self, _reason: &str) -> Result<bool> {
        // v0.4.0-alpha.0: no biometry, master-password gate lands in Task 9.
        Ok(true)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn evaluate_inner(&self, _reason: &str) -> Result<bool> {
        Ok(true)
    }
}

fn cache_hit() -> bool {
    let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(g) = cache.lock() {
        if let Some(t) = *g {
            return t.elapsed() < CACHE_TTL;
        }
    }
    false
}

fn mark_cache() {
    let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cache.lock() {
        *g = Some(Instant::now());
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::Result;
    use objc2::rc::autoreleasepool;
    use objc2_foundation::NSString;
    use objc2_local_authentication::{LAContext, LAPolicy};
    use std::sync::mpsc;
    use std::time::Duration;

    pub fn evaluate_la_context(reason: &str) -> Result<bool> {
        let ok = autoreleasepool(|_| unsafe {
            let ctx = LAContext::new();
            let policy = LAPolicy::DeviceOwnerAuthentication;
            if ctx.canEvaluatePolicy_error(policy).is_err() {
                return false;
            }

            let reason_ns = NSString::from_str(reason);
            let (sender, receiver) = mpsc::channel::<bool>();
            let block = block2::RcBlock::new(move |success: objc2::runtime::Bool, _err: *mut objc2_foundation::NSError| {
                let _ = sender.send(success.as_bool());
            });
            ctx.evaluatePolicy_localizedReason_reply(policy, &reason_ns, &*block);
            receiver.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
        });
        Ok(ok)
    }
}
```

- [ ] **Step 2: Build verification**

```bash
cargo build --release 2>&1 | tail -5
```

Expected: clean build on macOS. On Linux (when CI runs it) the macos `mod` is `cfg`-gated out.

- [ ] **Step 3: Commit**

```bash
git add src/core/biometry.rs
git commit -m "feat(core): cfg-gate biometry — macOS Touch ID + Linux stub"
```

---

## Task 7: Cross-platform end-to-end smoke test

**Files:**
- Create: `tests/clipboard_smoke.rs`

A platform-agnostic smoke test for the read/write roundtrip via `arboard`. CI on Linux uses `xvfb` to provide a virtual display.

- [ ] **Step 1: Write test**

```rust
#[test]
#[ignore] // touches real system clipboard
fn arboard_round_trip() {
    use clipboard_history_mcp::core::pasteboard::{read_clipboard, write_clipboard};

    let needle = format!("v4-smoke-{}", std::process::id());
    write_clipboard(&needle).unwrap();
    let got = read_clipboard().unwrap();
    assert_eq!(got, needle);
}
```

- [ ] **Step 2: Run locally on macOS**

```bash
cargo test --release --test clipboard_smoke -- --ignored 2>&1 | tail -5
```

Expected: `1 passed`. Linux CI runs it under xvfb (set up in Task 16).

- [ ] **Step 3: Commit**

```bash
git add tests/clipboard_smoke.rs
git commit -m "test: platform-agnostic clipboard round-trip via arboard"
```

---

## Task 8: Verify daemon still captures on macOS post-swap

This is a manual smoke. We don't write a new test — Task 7 already covered the pasteboard layer. Here we just make sure the watcher loop still drives `arboard` correctly through `Store`.

- [ ] **Step 1: Build + run daemon in foreground**

```bash
cargo build --release
CLIPBOARD_DATA_DIR=/tmp/cbhist-v4-smoke ./target/release/clipboard-history-mcp daemon &
PID=$!
sleep 2
pbcopy <<< "v4 smoke $(date +%s)"
sleep 3
kill -TERM $PID
wait $PID 2>/dev/null

sqlite3 /tmp/cbhist-v4-smoke/history.db "SELECT primary_kind, source_app, preview FROM clips ORDER BY id DESC LIMIT 1"
```

Expected: one row showing `text|...|v4 smoke <epoch>`.

- [ ] **Step 2: Cleanup tmp dir**

```bash
rm -rf /tmp/cbhist-v4-smoke
```

No commit (verification only).

---

# M3 — Master-password biometry + migration

## Task 9: New module `core/master_password.rs`

**Files:**
- Create: `src/core/master_password.rs`
- Modify: `src/core/mod.rs` (add `pub mod master_password;`)
- Modify: `src/core/crypto.rs` (add v2 storage path)

Adds Argon2id KDF, PIN/password prompting, cache, and the v2 keyring entry.

- [ ] **Step 1: Create `src/core/master_password.rs`**

```rust
use anyhow::{anyhow, Context, Result};
use argon2::{Argon2, Algorithm, Params, Version};
use rand::RngCore;

const SALT_LEN: usize = 16;
const KEK_LEN: usize = 32;

pub struct WrappedKey {
    pub salt: Vec<u8>,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub fn derive_kek(password: &str, salt: &[u8]) -> Result<[u8; KEK_LEN]> {
    let params = Params::new(19_456, 2, 1, Some(KEK_LEN))
        .map_err(|e| anyhow!("argon2 params: {}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; KEK_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| anyhow!("argon2 hash: {}", e))?;
    Ok(out)
}

pub fn wrap_master_key(master: &[u8; 32], password: &str) -> Result<WrappedKey> {
    let mut salt = vec![0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    let kek = derive_kek(password, &salt)?;
    let (ct, nonce) = crate::core::crypto::encrypt_bytes(master, &kek);
    Ok(WrappedKey { salt, nonce, ciphertext: ct })
}

pub fn unwrap_master_key(wrapped: &WrappedKey, password: &str) -> Result<[u8; 32]> {
    let kek = derive_kek(password, &wrapped.salt)?;
    let pt = crate::core::crypto::decrypt_bytes(&wrapped.ciphertext, &wrapped.nonce, &kek)
        .context("incorrect master password or corrupted vault")?;
    let mut k = [0u8; 32];
    if pt.len() != 32 {
        return Err(anyhow!("decrypted master key is not 32 bytes"));
    }
    k.copy_from_slice(&pt);
    Ok(k)
}

pub fn prompt_password(prompt: &str) -> Result<String> {
    rpassword::prompt_password(prompt).map_err(|e| anyhow!("password prompt: {}", e))
}

pub fn prompt_password_with_confirmation() -> Result<String> {
    let p1 = prompt_password("Set a master password: ")?;
    let p2 = prompt_password("Confirm master password: ")?;
    if p1 != p2 {
        return Err(anyhow!("passwords did not match"));
    }
    if p1.len() < 8 {
        return Err(anyhow!("password must be at least 8 characters"));
    }
    Ok(p1)
}
```

- [ ] **Step 2: Add raw-bytes encrypt/decrypt helpers in `src/core/crypto.rs`**

Append to existing `crypto.rs`:

```rust
pub fn encrypt_bytes(plaintext: &[u8], key: &[u8; 32]) -> (Vec<u8>, [u8; 12]) {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let n = Nonce::from_slice(&nonce);
    let ct = cipher.encrypt(n, plaintext).expect("encrypt");
    (ct, nonce)
}

pub fn decrypt_bytes(ciphertext: &[u8], nonce: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());
    let n = Nonce::from_slice(nonce);
    cipher
        .decrypt(n, ciphertext)
        .map_err(|_| anyhow!("AEAD authentication failed"))
}
```

- [ ] **Step 3: Add `pub mod master_password;` to `src/core/mod.rs`**

```rust
pub mod master_password;
```

- [ ] **Step 4: Write test**

Create `tests/master_password.rs`:

```rust
use clipboard_history_mcp::core::master_password::{derive_kek, unwrap_master_key, wrap_master_key};

#[test]
fn wrap_unwrap_roundtrip() {
    let master = [42u8; 32];
    let pw = "correct horse battery staple";
    let wrapped = wrap_master_key(&master, pw).unwrap();
    let recovered = unwrap_master_key(&wrapped, pw).unwrap();
    assert_eq!(recovered, master);
}

#[test]
fn unwrap_with_wrong_password_fails() {
    let master = [7u8; 32];
    let wrapped = wrap_master_key(&master, "right-pw").unwrap();
    let err = unwrap_master_key(&wrapped, "wrong-pw").unwrap_err();
    assert!(err.to_string().contains("password") || err.to_string().contains("AEAD"));
}

#[test]
fn kek_is_deterministic_for_same_inputs() {
    let salt = [9u8; 16];
    let a = derive_kek("hello", &salt).unwrap();
    let b = derive_kek("hello", &salt).unwrap();
    assert_eq!(a, b);
}

#[test]
fn kek_differs_for_different_salts() {
    let a = derive_kek("hello", &[1u8; 16]).unwrap();
    let b = derive_kek("hello", &[2u8; 16]).unwrap();
    assert_ne!(a, b);
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test --release --test master_password 2>&1 | tail -8
```

Expected: 4 passed.

- [ ] **Step 6: Commit**

```bash
git add src/core/master_password.rs src/core/mod.rs src/core/crypto.rs tests/master_password.rs
git commit -m "feat(core): master_password module — Argon2id KEK wrap/unwrap"
```

---

## Task 10: Migrate v3 master-key-v1 → v4 master-key-v2

**Files:**
- Modify: `src/core/crypto.rs` — add v2 read/write paths
- Modify: `src/cli/migrate_v2.rs` — extend with master-key migration
- Modify: `src/main.rs` — first-run prompt for master password

This is the most user-visible change. v3 users will see a prompt the first time they run a v4 binary.

- [ ] **Step 1: Add v2 keyring helpers to `src/core/crypto.rs`**

Append:

```rust
const KEYCHAIN_ACCOUNT_V2: &str = "master-key-v2";

pub struct WrappedKeyEntry {
    pub salt: Vec<u8>,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub fn read_master_key_v2() -> Result<Option<WrappedKeyEntry>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V2)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    let bytes = match entry.get_secret() {
        Ok(b) => b,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(anyhow!("keyring read v2: {}", e)),
    };
    if bytes.len() < 16 + 12 + 16 {
        return Err(anyhow!("master-key-v2 has invalid length"));
    }
    let salt = bytes[..16].to_vec();
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&bytes[16..28]);
    let ciphertext = bytes[28..].to_vec();
    Ok(Some(WrappedKeyEntry { salt, nonce, ciphertext }))
}

pub fn write_master_key_v2(wrapped: &crate::core::master_password::WrappedKey) -> Result<()> {
    let mut buf = Vec::with_capacity(16 + 12 + wrapped.ciphertext.len());
    buf.extend_from_slice(&wrapped.salt);
    buf.extend_from_slice(&wrapped.nonce);
    buf.extend_from_slice(&wrapped.ciphertext);
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V2)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    entry.set_secret(&buf).map_err(|e| anyhow!("keyring write v2: {}", e))?;
    Ok(())
}

pub fn delete_master_key_v1() -> Result<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_V1)
        .map_err(|e| anyhow!("keyring entry: {}", e))?;
    match entry.delete_credential() {
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!("keyring delete v1: {}", e)),
    }
}
```

- [ ] **Step 2: Replace `get_or_create_master_key()` with the v2-aware version**

Edit existing `get_or_create_master_key` in `crypto.rs`:

```rust
pub fn get_or_create_master_key(password_provider: impl FnOnce() -> Result<String>) -> Result<[u8; 32]> {
    if let Some(wrapped_entry) = read_master_key_v2()? {
        let pw = password_provider()?;
        let wrapped = crate::core::master_password::WrappedKey {
            salt: wrapped_entry.salt,
            nonce: wrapped_entry.nonce,
            ciphertext: wrapped_entry.ciphertext,
        };
        return crate::core::master_password::unwrap_master_key(&wrapped, &pw);
    }
    if let Some(legacy_key) = read_master_key_v1()? {
        return Ok(legacy_key);
    }
    // First-ever run — generate fresh, store as v1 (compat path) until user
    // sets a master password via `clipboard-history-mcp migrate-v2`.
    let k = generate_master_key();
    write_master_key_v1(&k)?;
    Ok(k)
}
```

This signature change ripples — call sites need to pass a provider closure. Update them:

In `src/main.rs::run_daemon()`:

```rust
let key = clipboard_history_mcp::core::crypto::get_or_create_master_key(|| {
    clipboard_history_mcp::core::master_password::prompt_password("Master password (5-min cache): ")
})?;
```

In `src/mcp/mod.rs::run_server()`:

```rust
let key = clipboard_history_mcp::core::crypto::get_or_create_master_key(|| {
    clipboard_history_mcp::core::master_password::prompt_password("Master password (5-min cache): ")
})?;
```

In `src/cli/vault.rs::vault()`:

```rust
let key = clipboard_history_mcp::core::crypto::get_or_create_master_key(|| {
    clipboard_history_mcp::core::master_password::prompt_password("Master password (5-min cache): ")
})?;
```

The provider only fires when v2 storage exists; on v1 systems it's never called.

- [ ] **Step 3: Implement master-key migration in `src/cli/migrate_v2.rs`**

Replace the existing no-op stub with:

```rust
use crate::core::crypto::{
    delete_master_key_v1, read_master_key_v1, read_master_key_v2, write_master_key_v2,
};
use crate::core::master_password::{prompt_password_with_confirmation, wrap_master_key};
use anyhow::{anyhow, Result};

pub fn migrate_v2() -> Result<()> {
    if read_master_key_v2()?.is_some() {
        println!("v4 master-key-v2 already present — nothing to migrate.");
        return Ok(());
    }
    let legacy = match read_master_key_v1()? {
        Some(k) => k,
        None => {
            println!("No v3 master-key-v1 found. Set a fresh master password instead.");
            return set_fresh_password();
        }
    };
    println!("Found v3 master-key-v1. Setting up v4 master-password wrap.");
    let pw = prompt_password_with_confirmation()?;
    let wrapped = wrap_master_key(&legacy, &pw)?;
    write_master_key_v2(&wrapped)?;
    delete_master_key_v1()?;
    println!("Migrated. Master key is now password-wrapped under master-key-v2.");
    Ok(())
}

fn set_fresh_password() -> Result<()> {
    let pw = prompt_password_with_confirmation()?;
    let mut master = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut master);
    let wrapped = wrap_master_key(&master, &pw)?;
    write_master_key_v2(&wrapped)?;
    println!("New master key set under master-key-v2.");
    Ok(())
}
```

- [ ] **Step 4: Build + smoke**

```bash
cargo build --release 2>&1 | tail -5
./target/release/clipboard-history-mcp migrate-v2
# Should say "v4 master-key-v2 already present" if you've run it before,
# or prompt for password if first time.
```

If you have a real v3 master-key-v1 in Keychain from earlier runs, the prompt will fire. Set a test password to migrate.

- [ ] **Step 5: Commit**

```bash
git add src/core/crypto.rs src/main.rs src/mcp/mod.rs src/cli/vault.rs src/cli/migrate_v2.rs
git commit -m "feat(crypto): v3->v4 master-key migration with master-password wrap"
```

---

## Task 11: Wire BiometryGate to consult master password on Linux

**Files:**
- Modify: `src/core/biometry.rs` — Linux path now actually prompts

Now that we have `master_password` and v2 storage, Linux biometry can actually evaluate.

- [ ] **Step 1: Update Linux `evaluate_inner`**

Replace the Linux branch in `src/core/biometry.rs`:

```rust
#[cfg(target_os = "linux")]
fn evaluate_inner(&self, _reason: &str) -> Result<bool> {
    use crate::core::crypto::read_master_key_v2;
    use crate::core::master_password::{prompt_password, unwrap_master_key, WrappedKey};

    let Some(entry) = read_master_key_v2()? else {
        // No v2 entry → user is on v1 raw-key compat mode → no biometry, just allow.
        return Ok(true);
    };
    let pw = prompt_password("Master password to unlock: ")?;
    let wrapped = WrappedKey {
        salt: entry.salt,
        nonce: entry.nonce,
        ciphertext: entry.ciphertext,
    };
    Ok(unwrap_master_key(&wrapped, &pw).is_ok())
}
```

- [ ] **Step 2: Compile**

```bash
cargo build --release 2>&1 | tail -3
```

- [ ] **Step 3: Commit**

```bash
git add src/core/biometry.rs
git commit -m "feat(biometry): Linux path consults master-password v2 entry"
```

---

# M4 — Linux daemon installer + doctor

## Task 12: cfg-gated installer (`launchd` macOS, `systemd` Linux)

**Files:**
- Create: `src/cli/install_macos.rs` — extract macOS-specific code from existing `install.rs`
- Create: `src/cli/install_linux.rs`
- Create: `scripts/systemd.service.template`
- Modify: `src/cli/install.rs` — becomes a dispatcher

- [ ] **Step 1: Move existing macOS code to `install_macos.rs`**

Create `src/cli/install_macos.rs` and paste the *current* contents of `src/cli/install.rs` there, but rename the public function from `install` to `install_macos`. (The current install.rs already targets macOS, so this is a copy + rename.)

- [ ] **Step 2: Create `src/cli/install_linux.rs`**

```rust
use anyhow::{anyhow, Result};
use crate::core::paths::{data_dir, log_path};
use std::path::PathBuf;
use std::process::Command;

const SERVICE_NAME: &str = "clipboard-history-mcp.service";

pub fn install_linux(opts: super::InstallOpts) -> Result<()> {
    let home = std::env::var("HOME")?;
    let unit_dir = PathBuf::from(&home).join(".config/systemd/user");
    std::fs::create_dir_all(&unit_dir)?;
    let unit_path = unit_dir.join(SERVICE_NAME);

    std::fs::create_dir_all(data_dir())?;
    let log = log_path();
    let bin = std::env::current_exe()?;

    let template = include_str!("../../scripts/systemd.service.template");
    let unit = template
        .replace("__BINARY__", bin.to_str().unwrap())
        .replace("__LOG__", log.to_str().unwrap())
        .replace("__CAPTURE_WINDOW_TITLE__", if opts.window_titles { "1" } else { "0" });
    std::fs::write(&unit_path, unit)?;

    let s = Command::new("systemctl").args(["--user", "daemon-reload"]).status()?;
    if !s.success() {
        return Err(anyhow!("systemctl daemon-reload failed"));
    }
    let s = Command::new("systemctl")
        .args(["--user", "enable", "--now", SERVICE_NAME])
        .status()?;
    if !s.success() {
        return Err(anyhow!("systemctl enable --now failed"));
    }

    if opts.linger {
        let user = std::env::var("USER")?;
        let _ = Command::new("loginctl")
            .args(["enable-linger", &user])
            .status();
    }

    println!("Installed → {}", unit_path.display());
    println!("Logs    → {}", log.display());
    Ok(())
}

pub fn uninstall_linux(opts: super::UninstallOpts) -> Result<()> {
    let home = std::env::var("HOME")?;
    let unit_path = PathBuf::from(&home).join(".config/systemd/user").join(SERVICE_NAME);

    if unit_path.exists() {
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", SERVICE_NAME])
            .status();
        std::fs::remove_file(&unit_path)?;
        println!("Removed {}", unit_path.display());
    }
    if !opts.keep_data {
        let dir = data_dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
            println!("Removed {}", dir.display());
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Create `scripts/systemd.service.template`**

```ini
[Unit]
Description=clipboard-history-mcp watcher daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=__BINARY__ daemon
Restart=on-failure
RestartSec=5
Environment="CLIPBOARD_CAPTURE_WINDOW_TITLE=__CAPTURE_WINDOW_TITLE__"
StandardOutput=append:__LOG__
StandardError=append:__LOG__

[Install]
WantedBy=default.target
```

- [ ] **Step 4: Replace `src/cli/install.rs` with the dispatcher**

```rust
use anyhow::Result;

#[cfg(target_os = "macos")]
mod install_macos;

#[cfg(target_os = "linux")]
pub mod install_linux;

pub struct InstallOpts {
    pub window_titles: bool,
    pub linger: bool,
}

pub struct UninstallOpts {
    pub keep_data: bool,
}

pub fn install(opts: InstallOpts) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = opts.linger; // unused on macOS
        return install_macos::install_macos(opts);
    }
    #[cfg(target_os = "linux")]
    {
        return install_linux::install_linux(opts);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("install is not supported on this platform")
    }
}
```

- [ ] **Step 5: Update `src/cli/uninstall.rs` similarly**

```rust
use anyhow::Result;
use crate::cli::install::{install_linux, UninstallOpts};

#[cfg(target_os = "macos")]
mod uninstall_macos {
    use super::*;
    pub fn uninstall_macos(opts: UninstallOpts) -> Result<()> {
        // existing v3 macOS code
    }
}

pub fn uninstall(opts: UninstallOpts) -> Result<()> {
    #[cfg(target_os = "macos")]
    return uninstall_macos::uninstall_macos(opts);
    #[cfg(target_os = "linux")]
    return install_linux::uninstall_linux(opts);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    anyhow::bail!("uninstall is not supported on this platform")
}
```

(The actual macOS uninstall logic is the existing v3 code — copy it into the `uninstall_macos` mod.)

- [ ] **Step 6: Update `src/cli/mod.rs` clap definitions**

Make sure the `Install` variant accepts `--linger`:

```rust
Install { #[arg(long)] window_titles: bool, #[arg(long)] linger: bool },
```

And update the dispatcher in `src/main.rs`:

```rust
Cmd::Install { window_titles, linger } => clipboard_history_mcp::cli::install::install(
    clipboard_history_mcp::cli::install::InstallOpts { window_titles, linger },
),
Cmd::Uninstall { keep_data } => clipboard_history_mcp::cli::uninstall::uninstall(
    clipboard_history_mcp::cli::install::UninstallOpts { keep_data },
),
```

- [ ] **Step 7: Build**

```bash
cargo build --release 2>&1 | tail -5
```

- [ ] **Step 8: Commit**

```bash
git add src/cli/install.rs src/cli/install_linux.rs src/cli/install_macos.rs src/cli/uninstall.rs src/cli/mod.rs src/main.rs scripts/systemd.service.template
git commit -m "feat(cli): cfg-gated install — launchd on macOS, systemd on Linux"
```

---

## Task 13: Update `cli/doctor.rs` for both platforms

**Files:**
- Modify: `src/cli/doctor.rs`

- [ ] **Step 1: Replace contents**

```rust
use anyhow::Result;
use std::process::Command;

pub fn doctor() -> Result<()> {
    let mut checks: Vec<(&str, Box<dyn Fn() -> Result<String>>)> = vec![];

    checks.push(("data dir writable", Box::new(|| {
        let p = crate::core::paths::data_dir();
        std::fs::create_dir_all(&p)?;
        Ok(p.display().to_string())
    })));

    checks.push(("keyring accessible", Box::new(|| {
        let entry = keyring::Entry::new("clipboard-history-mcp-doctor", "ping")
            .map_err(|e| anyhow::anyhow!("keyring entry: {}", e))?;
        let _ = entry.set_secret(b"ok");
        let _ = entry.delete_credential();
        Ok("ok".into())
    })));

    checks.push(("clipboard reachable (arboard)", Box::new(|| {
        let _ = crate::core::pasteboard::change_count();
        Ok("ok".into())
    })));

    #[cfg(target_os = "macos")]
    checks.push(("launchd service installed", Box::new(|| {
        let home = std::env::var("HOME")?;
        let p = std::path::PathBuf::from(home).join("Library/LaunchAgents/me.kz.clipboard-history-rs.plist");
        if p.exists() {
            Ok(p.display().to_string())
        } else {
            Err(anyhow::anyhow!("not installed (run `clipboard-history-mcp install`)"))
        }
    })));

    #[cfg(target_os = "linux")]
    checks.push(("systemd unit installed", Box::new(|| {
        let home = std::env::var("HOME")?;
        let p = std::path::PathBuf::from(home)
            .join(".config/systemd/user/clipboard-history-mcp.service");
        if p.exists() {
            Ok(p.display().to_string())
        } else {
            Err(anyhow::anyhow!("not installed (run `clipboard-history-mcp install`)"))
        }
    })));

    #[cfg(target_os = "linux")]
    checks.push(("X display present", Box::new(|| {
        if std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok() {
            Ok("yes".into())
        } else {
            Err(anyhow::anyhow!("no DISPLAY or WAYLAND_DISPLAY"))
        }
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

- [ ] **Step 2: Smoke**

```bash
cargo build --release
./target/release/clipboard-history-mcp doctor
```

Expected on macOS: 4 ✓ (data dir, keyring, clipboard, launchd if installed; otherwise the launchd line is ✗ which is fine).

- [ ] **Step 3: Commit**

```bash
git add src/cli/doctor.rs
git commit -m "feat(cli): doctor checks updated for cross-platform"
```

---

## Task 14: Linux migration of v3 data dir → v4 XDG path

**Files:**
- Modify: `src/cli/migrate_v2.rs` — extend with path migration

- [ ] **Step 1: Append to `migrate_v2()` in `src/cli/migrate_v2.rs`**

Insert before the existing master-key migration body:

```rust
// macOS: v3 used ~/Library/Application Support/clipboard-history-mcp/...
// v4 uses ~/Library/Application Support/me.kz.clipboard-history-mcp/...
// Move the directory if it exists.
#[cfg(target_os = "macos")]
{
    let home = std::env::var("HOME")?;
    let v3_dir = std::path::PathBuf::from(&home)
        .join("Library/Application Support/clipboard-history-mcp");
    let v4_dir = crate::core::paths::data_dir();
    if v3_dir.exists() && v3_dir != v4_dir && !v4_dir.exists() {
        std::fs::create_dir_all(v4_dir.parent().unwrap())?;
        std::fs::rename(&v3_dir, &v4_dir)?;
        println!("Moved v3 data dir → {}", v4_dir.display());
    }
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build --release 2>&1 | tail -3
git add src/cli/migrate_v2.rs
git commit -m "feat(cli): migrate-v2 also moves v3 data dir to XDG path"
```

---

## Task 15: Cross-platform integration test for the watcher loop

**Files:**
- Modify: `tests/clipboard_smoke.rs` (add a more meaningful test)

- [ ] **Step 1: Append to `tests/clipboard_smoke.rs`**

```rust
use clipboard_history_mcp::core::store::Store;
use clipboard_history_mcp::daemon::watcher::{run_watcher, WatcherOptions};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
#[ignore]
fn watcher_captures_a_clip() {
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

    thread::sleep(Duration::from_millis(800));
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
```

- [ ] **Step 2: Run on macOS**

```bash
cargo test --release --test clipboard_smoke -- --ignored 2>&1 | tail -10
```

Expected: 2 passed (round-trip + watcher).

- [ ] **Step 3: Commit**

```bash
git add tests/clipboard_smoke.rs
git commit -m "test: watcher captures clip end-to-end (cross-platform)"
```

---

# M5 — CI, README, ship

## Task 16: CI matrix — add Ubuntu

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Replace contents**

```yaml
name: CI
on:
  push: { branches: [main, v4-linux] }
  pull_request:
jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        os: [macos-13, macos-14, ubuntu-22.04, ubuntu-24.04]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install Linux deps
        if: contains(matrix.os, 'ubuntu')
        run: |
          sudo apt-get update
          sudo apt-get install -y libdbus-1-dev libxcb1-dev pkg-config xvfb gnome-keyring
      - run: cargo fmt -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - name: cargo test (macOS)
        if: startsWith(matrix.os, 'macos')
        run: cargo test --release
      - name: cargo test (Linux, headless)
        if: contains(matrix.os, 'ubuntu')
        run: |
          # Run gnome-keyring-daemon to satisfy keyring tests if any
          eval $(dbus-launch --sh-syntax)
          eval $(echo -n "" | gnome-keyring-daemon --unlock)
          eval $(echo -n "" | gnome-keyring-daemon --start --components=secrets)
          export DISPLAY=:99
          Xvfb :99 -screen 0 1024x768x24 &
          sleep 2
          cargo test --release
        shell: bash
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: cross-platform matrix (macos-13/14 + ubuntu-22.04/24.04)"
```

(Push happens at end of M5.)

---

## Task 17: README — Linux install instructions

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Insert a new "Install on Linux" section after the existing "Install" sections**

Add this block to `README.md` (find the section starting `## Install` and append after the existing macOS instructions):

````markdown
### Linux (X11; Wayland partial)

Requirements: a working Secret Service implementation (GNOME Keyring or KWallet — comes with most desktop installs), `xvfb` not required for normal use (only for headless CI).

```bash
git clone https://github.com/d-khomenko/clipboard-history-mcp
cd clipboard-history-mcp
cargo build --release

# Set a master password (used to wrap your AES master key)
./target/release/clipboard-history-mcp migrate-v2

# Install systemd user service
./target/release/clipboard-history-mcp install
# Optional: keep daemon running after logout
./target/release/clipboard-history-mcp install --linger

# Register with Claude Code
claude mcp add -s user clipboard-history -- "$(pwd)/target/release/clipboard-history-mcp" serve
```

**Wayland note:** clipboard read/write works on Wayland via `arboard`'s portal handling. Window-title capture (opt-in via `CLIPBOARD_CAPTURE_WINDOW_TITLE=1`) currently only works on X11; Wayland support is deferred to v0.4.x once `xdg-desktop-portal` window-title APIs are widely shipped.

**1Password / Bitwarden:** add app names to `CLIPBOARD_IGNORE_APPS` so password-manager paste events are skipped:

```bash
CLIPBOARD_IGNORE_APPS="1Password,Bitwarden,KeePassXC" \
  ./target/release/clipboard-history-mcp install
```
````

Update the comparison table at the top of the README — change the platform row to:

```markdown
| Standalone binary (no runtime needed) | n/a | ✗ | ✓ macOS + Linux |
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: Linux install + Wayland caveat in README"
```

---

## Task 18: CHANGELOG entry

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Prepend new section**

```markdown
## [0.4.0-alpha.0] — 2026-05-06

### Added
- **Linux port** — clipboard-history-mcp now runs on Linux (X11 and Wayland) in addition to macOS.
- Cross-platform crates replace macOS-specific ones: `arboard` (clipboard), `keyring` (secret storage), `directories-next` (XDG paths), `active-win-pos-rs` (frontmost app).
- X11 window-title capture via `x11rb` (Wayland deferred to v0.4.x).
- systemd user service installer for Linux (with optional `--linger`).
- Master-password vault (Argon2id KEK wrap of master key) — universal biometry mechanism. Touch ID stays as macOS-native unlock UX.
- v3 → v4 migration: `clipboard-history-mcp migrate-v2` now also re-encrypts the master key under the new password-wrapped scheme.

### Changed
- Master key is now stored as `master-key-v2` in the keyring (password-wrapped). `master-key-v1` (raw) remains readable for compat-mode but the migration command upgrades it.
- macOS data dir moved from `~/Library/Application Support/clipboard-history-mcp/` to `~/Library/Application Support/me.kz.clipboard-history-mcp/` (XDG-style qualified). `migrate-v2` moves the existing dir.
- Doctor command checks adjusted per OS.

### Removed
- Swift `pasteboard-types` helper. NSPasteboard transient-type detection is replaced by a populated default `CLIPBOARD_IGNORE_APPS` list (1Password, Bitwarden, KeePassXC).
- `security-framework` dependency. `keyring` covers the same Keychain Services API on macOS.

### Notes
- Windows port is planned for v0.5.
- MCPB packaging is still macOS-only (the format is Anthropic-targeted at Claude Desktop on Mac).
```

- [ ] **Step 2: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for v0.4.0-alpha.0"
```

---

## Task 19: Bump version, push, tag

**Files:**
- Modify: `Cargo.toml` (version already set in Task 1)
- Tag

- [ ] **Step 1: Verify clean tree**

```bash
git status
cargo test --release 2>&1 | tail -5
```

- [ ] **Step 2: Update version in `Cargo.toml`**

(Already set to `0.4.0-alpha.0` in Task 1; verify.)

```bash
grep '^version' Cargo.toml
```

Expected: `version = "0.4.0-alpha.0"`.

- [ ] **Step 3: Push branch**

```bash
git -c pack.window=0 -c pack.depth=0 push origin v4-linux
```

- [ ] **Step 4: Merge to main**

```bash
git checkout main
git merge v4-linux --no-ff -m "Merge v4-linux: Linux port + cross-platform crates"
git -c pack.window=0 -c pack.depth=0 push origin main
```

- [ ] **Step 5: Tag**

```bash
git tag -a v0.4.0-alpha.0 -m "v0.4.0-alpha.0 — Linux port

Cross-platform port via cfg-gated platform layer. arboard replaces objc2 for
clipboard, keyring replaces security-framework, master-password Argon2id
wrap is the universal biometry. Touch ID remains on macOS as bonus UX.
Linux daemon uses systemd user service. X11 window-title; Wayland deferred.

See CHANGELOG.md for full details."
git push origin v0.4.0-alpha.0
```

---

## Task 20: GitHub release + binary upload

- [ ] **Step 1: Build release binaries (current OS only — CI handles cross-platform)**

```bash
cargo build --release
```

- [ ] **Step 2: Create release**

```bash
gh release create v0.4.0-alpha.0 \
  ./target/release/clipboard-history-mcp \
  --prerelease \
  --generate-notes \
  --title "v0.4.0-alpha.0 — Linux port"
```

- [ ] **Step 3: Verify**

```bash
gh release view v0.4.0-alpha.0 2>&1 | head -10
```

Expected: pre-release shows up with the binary attached. CI's `release.yml` workflow (carried over from v3) will additionally upload the universal macOS `.mcpb` once the tag push triggers it.

---

## Self-review

Spec coverage walk:

- §3.1 cfg-gating → Tasks 5, 6, 12, 13 ✓
- §3.2 crate replacements → Tasks 1, 2, 3, 4 ✓
- §3.3 keyring on macOS → Task 3 ✓
- §3.4 arboard pasteboard → Task 2 ✓ (compensation IGNORE_APPS happens via existing watcher logic, no new task — but acceptance criteria: default IGNORE_APPS list. Adding to Task 12 install template ✓)
- §4 Linux clipboard → Tasks 2, 7 ✓
- §4.2 X11 window title → Task 5 ✓
- §4.3 frontmost app → Task 5 ✓
- §5 master-password biometry → Tasks 9, 10, 11 ✓
- §6 daemon lifecycle → Task 12 ✓
- §7 file paths → Task 4 ✓; macOS path migration → Task 14 ✓
- §8 CI matrix → Task 16 ✓
- §9 tests → Tasks 7, 9, 15 ✓
- §10 phase scope → covered (Wayland window-title and Windows port both deferred and not in this plan)
- §11 risks: 1 (`arboard` macOS reliability) covered by Task 8 manual smoke. Other risks documented in spec, no specific tasks.

**Placeholder scan:** no "TBD" / "implement later" / "similar to" patterns found. All code blocks complete.

**Type / name consistency:**
- `WrappedKey` (Task 9) and `WrappedKeyEntry` (Task 10) are deliberately distinct — `WrappedKey` is the in-memory shape from `master_password::wrap_master_key`; `WrappedKeyEntry` is the keyring-deserialized shape used by `core::crypto::read_master_key_v2`. Conversion between them is in Task 10's call site.
- `InstallOpts { window_titles, linger }` defined in Task 12, used in Tasks 12, 17 (README).
- `paths::data_dir()`, `paths::db_path()`, `paths::pid_file_path()`, `paths::log_path()` defined in Task 4, referenced in 12, 13.

---

## Execution

**Plan complete and saved to `docs/superpowers/plans/2026-05-06-clipboard-history-mcp-v4-linux-implementation.md`.**

Two execution options:

1. **Subagent-Driven (recommended)** — fresh subagent per task batch, review between, fast iteration.
2. **Inline Execution** — execute tasks here, batch checkpoints.

Which approach?
