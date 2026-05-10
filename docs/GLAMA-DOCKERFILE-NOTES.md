# Glama Dockerfile Notes

This Dockerfile exists solely to satisfy the [Glama.ai](https://glama.ai/mcp/servers) listing checker, which only verifies that the MCP server starts and responds to the `initialize` request. The clipboard-history feature requires native NSPasteboard (macOS) or wl-clipboard/X11 (Linux) access to the host pasteboard — a container has neither.

For real clipboard capture, install via:

- macOS / Linux: `cargo install clipboard-history-mcp`
- macOS one-click: download the `.mcpb` from a [GitHub Release](https://github.com/d-khomenko/clipboard-history-mcp/releases)

The container runs with `CLIPBOARD_EPHEMERAL_KEY=1` set, which skips keyring access and uses a `[0u8; 32]` ephemeral key. Safe because no real clips ever land in the container — only MCP introspection responses.
