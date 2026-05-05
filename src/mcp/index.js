#!/usr/bin/env node
import { join } from 'node:path';
import { mkdirSync, statSync, existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { ListToolsRequestSchema, CallToolRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';
import { loadAllTools, listTools, callTool } from '../tools/index.js';

const DEFAULT_DIR = process.env.CLIPBOARD_DATA_DIR
  || join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const DEFAULT_DB = process.env.CLIPBOARD_DB_PATH || join(DEFAULT_DIR, 'history.db');
const PID_FILE = join(DEFAULT_DIR, 'daemon.pid');

mkdirSync(DEFAULT_DIR, { recursive: true });

const masterKey = getOrCreateMasterKey();
const store = new Store(DEFAULT_DB, { masterKey });

await loadAllTools();

const server = new Server(
  { name: 'clipboard-history-mcp', version: '0.2.0' },
  { capabilities: { tools: {} } }
);

const ctx = {
  store,
  dbPath: DEFAULT_DB,
  statSync,
  daemonStatus: () => {
    if (!existsSync(PID_FILE)) return { running: false };
    try {
      const pid = parseInt(readFileSync(PID_FILE, 'utf8'), 10);
      process.kill(pid, 0);
      return { running: true, pid };
    } catch {
      return { running: false };
    }
  },
  log: (msg) => process.stderr.write(`[mcp] ${new Date().toISOString()} ${msg}\n`),
};

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools: listTools() };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  try {
    const result = await callTool(name, args, ctx);
    return {
      content: [{ type: 'text', text: typeof result === 'string' ? result : JSON.stringify(result, null, 2) }],
    };
  } catch (err) {
    return {
      isError: true,
      content: [{ type: 'text', text: err.message }],
    };
  }
});

const transport = new StdioServerTransport();
await server.connect(transport);
