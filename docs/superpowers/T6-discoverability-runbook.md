# T6 discoverability runbook

Post-merge of v0.6, complete these manual outreach steps to make the project discoverable.

## Done autonomously

- [x] **awesome-mcp-servers PR**: [punkpeye/awesome-mcp-servers#6166](https://github.com/punkpeye/awesome-mcp-servers/pull/6166) — adds `d-khomenko/clipboard-history-mcp` under the **OS Automation** category. Waiting on upstream merge; no further action needed unless the maintainer requests changes.

## Manual (maintainer action required)

### Glama submission

[Glama](https://glama.ai/mcp/servers) is a curated MCP directory that auto-syncs into the awesome-mcp-servers README (each indexed entry gets the score badge you see next to other entries). Submission is form-based, so it can't be automated.

1. Visit <https://glama.ai/mcp/servers/submit>.
2. Fill the form:
   - **Repository URL:** `https://github.com/d-khomenko/clipboard-history-mcp`
   - **Category:** OS Automation (matches the awesome-mcp-servers placement)
   - **Description:** copy from `Cargo.toml` `description` field — `Type-aware, secret-safe clipboard history exposed to Claude via MCP. macOS + Linux.`
   - **Tags:** `clipboard`, `history`, `secrets`, `macos`, `linux`, `rust`, `obsidian`, `touch-id`
3. Submit. Glama's review queue typically responds within ~1 week.
4. After Glama indexes the project, open a follow-up PR to `punkpeye/awesome-mcp-servers` adding the score badge inline with the entry — copy the badge syntax from any existing entry that already has one (e.g. `sbuysse/gnome-desktop-mcp` in the same OS Automation block).

### MCP Registry verification

After the next `v*` tag fires the new `mcp-publish` job (T5), confirm the registry picked it up:

```bash
curl -s 'https://registry.modelcontextprotocol.io/v0.1/servers?search=clipboard-history-mcp' | jq
```

Expect a single entry with the namespace `io.github.d-khomenko/clipboard-history-mcp` and `version` matching the tag. If the search returns empty after ~10 min, check the GitHub Actions run for `mcp-publish` and re-trigger if it failed.

### PulseMCP

[PulseMCP](https://www.pulsemcp.com/) indexes the official MCP Registry. Once the registry entry exists (above), the PulseMCP listing should appear automatically within ~24 h — no submission step required. Spot-check after a day:

```bash
open 'https://www.pulsemcp.com/servers?q=clipboard-history-mcp'
```

If it's still missing after 48 h, file an issue against `pulsemcp/website` referencing the registry namespace.

### Optional: Show HN / r/ClaudeAI / r/rust

Timing recommendation: post **1–2 weeks after the v0.6 GitHub Release lands** and at least one external user has tried `cargo install clipboard-history-mcp`. Otherwise the front-page traffic shows up to a half-finished install path and burns the launch slot.

Suggested titles:

- **Show HN:** `Show HN: A clipboard history MCP for Claude that handles secrets and Obsidian sync`
- **r/ClaudeAI:** `Built a clipboard-history MCP server for Claude Desktop — secrets-aware, Touch ID gated, mirrors clips into Obsidian`
- **r/rust:** `clipboard-history-mcp — type-aware clipboard history MCP server in Rust (~24 MB RSS, ~1 mW idle)`

Body should mention: Rust binary, macOS + Linux, Touch ID gate, content-hash dedup'd image / file capture, ~1 mW idle on macOS. Link the README, not the crates.io page (the README has the demo GIFs and config snippets). Keep it under 300 words; HN front page is allergic to long pitches.

Comment proactively in the first hour with config snippets — that's where the conversion to actual installs happens.
