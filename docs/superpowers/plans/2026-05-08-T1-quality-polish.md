# T1 Quality Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the v0.6 quick-win quality polish from the codebase audit — replace `.expect()` / `.unwrap()` reachable from production paths, remove dead code, fix the `is_transient()` silent regression, update stale strings — so first-time readers of the source see code that matches the "secure clipboard" promise.

**Architecture:** Eleven small, sequential commits on a single feature branch (`feat/v0.6-T1-quality-polish`) off `main`. No schema changes, no MCP tool changes, no public-API breaks visible to MCP clients. The one signature change (`encrypt` and `encrypt_bytes` return `Result`) is internal — all callers are in this repo. NSPasteboard query for `is_transient()` is added under `#[cfg(target_os = "macos")]` and gated by a new `objc2-app-kit = "0.3"` target-specific dep.

**Tech Stack:** Rust 1.95, `aes-gcm`, `arboard`, `objc2` family (existing), `objc2-app-kit` (new — macOS only), `tracing`, `tempfile` (dev), `cargo test`, `cargo clippy`.

**Spec reference:** [`docs/superpowers/specs/2026-05-08-v0.6-adoption-roadmap-design.md`](../specs/2026-05-08-v0.6-adoption-roadmap-design.md) §3 T1.

---

## Pre-flight

Before Task 1, the engineer must be on a fresh branch off `origin/main`:

```bash
cd /Users/stock/dev/clipboard-history-mcp
git fetch origin main
git checkout -b feat/v0.6-T1-quality-polish origin/main
```

All paths in this plan are relative to that repo root.

---

## Task 1: Make `crypto::encrypt` return `Result`

**Why.** `src/core/crypto.rs:31` calls `.expect("encrypt")`. AES-256-GCM encrypt cannot legitimately fail given a valid 32-byte key + 12-byte nonce, but `.expect` aborts the daemon process. Convert to `Result` so any future change in caller assumptions surfaces as a normal error path.

**Files:**
- Modify: `src/core/crypto.rs:26-33` (function signature + body)
- Modify: `src/core/store.rs:95` (caller — adapt to `?`)
- Modify: `tests/crypto.rs:1-31` (existing tests need `.unwrap()` on the new `Result`)

- [ ] **Step 1: Read existing tests, run them green to establish baseline**

```bash
cargo test --test crypto -- --nocapture
```

Expected: 3 tests pass (`roundtrip_utf8`, `rejects_tampered_ciphertext`, `rejects_wrong_key`).

- [ ] **Step 2: Add a new test asserting `encrypt` returns a `Result`**

Add to `tests/crypto.rs`:

```rust
#[test]
fn encrypt_returns_result_not_panic() {
    let key = [0u8; 32];
    // Even on the impossible-error path the type must be Result so the
    // compiler proves we never panic in production.
    let r: Result<(Vec<u8>, [u8; 12]), anyhow::Error> = encrypt("test", &key);
    assert!(r.is_ok());
}
```

- [ ] **Step 3: Run the new test — should fail to compile (signature is currently `(Vec<u8>, [u8; 12])`, not `Result`)**

```bash
cargo test --test crypto encrypt_returns_result_not_panic
```

Expected: compile error — `mismatched types: expected Result<...>, found tuple`.

- [ ] **Step 4: Change `encrypt` signature in `src/core/crypto.rs`**

Replace lines 26-33:

```rust
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12])> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let n = Nonce::from_slice(&nonce);
    let ct = cipher
        .encrypt(n, plaintext.as_bytes())
        .map_err(|_| anyhow!("AEAD encrypt failed"))?;
    Ok((ct, nonce))
}
```

- [ ] **Step 5: Update the caller in `src/core/store.rs:95`**

Find the existing call site:

```bash
grep -n "encrypt(" src/core/store.rs
```

The line is in `add_secret`. Change the existing pattern from:

```rust
let (ciphertext, nonce) = encrypt(&input.text, &self.master_key);
```

to:

```rust
let (ciphertext, nonce) = encrypt(&input.text, &self.master_key)?;
```

- [ ] **Step 6: Update existing tests in `tests/crypto.rs` to `.unwrap()` the new `Result`**

Each existing test that has `let (ciphertext, nonce) = encrypt(...)` needs `.unwrap()`. Replace all three occurrences:

```rust
let (ciphertext, nonce) = encrypt(plaintext, &key).unwrap();
// ...
let (mut ct, nonce) = encrypt("hello", &key).unwrap();
// ...
let (ct, nonce) = encrypt("hello", &k1).unwrap();
```

- [ ] **Step 7: Run all crypto tests — must pass**

```bash
cargo test --test crypto
```

Expected: 4 tests pass (3 existing + the new one).

- [ ] **Step 8: Build the whole crate to make sure no other caller broke**

```bash
cargo check
```

Expected: clean compile, no errors, no new warnings.

- [ ] **Step 9: Commit**

```bash
git add src/core/crypto.rs src/core/store.rs tests/crypto.rs
git commit -m "fix(crypto): encrypt returns Result instead of panicking on AEAD failure

AES-256-GCM cannot legitimately fail given valid key+nonce length, but
.expect() aborts the daemon process on the impossible-but-not-checked
path. Propagate as a normal error so future callers can't accidentally
trigger a panic in a launchd-managed service.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Make `crypto::encrypt_bytes` return `Result`

**Why.** Same reasoning as Task 1, applied to the byte-array variant used by `master_password::wrap_master_key`. `src/core/crypto.rs:101` calls `.expect("encrypt")`.

**Files:**
- Modify: `src/core/crypto.rs:96-103` (function signature + body)
- Modify: `src/core/master_password.rs:32` (caller)

- [ ] **Step 1: Add a unit test inside `src/core/crypto.rs` asserting the new signature**

Add at the bottom of the file (before `#[cfg(test)]` if any, or create a new test module):

```rust
#[cfg(test)]
mod encrypt_bytes_tests {
    use super::*;

    #[test]
    fn encrypt_bytes_returns_result() {
        let key = [0u8; 32];
        let r: Result<(Vec<u8>, [u8; 12])> = encrypt_bytes(b"hello", &key);
        assert!(r.is_ok());
    }
}
```

- [ ] **Step 2: Run the new test — should fail to compile**

```bash
cargo test --lib core::crypto::encrypt_bytes_tests
```

Expected: compile error on the `Result` type annotation.

- [ ] **Step 3: Change `encrypt_bytes` signature**

Replace lines 96-103 of `src/core/crypto.rs`:

```rust
pub fn encrypt_bytes(plaintext: &[u8], key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12])> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let n = Nonce::from_slice(&nonce);
    let ct = cipher
        .encrypt(n, plaintext)
        .map_err(|_| anyhow!("AEAD encrypt failed"))?;
    Ok((ct, nonce))
}
```

- [ ] **Step 4: Update the caller in `src/core/master_password.rs:32`**

Change:

```rust
let (ct, nonce) = crate::core::crypto::encrypt_bytes(master, &kek);
```

to:

```rust
let (ct, nonce) = crate::core::crypto::encrypt_bytes(master, &kek)?;
```

- [ ] **Step 5: Run the master_password test suite to make sure it still passes**

```bash
cargo test --test master_password
```

Expected: all tests still green.

- [ ] **Step 6: Run the full library tests**

```bash
cargo test --lib
```

Expected: clean run, including the new `encrypt_bytes_returns_result` test.

- [ ] **Step 7: Commit**

```bash
git add src/core/crypto.rs src/core/master_password.rs
git commit -m "fix(crypto): encrypt_bytes returns Result instead of panicking

Mirror Task 1: convert the byte-array AES-GCM helper from .expect() to
Result propagation. Sole caller is wrap_master_key in master_password.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Replace `.unwrap()` in `mcp/tools.rs:275` (`copy_item`)

**Why.** `item.text.as_deref().unwrap()` is guarded by an `is_none` early-return on line 267, so the unwrap is currently unreachable. But the surrounding code does not make that obvious — a future edit that drops the guard would re-introduce a panic in the MCP server. Refactor to a single `if let` so the safety is structurally enforced.

**Files:**
- Modify: `src/mcp/tools.rs:264-286` (function `copy_item`)

- [ ] **Step 1: Run mcp tests to baseline**

```bash
cargo test --test mcp_smoke
```

Expected: all currently-non-ignored tests pass (some are `#[ignore]`'d and don't run by default — that's fine).

- [ ] **Step 2: Refactor `copy_item` to use pattern-matching on `item.text`**

Replace the body of the `Ok(Some(item))` arm (lines 266-282) with:

```rust
Ok(Some(item)) => {
    let Some(text) = item.text.as_deref() else {
        return serde_json::json!({
            "error": "cannot restore secret directly",
            "requiresUnlock": true
        })
        .to_string();
    };
    if let Err(e) = pasteboard::write_clipboard(text) {
        return format!("error: {}", e);
    }
    let _ = self.store.bump_paste(p.id);
    serde_json::json!({ "ok": true, "id": p.id, "length": item.length }).to_string()
}
```

- [ ] **Step 3: Run mcp tests again — must still pass**

```bash
cargo test --test mcp_smoke
```

Expected: same green result as Step 1.

- [ ] **Step 4: Confirm no `.unwrap()` remains in `copy_item`**

```bash
grep -A 25 'async fn copy_item' src/mcp/tools.rs | grep -F 'unwrap'
```

Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "refactor(mcp): copy_item uses let-else instead of guarded unwrap

The unwrap was unreachable today (guarded by is_none early return on
the line above) but a future edit could drop the guard. Switch to a
let-else so the safety is structurally enforced.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Replace `.unwrap()` in `mcp/tools.rs:359` (`daemon_status`)

**Why.** `self.db_path.parent().unwrap()` panics if `db_path` is somehow a bare filename (no parent component). In practice it never is, but there is no invariant in the type system that guarantees it. Use a fallback.

**Files:**
- Modify: `src/mcp/tools.rs:357-380` (function `daemon_status`)

- [ ] **Step 1: Identify the line**

```bash
grep -n 'parent()' src/mcp/tools.rs
```

Expected: one hit on line 359 (or near it after Task 3's refactor adjusted line numbers).

- [ ] **Step 2: Replace `.unwrap()` with `.unwrap_or_else(|| std::path::Path::new("."))`**

Find:

```rust
let pid_file = self.db_path.parent().unwrap().join("daemon.pid");
```

Replace with:

```rust
let pid_file = self
    .db_path
    .parent()
    .unwrap_or_else(|| std::path::Path::new("."))
    .join("daemon.pid");
```

- [ ] **Step 3: Build to verify**

```bash
cargo check
```

Expected: clean.

- [ ] **Step 4: Run mcp tests**

```bash
cargo test --test mcp_smoke
```

Expected: still green.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "refactor(mcp): daemon_status falls back to '.' instead of panicking

self.db_path.parent() is None only when db_path is a bare filename. We
do not currently construct one that way, but the type system does not
guarantee it; fall back to the current directory.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Replace `.unwrap()` in `core/vault.rs:192` (`append_daily`)

**Why.** `s.find(&header).unwrap()` is guarded by the surrounding `s.contains(&header)` check on line 189. Brittle — if the guard ever moves, this panics inside the daemon's vault-write path on every capture.

**Files:**
- Modify: `src/core/vault.rs:184-210` (the `Some(s) if s.contains(&header)` arm)

- [ ] **Step 1: Run vault_mirror tests to baseline**

```bash
cargo test --test vault_mirror
```

Expected: 6 tests pass.

- [ ] **Step 2: Restructure the match arm to compute `section_start` from the `find()` result directly**

Replace the existing `Some(s) if s.contains(&header)` arm (lines 188-208) with:

```rust
// Case 3 (common): section already there — locate it and insert.
Some(s) => {
    let Some(section_start) = s.find(&header) else {
        // Section header missing despite Some(s) — fall through to the
        // "create section" path. Should not happen in practice; defensive.
        let combined = format!("{}\n\n{}\n{}", s.trim_end(), header, line);
        write_atomically(&path, &combined)?;
        return Ok(());
    };
    let after_header = section_start + header.len();
    let next_h2 = s[after_header..]
        .find("\n## ")
        .map(|i| after_header + i + 1)
        .unwrap_or(s.len());
    let mut before = s[..next_h2].trim_end().to_string();
    let after = &s[next_h2..];
    let _ = writeln!(before, "\n{}", line);
    if after.is_empty() {
        before
    } else {
        format!("{}\n\n{}", before, after.trim_start_matches('\n'))
    }
}
```

Note: this change merges `Some(s) if s.contains(...)` with the no-section case by using `let-else`. Verify the surrounding `match existing { ... }` no longer needs the separate "section missing inside Some(s)" arm — in the original code that case was implicit fall-through to the `None` branch via `if`-guard non-match.

- [ ] **Step 3: Run vault_mirror tests again — must still pass**

```bash
cargo test --test vault_mirror
```

Expected: 6 tests pass, same as before.

- [ ] **Step 4: Confirm no `.unwrap()` remains in `append_daily`**

```bash
grep -A 30 'fn append_daily' src/core/vault.rs | grep -F 'unwrap'
```

Expected: no output (or only `unwrap_or` / `unwrap_or_else` which are safe).

- [ ] **Step 5: Commit**

```bash
git add src/core/vault.rs
git commit -m "refactor(vault): append_daily uses let-else instead of guarded unwrap

The unwrap was protected by an outer s.contains(&header) check, but if
the contains guard ever drifts the daemon would panic on every vault
write. Restructure as let-else; the structurally-impossible None case
falls through to the recreate-section path defensively.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Remove dead code — `core::paths::config_dir`

**Why.** `pub fn config_dir()` in `src/core/paths.rs:20` has no consumers in the codebase (verified via `grep -rn config_dir src/ tests/`). It is a leftover from earlier scaffolding. Remove it. (Note: `change_count` in `pasteboard.rs` is **not** dead — `cli/doctor.rs:38` calls it. Leave it alone.)

**Files:**
- Modify: `src/core/paths.rs:20-22` (delete the function)

- [ ] **Step 1: Confirm one more time that nothing uses it**

```bash
grep -rn 'config_dir' /Users/stock/dev/clipboard-history-mcp/src /Users/stock/dev/clipboard-history-mcp/tests
```

Expected: only the definition site (`src/core/paths.rs:20-21`). No callers.

- [ ] **Step 2: Delete lines 20-22 of `src/core/paths.rs`**

Remove these lines:

```rust
pub fn config_dir() -> PathBuf {
    project_dirs().config_dir().to_path_buf()
}
```

- [ ] **Step 3: Build and run all tests to confirm nothing breaks**

```bash
cargo check && cargo test --lib && cargo test --tests
```

Expected: clean compile, all tests still pass.

- [ ] **Step 4: Commit**

```bash
git add src/core/paths.rs
git commit -m "chore(paths): drop unused config_dir export

config_dir was scaffolded but never consumed (verified with grep across
src/ and tests/). Remove it; the day a config file lands we can
reintroduce a focused version next to its first caller.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: Update CLI `about` string for cross-platform reality

**Why.** `src/cli/mod.rs:12` reads `about = "macOS clipboard history MCP"`. Linux has shipped since v0.4.

**Files:**
- Modify: `src/cli/mod.rs:12`

- [ ] **Step 1: Edit `src/cli/mod.rs:12`**

Replace:

```rust
#[command(name = "clipboard-history-mcp", version, about = "macOS clipboard history MCP")]
```

with:

```rust
#[command(name = "clipboard-history-mcp", version, about = "Cross-platform clipboard history MCP")]
```

- [ ] **Step 2: Build and verify the help text**

```bash
cargo build --release
./target/release/clipboard-history-mcp --help | head -3
```

Expected: second line reads `Cross-platform clipboard history MCP`.

- [ ] **Step 3: Commit**

```bash
git add src/cli/mod.rs
git commit -m "docs(cli): about string says cross-platform, not just macOS

Linux has shipped since v0.4. The clap about string was a leftover from
when this was macOS-only.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Make biometry's non-macOS / non-Linux fallback warn loudly

**Why.** `src/core/biometry.rs:62-65` returns `Ok(true)` on unsupported platforms (Windows, FreeBSD, etc.) without logging. If a Windows port lands without updating biometry, biometric protection silently disables. Add a `tracing::warn!` so that case is visible in the daemon log.

**Files:**
- Modify: `src/core/biometry.rs:62-65`

- [ ] **Step 1: Replace the fallback with a warning**

Change:

```rust
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn evaluate_inner(&self, _reason: &str) -> Result<bool> {
    Ok(true)
}
```

to:

```rust
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn evaluate_inner(&self, _reason: &str) -> Result<bool> {
    tracing::warn!(
        "biometry gate is not implemented on this platform; \
         secret unlock is permitted without authentication. \
         Track at https://github.com/d-khomenko/clipboard-history-mcp/issues"
    );
    Ok(true)
}
```

- [ ] **Step 2: Build to confirm the change compiles on macOS (the cfg branch is for "anything else" so it will not be exercised, but the syntax must be valid)**

```bash
cargo check
```

Expected: clean compile, no warnings.

- [ ] **Step 3: Commit**

```bash
git add src/core/biometry.rs
git commit -m "fix(biometry): non-macOS/non-Linux fallback warns loudly

The pre-existing cfg-fallback returned Ok(true) silently. If a Windows
or FreeBSD port lands without updating this module, biometric
protection silently disables. Add a tracing::warn! so the gap is
visible in daemon logs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Add `objc2-app-kit` dependency for `is_transient()` fix

**Why.** Restoring the macOS NSPasteboard transient-type check requires reading the pasteboard's `types` array and looking for `org.nspasteboard.ConcealedType` / `com.apple.is-sensitive`. `objc2-app-kit` exposes the `NSPasteboard` Obj-C class. The crate is target-gated so it does not bloat Linux builds.

**Files:**
- Modify: `Cargo.toml` — add to the `[target.'cfg(target_os = "macos")'.dependencies]` block.

- [ ] **Step 1: Locate the macOS-only dep block**

```bash
grep -n 'cfg(target_os = "macos")' Cargo.toml
```

Expected: one hit identifying the start of the block. Existing entries are `objc2`, `objc2-foundation`, `objc2-local-authentication`, `block2`.

- [ ] **Step 2: Add the new dependency**

Append to that block:

```toml
objc2-app-kit             = { version = "0.3", features = ["NSPasteboard"] }
```

The `features = ["NSPasteboard"]` selector keeps the linked surface tight — the crate is feature-gated and we only need the pasteboard subset.

- [ ] **Step 3: Verify the dep resolves**

```bash
cargo check
```

Expected: clean compile. The new dep downloads on first run; that is normal.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(deps): add objc2-app-kit (macOS only) for NSPasteboard

Target-gated dependency; Linux + Windows builds are unaffected. Used
in the next commit to restore the is_transient() check that was
removed in v0.4 when we switched the cross-platform clipboard layer
to arboard.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Fix `is_transient()` regression — restore NSPasteboard concealed-type check on macOS

**Why.** `src/core/pasteboard.rs:27-29` returns `false` unconditionally. This was the primary mechanism for skipping password-manager copies pre-v0.4; the `CLIPBOARD_IGNORE_APPS` env var was meant to be a *secondary* defence. Today the daemon captures any text that 1Password / Bitwarden / KeePassXC writes to the pasteboard with `org.nspasteboard.ConcealedType`, even if the source app is not on the ignore list.

The fix queries `NSPasteboard.generalPasteboard().types()` and returns `true` if any of the well-known concealed-type UTIs is present.

**Files:**
- Modify: `src/core/pasteboard.rs` — add a `#[cfg(target_os = "macos")] mod macos_pb` and route `is_transient()` through it.

**Reference UTIs.** macOS clipboard managers honor (in order of historical adoption):
- `org.nspasteboard.ConcealedType` — community convention, used by 1Password, Bitwarden, Maccy, Pastebot.
- `org.nspasteboard.AutoGeneratedType` — auto-generated content (e.g. paste from a notification).
- `com.apple.is-sensitive` — Apple-internal, sometimes set by Keychain Access.

We treat all three as "transient = true" (skip capture).

- [ ] **Step 1: Write a unit test for the cross-platform fallback**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/core/pasteboard.rs`:

```rust
#[test]
#[cfg(not(target_os = "macos"))]
fn is_transient_false_on_non_macos() {
    // Linux / Windows / etc. have no NSPasteboard; the fallback must be
    // false so the watcher does not silently skip every clip.
    assert!(!is_transient());
}
```

- [ ] **Step 2: Run the test on Linux (or via cargo on macOS — the cfg gate keeps it out of the macOS build, so it will simply be skipped there)**

```bash
cargo test --lib pasteboard
```

Expected: on macOS the test is excluded by cfg; on Linux it passes. Either way, no failures.

- [ ] **Step 3: Add the macOS implementation module to `src/core/pasteboard.rs`**

Append at the bottom of the file (before `#[cfg(test)]`):

```rust
#[cfg(target_os = "macos")]
mod macos_pb {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;

    /// Pasteboard type UTIs that signal "do not record" by community or
    /// Apple convention.
    const CONCEALED_UTIS: &[&str] = &[
        "org.nspasteboard.ConcealedType",
        "org.nspasteboard.AutoGeneratedType",
        "com.apple.is-sensitive",
    ];

    pub fn is_transient_macos() -> bool {
        autoreleasepool(|_| unsafe {
            let pb = NSPasteboard::generalPasteboard();
            let types = pb.types();
            let Some(types) = types else { return false };
            for uti in CONCEALED_UTIS {
                let needle = NSString::from_str(uti);
                if types.containsObject(&needle) {
                    return true;
                }
            }
            false
        })
    }
}
```

- [ ] **Step 4: Replace the body of `is_transient()` to call into the macOS module**

Replace lines 24-29 (the existing `is_transient` function and its doc comment):

```rust
/// Returns `true` if the current pasteboard carries a "concealed" UTI
/// (org.nspasteboard.ConcealedType / AutoGeneratedType /
/// com.apple.is-sensitive). On macOS this queries NSPasteboard via
/// `objc2-app-kit`. Other platforms return `false`; CLIPBOARD_IGNORE_APPS
/// is the cross-platform compensation mechanism.
pub fn is_transient() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_pb::is_transient_macos()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
```

- [ ] **Step 5: Build and run tests**

```bash
cargo check
cargo test --lib pasteboard
```

Expected: clean compile, fallback test passes (or is excluded on macOS).

- [ ] **Step 6: On macOS, manually verify the fix end-to-end**

```bash
# 1Password / Bitwarden copy a password to the pasteboard; verify the
# daemon logs "skip: transient pasteboard type" instead of capturing.
cargo build --release
./target/release/clipboard-history-mcp uninstall
./target/release/clipboard-history-mcp install
# Use 1Password to copy a password.
sleep 2
tail -20 ~/Library/Application\ Support/kz.me.clipboard-history-mcp/daemon.log
```

Expected: a `WARN`-or-`INFO`-level line containing `skip: transient pasteboard type`. The current line is in `src/daemon/watcher.rs:82-85`.

- [ ] **Step 7: Commit**

```bash
git add src/core/pasteboard.rs
git commit -m "fix(pasteboard): restore is_transient() NSPasteboard query on macOS

is_transient() returned false unconditionally since v0.4, which meant
password-manager pastes (1Password, Bitwarden, KeePassXC) bypassed
the primary skip-mechanism and relied entirely on CLIPBOARD_IGNORE_APPS.
That's a hole — the ignore list only covers known apps.

Restore the NSPasteboard.types() check via objc2-app-kit. We honor
three concealed UTIs:
- org.nspasteboard.ConcealedType (community convention)
- org.nspasteboard.AutoGeneratedType
- com.apple.is-sensitive (Apple-internal)

Linux / Windows fallback is still false; the cross-platform layer
keeps relying on CLIPBOARD_IGNORE_APPS until each platform gets its
own native query.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Update CHANGELOG, run final integration check, push branch

**Why.** A single CHANGELOG entry summarizes the whole T1 track for release notes. The integration check (full test suite + `clippy`) catches anything missed during per-task verification.

**Files:**
- Modify: `CHANGELOG.md` — add to the `[Unreleased]` section.

- [ ] **Step 1: Update `CHANGELOG.md`**

Find the `## [Unreleased]` section. Add (or extend) a `### Fixed` and `### Changed` block:

```markdown
## [Unreleased]

### Fixed
- **`is_transient()` silent regression on macOS** — since v0.4 the function returned `false` unconditionally, meaning password-manager pastes (1Password, Bitwarden, KeePassXC) bypassed the primary skip mechanism and relied entirely on `CLIPBOARD_IGNORE_APPS`. Restored via `NSPasteboard.types()` query honoring `org.nspasteboard.ConcealedType`, `AutoGeneratedType`, and `com.apple.is-sensitive`.
- **Panic-free crypto** — `crypto::encrypt` and `crypto::encrypt_bytes` now return `Result<(Vec<u8>, [u8; 12])>` instead of `.expect()`-ing on the impossible-but-not-checked AEAD-failure path. Daemon will surface a normal error rather than aborting if a future change ever invalidates the assumption.
- **Defensive refactors** in `mcp::tools::copy_item`, `mcp::tools::daemon_status`, and `core::vault::append_daily` — replaced guarded `.unwrap()` patterns with `let-else` / `unwrap_or_else` so the safety invariants are structural rather than positional.
- **Biometry fallback** on non-macOS / non-Linux now logs a `tracing::warn!` instead of silently returning `Ok(true)`. Visible to operators if a Windows or BSD port lands without porting biometry.

### Changed
- CLI `about` string is now `Cross-platform clipboard history MCP` (was `macOS clipboard history MCP`). Linux has shipped since v0.4.
- New target-gated dependency: `objc2-app-kit = "0.3"` (macOS only). Used by the `is_transient()` fix above.

### Removed
- Unused `pub fn config_dir() -> PathBuf` in `core::paths`. No callers in the codebase. The day a config file lands we can reintroduce a focused version next to its first caller.
```

- [ ] **Step 2: Run the entire test suite**

```bash
cargo test
```

Expected: every test passes (lib + crypto + master_password + secrets + store + types + vault_mirror + clipboard_smoke). Some are `#[ignore]`'d (system-clipboard, Touch ID); they remain ignored.

- [ ] **Step 3: Run clippy with all targets**

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: no warnings, no errors. If clippy flags new lints introduced by the refactors, fix them and amend the relevant earlier commit.

- [ ] **Step 4: Sanity-grep for any remaining `.expect()` / `.unwrap()` that could be reached from production paths**

```bash
grep -nE '\.(expect|unwrap)\(' src/main.rs src/daemon/ src/mcp/ src/core/{crypto,store,vault,pasteboard,biometry}.rs | grep -v 'unwrap_or' | grep -v '#\[cfg(test)' | grep -v '// '
```

Expected: only the two known init-time hits below survive; everything else has been converted by Tasks 1–10.

**Acknowledged init-time `.expect()` calls deferred to v0.7:**

1. `src/core/crypto.rs:22` — `keyring::use_native_store(true).expect("failed to init keyring store")` inside `ensure_keyring_store()`. Lives in a `OnceCell::get_or_init` closure (no `Result` return). Converting requires changing `ensure_keyring_store` to return `Result` and propagating through `read_master_key_v1`, `read_master_key_v2`, `write_master_key_v1`, `write_master_key_v2`, `delete_master_key_v1` — five call sites. Out of T1's quick-win scope.

2. `src/core/pasteboard.rs:6` — `Mutex::new(Clipboard::new().expect("init system clipboard"))` inside the `CLIPBOARD: Lazy<...>` static initialiser. Same constraint: `Lazy::new` takes an `FnOnce() -> T`, not `FnOnce() -> Result<T, _>`. Conversion would mean replacing `Lazy` with manual `OnceCell<Result<...>>` and threading the error through `read_clipboard` / `write_clipboard`. Out of T1 scope.

If the grep flags **any other** hit not in this list, evaluate it and either fold into T1 (preferred — extend the plan with a new task) or document the deferral in the PR description.

- [ ] **Step 5: Commit the CHANGELOG update**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): T1 quality polish — fixed/changed/removed entries

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 6: Push the branch and open the PR**

```bash
git push -u origin feat/v0.6-T1-quality-polish
gh pr create --base main --title "feat(v0.6): T1 quality polish" --body "$(cat <<'EOF'
## Summary

T1 from the [v0.6 adoption-first roadmap](docs/superpowers/specs/2026-05-08-v0.6-adoption-roadmap-design.md). Eleven small commits landing the codebase-audit quick-wins so first-time readers see code that matches the "secure clipboard" promise.

### Fixed
- `is_transient()` silent regression on macOS (since v0.4): NSPasteboard concealed-type check restored via objc2-app-kit. Closes the password-manager-paste hole that the `CLIPBOARD_IGNORE_APPS` env var alone could not cover.
- Panic-free crypto: `encrypt` and `encrypt_bytes` return `Result` instead of `.expect()`-ing on the impossible AEAD-failure path. Sole production callers are `store::add_secret` and `master_password::wrap_master_key`; both updated to propagate.
- Guarded-unwrap refactors in `mcp::tools::copy_item`, `mcp::tools::daemon_status`, and `core::vault::append_daily` — switched to `let-else` / `unwrap_or_else` so the invariants survive future edits.
- Biometry fallback on non-macOS / non-Linux logs a `tracing::warn!` instead of silently returning `Ok(true)`.

### Changed
- CLI `about` string: `Cross-platform clipboard history MCP` (was `macOS clipboard history MCP`).
- New target-gated dep: `objc2-app-kit = "0.3"` (macOS only).

### Removed
- Unused `core::paths::config_dir`.

## Test plan

- [x] `cargo test` — all suites green
- [x] `cargo clippy --all-targets -- -D warnings` — clean
- [ ] Manual on macOS: `1Password` → copy a password, verify daemon log shows `skip: transient pasteboard type` instead of a capture
- [ ] Manual on macOS: \`./target/release/clipboard-history-mcp --help\` shows the new about string

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: branch pushed, PR URL printed.

---

## Done-criteria for T1

The track ships when **all** are true:

1. PR merged into `main`.
2. Zero `.expect()` / `.unwrap()` reachable from `run_daemon`, `run_server`, or any MCP tool implementation outside of `#[cfg(test)]` modules. Verified with the Step 4 grep in Task 11.
3. `cargo test` and `cargo clippy --all-targets -- -D warnings` are green on the merge commit.
4. On macOS, copying a password from 1Password produces a `skip: transient pasteboard type` log line — manually verified by the reviewer or the author.
5. CHANGELOG `[Unreleased]` covers all four bullet points (Fixed: is_transient, panic-free crypto, defensive refactors, biometry warn; Changed: about + dep; Removed: config_dir).
6. CLI `--help` output reads `Cross-platform clipboard history MCP`.
