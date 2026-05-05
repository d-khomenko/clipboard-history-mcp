#!/usr/bin/env node
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

import { HistoryStore } from "./store.js";
import { startWatcher } from "./watcher.js";
import { writeClipboard, readClipboard } from "./clipboard.js";

const store = new HistoryStore();
const stopWatcher = startWatcher(store, {
  onError: (err) => console.error("[clipboard-watcher]", err.message),
});

const tools = [
  {
    name: "list_history",
    description:
      "List the most recent clipboard entries (newest first). Returns id, preview, length, createdAt — full text only via get_item.",
    inputSchema: {
      type: "object",
      properties: {
        limit: { type: "number", description: "Max items to return (default 20, max 100).", minimum: 1, maximum: 100 },
        offset: { type: "number", description: "Skip this many newest entries (for paging).", minimum: 0 },
      },
    },
  },
  {
    name: "get_item",
    description: "Fetch the full text of a single clipboard entry by id.",
    inputSchema: {
      type: "object",
      properties: { id: { type: "string", description: "Clipboard entry id from list_history." } },
      required: ["id"],
    },
  },
  {
    name: "search_history",
    description: "Case-insensitive substring search across clipboard history.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Substring to match." },
        limit: { type: "number", description: "Max matches (default 20).", minimum: 1, maximum: 100 },
      },
      required: ["query"],
    },
  },
  {
    name: "copy_item",
    description: "Restore a stored entry back to the system clipboard so the user can paste it.",
    inputSchema: {
      type: "object",
      properties: { id: { type: "string", description: "Clipboard entry id." } },
      required: ["id"],
    },
  },
  {
    name: "get_current_clipboard",
    description: "Return the text currently on the system clipboard (without modifying history).",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "clear_history",
    description: "Wipe all stored clipboard history. The current system clipboard is untouched.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "get_stats",
    description: "Show counts, file path, oldest/newest timestamps for the history store.",
    inputSchema: { type: "object", properties: {} },
  },
];

const server = new Server(
  { name: "clipboard-history-mcp", version: "0.1.0" },
  { capabilities: { tools: {} } },
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools }));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  const { name, arguments: args = {} } = req.params;
  try {
    switch (name) {
      case "list_history": {
        const items = store.list({ limit: args.limit ?? 20, offset: args.offset ?? 0 });
        return jsonResult({ count: items.length, items });
      }
      case "get_item": {
        const item = store.get(String(args.id));
        if (!item) return errorResult(`No entry with id ${args.id}`);
        return jsonResult(item);
      }
      case "search_history": {
        const items = store.search({ query: String(args.query ?? ""), limit: args.limit ?? 20 });
        return jsonResult({ count: items.length, items });
      }
      case "copy_item": {
        const item = store.get(String(args.id));
        if (!item) return errorResult(`No entry with id ${args.id}`);
        await writeClipboard(item.text);
        return jsonResult({ ok: true, id: item.id, length: item.length });
      }
      case "get_current_clipboard": {
        const text = await readClipboard();
        return jsonResult({ length: text.length, preview: text.slice(0, 200), text });
      }
      case "clear_history": {
        const removed = store.clear();
        return jsonResult({ ok: true, removed });
      }
      case "get_stats":
        return jsonResult(store.stats());
      default:
        return errorResult(`Unknown tool: ${name}`);
    }
  } catch (err) {
    return errorResult(err instanceof Error ? err.message : String(err));
  }
});

function jsonResult(data) {
  return { content: [{ type: "text", text: JSON.stringify(data, null, 2) }] };
}

function errorResult(message) {
  return { isError: true, content: [{ type: "text", text: message }] };
}

const transport = new StdioServerTransport();
await server.connect(transport);

const shutdown = () => {
  stopWatcher();
  process.exit(0);
};
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
