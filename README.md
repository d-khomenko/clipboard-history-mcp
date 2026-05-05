# clipboard-history-mcp

Local MCP server (stdio) that records the macOS pasteboard and exposes recent
entries to Claude. Polls `pbpaste` while running and stores history in
`~/.clipboard-history-mcp/history.json`.

> **macOS only** — uses `pbpaste` / `pbcopy`.

## Tools

| Tool | What it does |
|---|---|
| `list_history` | Recent entries (id, preview, length, createdAt). Newest first. |
| `get_item` | Full text of one entry by id. |
| `search_history` | Case-insensitive substring search. |
| `copy_item` | Restore an entry to the system clipboard (so you can paste it). |
| `get_current_clipboard` | Read what's on the clipboard right now. |
| `clear_history` | Wipe stored history. System clipboard untouched. |
| `get_stats` | Counts, file path, timestamps. |

## Install

```bash
cd /Users/stock/Documents/wezom/clipboard-history-mcp
npm install
```

## Add to Claude Code

```bash
claude mcp add clipboard-history -- node /Users/stock/Documents/wezom/clipboard-history-mcp/src/index.js
```

Or edit `~/.claude.json` manually under `"mcpServers"`:

```json
"clipboard-history": {
  "type": "stdio",
  "command": "node",
  "args": ["/Users/stock/Documents/wezom/clipboard-history-mcp/src/index.js"]
}
```

Restart Claude Code. Test with: *"List my last 5 clipboard entries."*

## Config (env vars)

| Var | Default | Notes |
|---|---|---|
| `CLIPBOARD_POLL_MS` | `1500` | Polling interval. |
| `CLIPBOARD_HISTORY_MAX` | `500` | Ring-buffer size. |
| `CLIPBOARD_MAX_TEXT_BYTES` | `1000000` | Skip clipboard items larger than this (avoid huge pastes). |

## Notes

- History is captured **only while the MCP server is running** (i.e. Claude
  Code has connected to it). Closing Claude Code stops the watcher.
- Consecutive identical clipboard contents are deduped.
- Storage is plain JSON — readable, deletable, syncable as you like.
