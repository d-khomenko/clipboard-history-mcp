# clipboard-history-mcp — Obsidian vault mirror design spec

**Status:** Draft
**Date:** 2026-05-06
**Author:** Dmytro Khomenko + Claude
**Topic:** Daemon-side auto-mirror of captured clips to an Obsidian vault — sidecar `.md` per clip + append to daily note.

---

## 1. Why now

Two of the user's daily-driver tools are clipboard-history-mcp and Obsidian. They currently live behind a wall: every classified clip is reachable from Claude (good), but invisible to the user's own knowledge graph (bad). The user already maintains a vault at `~/dev/ohnoma/`; copying a useful URL or code snippet should leave a trace there without manual export.

Auto-mirroring closes that loop. Once the daemon is configured with a vault path:

- Every captured clip becomes a navigable note in the vault.
- The day's clipboard activity rolls up into the daily note as a timeline.
- Existing Obsidian features (backlinks, search, dataview) work for free against captured content.
- Claude gains nothing new — the vault mirror is purely for the human user. Claude continues reading from SQLite via existing MCP tools.

Strategic positioning: this turns clipboard-history-mcp from a "clipboard manager Claude can read" into a "frictionless clip-to-PKM bridge." That is a story Maccy and `vlad-ds/maccy-clipboard-mcp` cannot tell — they don't classify clips, so even if they could write files they couldn't write *useful* ones.

---

## 2. Reference

- [v3 Rust design spec](2026-05-05-clipboard-history-mcp-v3-rust-design.md) — canonical for capture flow, classification, secret detection, data model.
- [v4 Linux port spec](2026-05-06-clipboard-history-mcp-v4-linux-design.md) — canonical for `WatchOpts`, install flow, env-var → daemon plumbing.

This document captures **deltas vs v4** required to add vault mirroring. No data-model changes, no MCP tool changes, no schema migration.

---

## 3. UX model

### 3.1 At install time

User passes `--vault PATH` (optional) to `clipboard-history-mcp install`. Install writes the path into the platform's service environment so the daemon picks it up on every launch:

- **macOS** — appended to launchd plist `EnvironmentVariables` dict.
- **Linux** — appended to systemd unit `Environment=` line.

If `--vault` is omitted, daemon runs with no mirror — feature is fully opt-in. Existing installs keep working unchanged.

```bash
# macOS — auto-mirror enabled
clipboard-history-mcp install --vault ~/Documents/Obsidian/MyVault

# macOS — original behaviour, no vault
clipboard-history-mcp install
```

The path can be relative or absolute; relative is resolved against `$HOME` at write time. The daemon **does not require the path to exist at install time** — folders are created lazily on first capture.

### 3.2 At capture time

For every successful `add_clip` insert (i.e. **non-secret** clips only), the daemon performs two filesystem writes inside the vault:

1. **Sidecar** at `<vault>/clipboard/<YYYY-MM>/<id>-<kind>-<slug>.md`
2. **Daily-note append** to `<vault>/daily/<YYYY-MM-DD>.md` adding a single bullet that wikilinks to the sidecar.

Secrets (matched by `secrets::detect_secret`) are **never** mirrored. They take the existing `add_secret` branch and bypass vault writes entirely. This is enforced by code structure (the vault-write call is inside the `else` branch where `add_clip` is called) — there is no `if !is_secret` guard to forget.

### 3.3 At runtime

A typical Obsidian session 30 minutes after install:

```
~/Documents/Obsidian/MyVault/
├── clipboard/
│   ├── 2026-05/
│   │   ├── 142-url-platform-openai-com.md
│   │   ├── 143-json-stripe-config.md
│   │   ├── 144-code_python-authenticate.md
│   │   └── 145-sql-select-from-users.md
└── daily/
    └── 2026-05-06.md       ← appended-to throughout the day
```

The user's existing daily-note workflow continues unaffected — the vault writer adds a single H2-bounded section and never touches anything outside it.

---

## 4. File formats

### 4.1 Sidecar — one MD file per non-secret clip

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

https://platform.openai.com/api-keys
```

- Frontmatter is YAML 1.2 single-doc.
- `id` matches `clips.id` in SQLite — stable identifier for cross-references.
- `captured` is RFC 3339 with **local** timezone offset (not UTC) — Obsidian dataview queries default to local.
- Body contains the literal clip text, unescaped, no markdown wrapping. If the clip is itself a code block (kind starts with `code:`), it's wrapped in fenced code with the language tag (`code:python` → ` ```python `).

### 4.2 Daily note section

The writer maintains exactly one H2-bounded section per daily file:

```md
## Clipboard captures · 2026-05-06

- 04:32:03 [[142-url-platform-openai-com|url from Safari]]
- 04:32:05 [[143-json-stripe-config|json from Cursor]]
- 04:32:07 [[144-code_python-authenticate|code:python from Cursor]]
```

Append rules:

- If file does not exist → create with the H2 header + first bullet.
- If file exists but no `## Clipboard captures` header → append a fresh section at the end.
- If section already present → append the new bullet at the end of that section, preserving the user's manual additions before/after.
- Detection of "the section" uses literal string matching on the H2 header text — robust to minor surrounding edits but breaks if the user renames the header. Acceptable: section is owned by the daemon, header is the contract.

### 4.3 Filename slug rules

```
<id>-<kind-with-dashes>-<preview-slug>.md
```

- `<id>` — SQLite autoincrement, guaranteed unique.
- `<kind-with-dashes>` — `:` replaced with `_` (so `code:python` → `code_python`). Bare alphanumerics + underscores.
- `<preview-slug>` — first 60 chars of clip text, lowercased, non-`[a-z0-9]+` collapsed to single `-`, leading/trailing `-` stripped. Empty preview → omitted (filename ends at kind).

Examples:

| input | filename |
|---|---|
| URL `https://platform.openai.com/api-keys` | `142-url-platform-openai-com-api-keys.md` |
| Python `def authenticate(token: str):` | `144-code_python-def-authenticate-token-str.md` |
| Empty/whitespace clip (impossible — daemon skips) | n/a |

---

## 5. Architecture

### 5.1 New module — `src/core/vault.rs`

Single new file (~150 lines). Public API:

```rust
pub struct VaultMirror {
    root: PathBuf,
}

impl VaultMirror {
    pub fn new(root: PathBuf) -> Self;

    /// Write sidecar + append to daily note. Errors are returned but
    /// callers are expected to log+continue, not crash.
    pub fn write(&self, item: &MirrorItem) -> Result<()>;
}

pub struct MirrorItem<'a> {
    pub id: i64,
    pub primary_kind: &'a str,
    pub source_app: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub text: &'a str,
    pub captured: chrono::DateTime<chrono::Local>,
}
```

Internals (private fns):

- `format_frontmatter(item) -> String`
- `format_body(item) -> String` — code blocks fenced when kind starts with `code:`
- `slug_for_filename(item) -> String`
- `sidecar_path(item) -> PathBuf` — `root/clipboard/YYYY-MM/<filename>`
- `daily_path(item) -> PathBuf` — `root/daily/YYYY-MM-DD.md`
- `atomic_write(path, contents) -> Result<()>` — write to `path.<pid>.tmp`, fsync, rename
- `append_daily(path, line) -> Result<()>` — read-modify-write with retry (handles Obsidian holding the file briefly)

### 5.2 `src/daemon/watcher.rs` — single hook

```rust
// inside the existing else branch where add_clip is called
let id = store.add_clip(ClipInput { ... })?;
info!("clip captured: {} from {:?}", cls.primary_kind, ctx.front_app);

if let Some(mirror) = &opts.vault_mirror {
    let item = MirrorItem {
        id,
        primary_kind: &cls.primary_kind,
        source_app: ctx.front_app.as_deref(),
        window_title: ctx.window_title.as_deref(),
        text: &text,
        captured: chrono::Local::now(),
    };
    if let Err(e) = mirror.write(&item) {
        warn!("vault mirror failed: {}", e);
    }
}
```

`store.add_clip` already returns the assigned `id: i64` — that's the only change to its return type story (it was already returning the id, just unused at this call-site).

The vault write is **never** in a hot path that blocks capture. A failed mirror is a `warn!` line in `daemon.log` and nothing else — capture continues as before.

### 5.3 `src/daemon/mod.rs` — `WatchOpts`

```rust
pub struct WatchOpts {
    // ... existing fields ...
    pub vault_mirror: Option<VaultMirror>,
}
```

Construction reads `CLIPBOARD_VAULT_PATH` env var at daemon startup; if present, `Some(VaultMirror::new(path))`, else `None`.

### 5.4 `src/cli/install_macos.rs` + `install_linux.rs`

Both gain a `vault: Option<PathBuf>` field on their respective `InstallOpts` struct, and an additional env-block injection step:

```rust
// macOS — extend the existing __ENV_DICT__ template substitution:
let mut env_dict = format!(
    "    <key>CLIPBOARD_CAPTURE_WINDOW_TITLE</key>\n    <string>{}</string>",
    if opts.window_titles { "1" } else { "0" }
);
if let Some(p) = &opts.vault {
    env_dict.push_str(&format!(
        "\n    <key>CLIPBOARD_VAULT_PATH</key>\n    <string>{}</string>",
        p.display()
    ));
}
```

Linux symmetric (one extra `Environment=CLIPBOARD_VAULT_PATH=...` line in the systemd unit).

### 5.5 CLI plumbing — `src/cli/install.rs`

One additional `clap` arg:

```rust
#[arg(long, value_name = "PATH",
      help = "Auto-mirror non-secret clips to this Obsidian vault")]
vault: Option<PathBuf>,
```

Forwarded into the existing `install_macos`/`install_linux` opts struct.

---

## 6. Error handling

The daemon's reliability bar is *capture must never fail because of mirror writes*. Concrete cases and behaviour:

| Failure | Behaviour |
|---|---|
| Vault path doesn't exist | `mkdir -p` lazily on first write. Fail only if mkdir itself errors. |
| Permission denied on vault dir | `warn!("vault mirror failed: {}", e)`, capture continues. Do not retry — root cause is config, not transient. |
| Disk full (ENOSPC) | `warn!`, capture continues. SQLite write already succeeded; user can manually export later. |
| Daily note locked by Obsidian (rare; Obsidian holds a brief write lock during sync) | Retry once after 100ms. If still locked, `warn!` and skip the daily-note append for this item. The sidecar still gets written. |
| iCloud / Obsidian Sync race on sidecar | Atomic write (write-then-rename) prevents partial reads. Filename is unique by `id`, so no two daemons can race on the same file. |
| Filename collision | Impossible by construction — `id` is monotonic SQLite autoincrement. |
| Slug produces empty string (clip is all whitespace) | Daemon already skips whitespace-only clips upstream. Defensive: if it reaches the writer, filename omits the slug suffix. |

All `warn!` lines include the relevant `id` so users can grep `daemon.log` for "vault mirror failed: 142" and see exactly what didn't sync.

---

## 7. Security & privacy

### 7.1 Secrets — never mirrored

The vault writer is invoked **only** in the `add_clip` branch of `watcher.rs`. The `add_secret` branch is structurally unable to call it — there is no shared code path. This makes the property an invariant of the file layout, not a runtime check.

A test in `tests/vault_integration.rs` asserts: copying a string that matches a gitleaks rule (e.g. an OpenAI key pattern) produces zero files in the vault. Regression-proof.

### 7.2 Window-title leakage

`window_title` is included verbatim in sidecar frontmatter when `CLIPBOARD_CAPTURE_WINDOW_TITLE=1`. Already an opt-in in v3; unchanged here. Users who don't want titles in their vault disable the env var globally.

### 7.3 Atomic writes

Sidecar writes go through `path.<pid>.tmp` → `fsync` → `rename` to prevent Obsidian or sync clients from observing half-written frontmatter. Daily-note appends use the same primitive: read full file → append in memory → write to `.tmp` → rename.

### 7.4 No new attack surface for Claude

Claude reads from SQLite via the existing MCP server. The vault mirror is a one-way fan-out from daemon → filesystem, with no Claude involvement. No new tool means no new prompt-injection vector.

---

## 8. Testing

### 8.1 Unit tests — `src/core/vault.rs`

- `slug_for_filename` — alphanumeric collapse, length cap, trim, special chars.
- `format_frontmatter` — required fields, escaping of quotes in window title, RFC-3339 timestamp.
- `format_body` — fenced code block when kind starts with `code:`, raw otherwise.

### 8.2 Integration test — `tests/vault_integration.rs`

Use `tempfile::TempDir` as fake vault root.

- **Sidecar shape:** capture a URL clip → assert `<root>/clipboard/2026-05/<id>-url-...md` exists with correct frontmatter and body.
- **Daily-note append:** capture two clips on same day → daily note has both bullets, in chronological order, under one section header.
- **Daily-note multi-day:** capture clips on different days (mock `chrono::Local::now()`) → two separate daily files.
- **Secret skip:** copy a fake OpenAI key → vault contains zero files (assertion: `read_dir(temp_root).count() == 0`).
- **Special chars in preview:** copy text containing `/`, spaces, unicode → filename slugified safely, file readable.
- **Vault path missing:** point to a path under a nonexistent parent → first capture creates parents recursively.

### 8.3 Manual smoke test

After install: copy a URL, copy a JSON snippet, copy an OpenAI-key-shaped string. Open the vault in Obsidian and verify:

- 2 sidecars under `clipboard/2026-05/` (no third).
- Daily note has 2 bullets, no third.
- Backlinks panel in Obsidian shows 2 incoming links to today's daily note.

---

## 9. Effort estimate

| Task | Effort |
|---|---|
| `src/core/vault.rs` — formatter + atomic writers | 1.5–2h |
| `WatchOpts` field + env-var read in daemon startup | 15min |
| Hook into `watcher.rs` after `add_clip` | 15min |
| `--vault` flag in CLI; plumb through install opts | 20min |
| `install_macos.rs` plist env injection | 15min |
| `install_linux.rs` systemd `Environment=` injection | 15min |
| Unit tests (slug, frontmatter, body) | 30min |
| Integration tests (tempdir vault) | 1h |
| Manual smoke test against real Obsidian vault | 30min |
| README + CHANGELOG | 15min |
| Edge-case polish (Obsidian sync retry, missing parents) | 30min |
| **Total** | **~5–6h** |

Single afternoon if uninterrupted. Two evenings if mixed with other work.

---

## 10. Out of scope

Explicitly NOT in this spec — track separately if needed:

- **Bidirectional sync** — editing a sidecar in Obsidian doesn't update SQLite. One-way fan-out only.
- **Retroactive backfill** — installing `--vault` does not export pre-existing clips. Only clips captured *after* install land in the vault. (Could add `clipboard-history-mcp vault backfill` CLI subcommand later.)
- **Custom templates** — sidecar/daily formats are fixed in this version. If users want a different layout they can post-process via Obsidian community plugins (e.g. Templater fires on file-create).
- **Tag/pin propagation** — Obsidian tags applied in the vault are not synced back to SQLite. Daemon does not re-read sidecars.
- **Vault path move detection** — if the user moves the vault folder after install, they re-run `install --vault NEW_PATH`. No automatic migration.
- **Per-kind vault subfolders** — all sidecars live under `clipboard/YYYY-MM/`, regardless of kind. Users who want tag-based folders use Obsidian filters/dataview.
- **MCP tool to write to vault** — Claude continues having no vault access. If wanted later, that's a separate `write_vault_note(content, path)` tool design — orthogonal to this spec.

---

## 11. Open questions

None blocking. Two minor decisions can be deferred to implementation:

1. **Local TZ vs config TZ** — `chrono::Local` reads system TZ at runtime. If the user's machine TZ disagrees with their preferred Obsidian daily-note TZ (e.g. travelling), the daily note rolls over at the "wrong" midnight. We accept this — overriding via `CLIPBOARD_VAULT_TZ` is YAGNI until requested.
2. **Daily-note section title localization** — currently hardcoded `"## Clipboard captures · 2026-05-06"` in English. Acceptable for v0.5; a `CLIPBOARD_VAULT_DAILY_HEADER` env var can be added later if a Ukrainian/Polish-speaking user complains.

---

## 12. Version target

Ship as part of **v0.5.0** alongside the v4-linux work. Adds:

- New env var: `CLIPBOARD_VAULT_PATH`
- New CLI flag: `--vault PATH` on `install`
- New module: `core::vault`

No breaking changes. Users who don't pass `--vault` see zero behavioural difference.
