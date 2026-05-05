# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
