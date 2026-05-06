# clipboard-history-mcp v4 — Linux port design spec

**Status:** Draft
**Date:** 2026-05-06
**Author:** Dmytro Khomenko + Claude
**Topic:** Cross-platform port — macOS-first becomes macOS+Linux. Strategic pivot: skip GUI, ship multi-platform instead.

---

## 1. Why now (strategic framing)

After shipping v0.3.0-alpha.0 (Rust, MCPB, Touch ID), two product directions opened up:

- **Add a popover sidebar GUI** to compete with Maccy on UX. Cost: ~1-2 days. Ceiling: macOS-only forever.
- **Port to Linux + Windows.** Cost: ~3-5 days. Ceiling: 5-10× audience.

User chose port. The reasoning: our differentiator vs `vlad-ds/maccy-clipboard-mcp` is already strong on the data layer (type-classified clips, gitleaks secret detection, encrypted vault, audit-trailed unlock). Adding GUI duplicates Maccy. Adding Linux makes us **the only clipboard-history MCP that works outside Apple's walled garden** — a positioning Maccy can never match.

Slogan becomes: *Clipboard intelligence for Claude on every desktop.*

This spec covers Linux port (v0.4). Windows is deferred to v0.5; design notes scattered to surface the eventual plan.

---

## 2. Reference

[v3 design spec](2026-05-05-clipboard-history-mcp-v3-rust-design.md) is canonical for: data model, MCP tool surface, secret detection rules, threat model, classification logic, FTS5 schema. Nothing about those changes in v4.

This document captures **deltas vs v3**.

---

## 3. Architecture deltas

### 3.1 Compile-time platform split (no runtime polymorphism)

OS-specific code lives behind `#[cfg(target_os = "macos")]` / `#[cfg(target_os = "linux")]` attributes within the same modules. Rust idiomatic, zero runtime cost, smaller binaries.

Affected modules:

```
src/core/pasteboard.rs   — clipboard read/write/transient probe
src/core/crypto.rs       — master key storage
src/core/biometry.rs     — Touch ID (macOS) / passcode prompt (Linux)
src/daemon/context.rs    — frontmost app + window title
src/cli/install.rs       — launchd plist (macOS) / systemd unit (Linux)
src/cli/uninstall.rs     — symmetric to install
src/cli/doctor.rs        — platform-specific health checks
```

Trait-based abstraction is **explicitly rejected** for the v4 port. Reasoning: traits add an indirection layer that benefits nobody — there is one impl per platform at compile time. The build matrix expresses platform variance; the source code expresses logic.

### 3.2 Crate replacements

| Concern | macOS v3 | v4 (cross-platform) | Notes |
|---|---|---|---|
| Pasteboard read/write | `objc2-app-kit::NSPasteboard` | `arboard` 3.x | Wayland+X11 autodetect on Linux; native on macOS. Sync API, no async runtime needed. |
| Master key storage | `security-framework` 3.x | `keyring` 3.x | macOS Keychain, Linux Secret Service (libsecret/GNOME Keyring/KWallet), Windows Credential Manager — single API. |
| Frontmost app | `objc2-app-kit::NSWorkspace` | `active-win-pos-rs` 0.x | macOS + Linux X11 + Windows. Wayland is a stub for now (returns None). |
| Data dirs | hard-coded `~/Library/...` | `directories-next` 2.x | macOS: `~/Library/Application Support/...`. Linux: XDG (`~/.local/share/...` + `~/.config/...`). Windows: `%APPDATA%\...`. |
| Biometry | `objc2-local-authentication` | macOS-only feature-flagged; Linux uses keyring prompt | See §5. |

### 3.3 Replace `security-framework` with `keyring` on macOS too

Even on macOS we switch to `keyring` for the master key. Reasons:
- One code path instead of two (`security-framework` for macOS + `keyring` for Linux)
- `keyring` on macOS uses the same Keychain Services C ABI under the hood — no behavioural regression
- Existing v0.3 master keys (stored under service `clipboard-history-mcp` / account `master-key-v1`) remain readable because `keyring` uses identical Keychain item naming

**Migration:** transparent. v0.4 reads the v0.3 entry on first run.

### 3.4 Pasteboard library choice

`arboard` 3.x replaces both `objc2-app-kit::NSPasteboard` (macOS) and the Swift helper for transient-type probing. Trade-offs:

- ✅ Single crate covers all OSes
- ✅ Sync API (no tokio overhead in watcher hot path)
- ✅ Shrinks the daemon's macOS-specific code by ~150 lines
- ⚠️ `arboard` does NOT expose `NSPasteboard.types` enumeration. We **lose transient/concealed-type detection** as a side effect.

Compensation for #3: on every platform, the canonical bypass becomes **`CLIPBOARD_IGNORE_APPS`** populated by default with the major password managers:

```
1Password 7,1Password,Bitwarden,KeePassXC,Keychain Access,Dashlane
```

The default list ships in `cli::install` and is documented in README. Users add their own as needed. This regresses macOS UX slightly (1Password Concealed-Type clips were ignored automatically; now require the app being on the ignore list — which `install` populates by default, so functionally identical for the common case).

If transient-type support becomes a high-priority macOS feature in v0.4.x, we can re-add a Swift helper or use `objc2-app-kit` only for the type probe while keeping `arboard` for read/write. Defer the decision.

---

## 4. Linux clipboard layer

### 4.1 X11 vs Wayland

`arboard` autodetects via `WAYLAND_DISPLAY` env var. Both backends expose the same Rust API.

**Wayland edge case:** when the source application that put data on the clipboard quits, the data is gone (Wayland clipboard is owner-bound, not a stash). `arboard` handles this with a "persistent paste" daemon thread it spawns internally. Our daemon polls every 1.5s; we read clipboard ourselves before any source app could quit, so this is mostly a non-issue.

### 4.2 Window title (opt-in)

| OS | Source | Status |
|---|---|---|
| macOS | `accessibility` crate (AXUIElement) | unchanged from v3 |
| Linux X11 | `x11rb` crate, query `_NET_WM_NAME` of `_NET_ACTIVE_WINDOW` | implemented in v0.4.0 |
| Linux Wayland | `org.freedesktop.portal.WindowTitle` portal API | **deferred to v0.4.x** — portal support is patchy across compositors |
| Windows | `GetWindowTextW` | v0.5 |

`CLIPBOARD_CAPTURE_WINDOW_TITLE=1` semantics unchanged from v3 (default off, requires permission grants on each OS).

### 4.3 Frontmost app

`active-win-pos-rs` 0.x returns `{ pid, app_name, window_title }`. We use `app_name` as our `source_app` field (the bundle ID equivalent on Linux is the X11 `WM_CLASS` instance string).

---

## 5. Biometry strategy

**Touch ID stays macOS-only.** No PAM/fprintd flows on Linux — too distro-fragile, low coverage, terrible UX.

Universal mechanism: **master password fallback**.

### 5.1 Design

On first run, the user is prompted to set a master password (CLI prompt, hidden input). The password is run through Argon2id (memory: 19 MiB, iterations: 2, parallelism: 1) to derive a 32-byte key-encryption-key (KEK). The KEK encrypts the AES-256-GCM master key with `aes-gcm`. The encrypted master key is stored in `keyring` under service `clipboard-history-mcp`, account `master-key-v2`.

**Per-process unlock cache** (existing 5-min TTL pattern from v3 biometry stays). On each `unlock_secret`, the password is required unless within the cache window.

### 5.2 macOS UX preservation

On macOS, the keyring entry is created with the biometric ACL (`kSecAccessControlBiometryCurrentSet`). The OS-level Touch ID prompt fires when the entry is read — the master password becomes effectively a fallback for users whose Mac doesn't have Touch ID.

For Macs WITH Touch ID: behavior unchanged from v0.3 (`unlock_secret` triggers the OS Touch ID prompt). Master password is set during install and stored encrypted alongside; if biometry fails, password input is the fallback.

### 5.3 Linux UX

`unlock_secret` triggers a CLI prompt (or `pinentry` if available — to support GUI clients integrating). Cached for 5 min per process. Documentation calls out the cache TTL clearly.

### 5.4 Migration from v0.3

Existing macOS users have a 32-byte raw master key in Keychain under `master-key-v1`. v0.4 first-run:
1. Detect `master-key-v1` exists
2. Prompt user to set a new master password (Argon2id-wrapped from now on)
3. Re-encrypt: read raw key from `master-key-v1`, encrypt with KEK from new password, write to `master-key-v2`
4. Delete `master-key-v1`
5. Existing `secrets.ciphertext` blobs continue to decrypt with the same raw key — only the storage shape of that key changes

Users who skip the prompt keep `master-key-v1` and v0.4 falls back to the raw-key path (compatibility mode, with a deprecation log message).

---

## 6. Daemon lifecycle

### 6.1 Linux — systemd user service

`clipboard-history-mcp install` writes `~/.config/systemd/user/clipboard-history-mcp.service`:

```ini
[Unit]
Description=clipboard-history-mcp watcher daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=/path/to/clipboard-history-mcp daemon
Restart=on-failure
RestartSec=5
Environment="CLIPBOARD_HISTORY_MAX=1000"
Environment="CLIPBOARD_CAPTURE_WINDOW_TITLE=0"

[Install]
WantedBy=default.target
```

Then runs:

```bash
systemctl --user daemon-reload
systemctl --user enable --now clipboard-history-mcp.service
loginctl enable-linger "$USER"   # optional, lets daemon survive logout
```

`loginctl enable-linger` is opt-in via `--linger` flag on `install` because it requires polkit consent (one-time prompt).

### 6.2 macOS — launchd plist

Unchanged from v0.3.

### 6.3 Cross-platform `install` entry point

```rust
pub fn install(opts: InstallOpts) -> Result<()> {
    #[cfg(target_os = "macos")]
    return install_launchd(opts);
    #[cfg(target_os = "linux")]
    return install_systemd(opts);
}
```

---

## 7. File paths

`directories-next::ProjectDirs::from("kz", "me", "clipboard-history-mcp")` resolves to:

| OS | Data | Config | Logs |
|---|---|---|---|
| macOS | `~/Library/Application Support/me.kz.clipboard-history-mcp/` | (same) | (same) |
| Linux | `~/.local/share/me.kz.clipboard-history-mcp/` | `~/.config/me.kz.clipboard-history-mcp/` | `$XDG_STATE_HOME/.../logs/` or fallback to data dir |
| Windows | `%APPDATA%\me.kz\clipboard-history-mcp\` | (same) | (same) |

**macOS migration:** the v0.3 path was `~/Library/Application Support/clipboard-history-mcp/` (no reverse-domain qualifier). v0.4 detects the v0.3 path on first run, moves contents to the new path, leaves a `.migrated-to-v4` breadcrumb.

---

## 8. CI matrix

```yaml
# .github/workflows/ci.yml
strategy:
  matrix:
    os: [macos-13, macos-14, ubuntu-22.04, ubuntu-24.04]
runs-on: ${{ matrix.os }}
steps:
  - uses: actions/checkout@v4
  - uses: dtolnay/rust-toolchain@stable
  - if: contains(matrix.os, 'ubuntu')
    run: sudo apt-get install -y libdbus-1-dev libxcb1-dev pkg-config xvfb
  - run: cargo fmt -- --check
  - run: cargo clippy --all-targets -- -D warnings
  - run: cargo test --release
```

Linux runners need `libdbus-1-dev` (for keyring's Secret Service) and `libxcb1-dev` (for `arboard` X11). `xvfb` runs as a virtual display so clipboard tests can execute headless.

Windows is **excluded** from v0.4 CI matrix — added when v0.5 lands.

---

## 9. Tests

Existing tests stay. Linux-specific tests added:

- `tests/clipboard_linux.rs` — `#[cfg(target_os = "linux")]` round-trip via xvfb
- `tests/keyring.rs` — round-trip on whatever platform CI is currently on (`#[ignore]`'d locally because it touches the real keyring)
- Master-password Argon2id round-trip in `tests/crypto.rs` (already platform-agnostic)

Existing macOS-specific tests gain `#[cfg(target_os = "macos")]` guards where they touch NSPasteboard or LAContext.

---

## 10. Phase scope

### v0.4.0 (this spec)

- Linux clipboard read/write via `arboard`
- Linux frontmost-app via `active-win-pos-rs`
- Linux X11 window-title via `x11rb` (Wayland deferred)
- `keyring` replaces `security-framework` on macOS too
- Master-password biometry on Linux; Touch ID stays primary on macOS
- systemd user service installer
- XDG-compliant data/config paths
- macOS migration: re-encrypt master key with new password-wrapped scheme
- CI: macos-13/14 + ubuntu-22.04/24.04
- All 15 MCP tools work identically on both platforms
- README updated with Linux install instructions
- MCPB still macOS-only (the format itself is Mac-targeted; Linux users use direct binary)

### v0.4.x

- Wayland window-title via portal API
- `.deb` / `.rpm` / AppImage packaging
- Touch ID UX improvements (per-secret cache, optional always-prompt)

### v0.5.0

- Windows port (`active-win-pos-rs` already supports it; Credential Manager via `keyring`)
- MSIX or just raw `.exe` distribution
- CI matrix gains `windows-2022`

---

## 11. Risks and open questions

1. **`arboard` reliability on macOS.** Direct objc2 was robust; arboard wraps the same APIs but adds a Rust layer. Mitigation: keep an `arboard-bypass` feature flag for v0.4.x escape hatch if perf/correctness regress. Initial CI-time perf comparison is a v0.4.0 acceptance criterion.

2. **Wayland portal compatibility.** Several compositors (sway, river, Hyprland) ship partial portal implementations. Window-title support varies. Mitigation: feature works where it works, returns `None` otherwise — does not break the daemon.

3. **`keyring` Linux backend selection.** Some distros default to GNOME Keyring; others to KWallet; on minimal setups (e.g. headless Linux dev container) neither runs. `keyring` 3.x has a `linux-secret-service` feature and a `linux-no-secret-service` mock — pick which features to enable carefully. Recommended: ship with `linux-secret-service` only; document that headless setups need `gnome-keyring-daemon --start` or KWallet equivalent.

4. **Argon2id parameters.** 19 MiB / 2 iters / 1 parallel = OWASP 2023 recommended minimums. Slow enough to defeat brute-force, fast enough that user doesn't notice (~100ms on modern hardware). Re-evaluate in v0.5.

5. **macOS migration consent.** First-run prompt asks for a password the user didn't set before. Some users will close it, leaving the system in compat-mode. Risk: split brain — secrets encrypted under new scheme can't be decrypted by v0.3 binary. Mitigation: write big migration warning in CHANGELOG; provide `clipboard-history-mcp downgrade-keys` to revert.

6. **`active-win-pos-rs` permission requirements on Wayland.** Some Wayland compositors require KDE-Connect-style explicit permission for window listing. Document the limitation; fall back to `None`.

---

## 12. Out of scope for v4

- Image / file clip capture
- iCloud / Git / cloud sync
- GUI (popover or otherwise)
- Semantic embedding search
- Windows port (v0.5)
- MCPB packaging for Linux (.mcpb is currently a macOS-targeted format from Anthropic; if extended to Linux we revisit)

---

## 13. Open-source-or-not checklist (carryover, refreshed)

- [ ] No personal data in test fixtures (existing v0.3 fixtures are clean)
- [ ] No hardcoded `/Users/stock/...` or `/home/stock/...` paths in code or specs
- [ ] No real API keys or PII anywhere
- [ ] `Cargo.lock` committed (binary project)
- [ ] LICENSE / CHANGELOG / CONTRIBUTING updated
- [ ] README install matrix: macOS .mcpb + macOS source + Linux source + Linux distro packages (when available)
- [ ] CI workflows tested on fresh runner per OS
