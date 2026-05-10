# Security policy

clipboard-history-mcp captures and stores potentially sensitive clipboard data on the user's machine. This document describes the threat model, supported versions, and how to report vulnerabilities.

## Supported versions

Only the latest tagged release on `main` receives security fixes. The project is in `0.x` alpha — we do not maintain LTS branches.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security reports. Instead:

1. Open a private security advisory at <https://github.com/d-khomenko/clipboard-history-mcp/security/advisories/new>, OR
2. Email the maintainer at `khomenkoknit@gmail.com` with the subject `[clipboard-history-mcp security]`.

You should receive an acknowledgement within a week. Fixes for verified vulnerabilities target the next tagged release.

## Threat model summary

- **Trust boundary:** the user's local filesystem and macOS Keychain. Anything readable by the same user account is in scope for the daemon.
- **Encryption at rest:** secrets are encrypted with AES-256-GCM under a master key in the platform keyring (`master-key-v1` raw, or `master-key-v2` Argon2id-wrapped after `migrate-v2`). Non-secret text, images, and files are stored plaintext in `data_dir/`.
- **Touch ID / biometric gate:** the `unlock_secret` MCP tool prompts for biometric auth on macOS (`LAContext.evaluatePolicy`). Linux uses an interactive password prompt.
- **Pasteboard concealed-type honour:** macOS pasteboards marked `org.nspasteboard.ConcealedType` (1Password, Bitwarden, KeePassXC) are skipped at capture. The `CLIPBOARD_IGNORE_APPS` env var is the secondary defence.
- **Vault-mirror exclusion:** clips classified as secrets are never written to the Obsidian vault even when `CLIPBOARD_VAULT_PATH` is set.

## Known limitations

- **`master-key-v1` raw mode** stores the master key as raw bytes in the keyring. Anyone with read access to the user's keyring can decrypt secrets. Run `clipboard-history-mcp migrate-v2` to upgrade to a password-wrapped master key.
- **Linux Wayland**: window-title capture works only on X11; Wayland support is deferred.
- **Windows**: not yet supported.
- **No remote-attack surface**: the daemon does not bind a network port. The only inputs are the local pasteboard and the MCP stdio transport from Claude.

For the operational footprint and design rationale, see `README.md` (Performance and Security Model sections) and `docs/superpowers/specs/`.
