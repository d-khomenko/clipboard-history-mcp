# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0-alpha.0] — 2026-05-06

### Added
- **Rust rewrite** — entire codebase ported from Node.js + Swift helper to a single self-contained Rust binary (`cargo build --release`)
- **MCPB packaging** — `mcpb/manifest.json` (manifest_version 0.2, binary server type) and `scripts/pack-mcpb.sh` produce a `clipboard-history-mcp.mcpb` (~6.2 MB) for one-click install in Claude Desktop
- **Universal macOS binary** — `lipo`-joined aarch64 + x86_64 release artifacts for full Intel/Apple Silicon support
- **Touch ID gate** — `unlock_secret` calls `objc2-local-authentication` `LAContext.evaluatePolicy` with `kLAContextInteractionNotAllowed` ACL; biometric required, no fallback to password via MCP
- **SQLite + FTS5 storage** — WAL mode, bundled FTS5 for full-text search, automatic migrations
- **15 MCP tools** via `rmcp` 1.6: `list_history`, `get_item`, `search_history`, `get_urls`, `get_code`, `get_json`, `get_secrets_index`, `unlock_secret`, `copy_item`, `pin_item`, `tag_item`, `delete_item`, `clear_history`, `get_stats`, `daemon_status`
- **CLI subcommands**: `daemon`, `serve`, `install`, `uninstall`, `status`, `vault`, `doctor`, `migrate-v2`
- **launchd integration** — `install` writes and loads plist; daemon auto-starts on login
- **AES-256-GCM encryption** with macOS Keychain–backed master key + biometric ACL
- **Type classifier** — classifies clips at capture: `url`, `json`, `sql`, `shell`, `code:<lang>`, `text`
- **Secret detector** — 252+ gitleaks patterns + JWT structure detection + Luhn-validated credit cards
- **Window title capture** — opt-in via `CLIPBOARD_CAPTURE_WINDOW_TITLE=1` (requires Accessibility permission)
- **GitHub Actions**: `ci.yml` (cargo check/test/fmt/clippy on macos-13 + macos-14), `release.yml` (builds universal binary, packs `.mcpb`, creates pre-release on `v*` tag)
- **Dependabot**: switched from npm to cargo ecosystem monitoring

### Changed
- Minimum macOS: **13.0** (up from 12.0)
- Distribution: `.mcpb` file replaces `npm install` + `bash scripts/build-native.sh` workflow
- MCP transport: pure `stdio` via `rmcp` (replaces `@modelcontextprotocol/sdk` Node.js transport)

### Removed
- Node.js / npm runtime dependency
- TypeScript source (`src/**/*.ts`, `tsconfig.json`)
- v2 Swift helper binary (`native/`)
- Vitest test suite (replaced by `cargo test`)
- `scripts/build-native.sh` (replaced by `scripts/pack-mcpb.sh`)

### Notes
- **x86_64 support**: Both ARM64 and x86_64 targets compile successfully. The universal binary is included in `.mcpb`. No separate x86_64-only package is needed.
- **Signing**: The `.mcpb` is unsigned in this alpha. Notarization / signing is planned for v0.3.1.

## [0.2.0] — 2026-05-05

### Added
- launchd-managed background daemon
- SQLite + FTS5 storage
- Type-aware classification (URL, JSON, SQL, shell, code:lang via flourite)
- Secret detection via vendored gitleaks rules + JWT + Luhn-validated credit cards
- AES-256-GCM secret encryption with macOS Keychain–backed master key
- Window-title capture (opt-in, Accessibility permission)
- 15-tool MCP surface (read / write / system)
- CLI: `install | uninstall | status | start | stop | vault | doctor | migrate-v1`

### Removed
- v1 single-process JSON-store implementation

## [0.1.0] — 2026-05-05

### Added
- Initial single-process MCP server polling pbpaste
