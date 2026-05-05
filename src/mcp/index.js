#!/usr/bin/env node
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';
import { statSync, existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';
import { listTools, callTool, loadAllTools } from '../tools/index.js';

const DEFAULT_DIR = process.env.CLIPBOARD_DATA_DIR
  || join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const DEFAULT_DB = process.env.CLIPBOARD_DB_PATH || join(DEFAULT_DIR, 'history.db');
const PID_FILE = join(DEFAULT_DIR, 'daemon.pid');

const masterKey = getOrCreateMasterKey();
const store = new Store(DEFAULT_DB, { masterKey });

await loadAllTools();

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

const server = new Server(
  { name: 'clipboard-history-mcp', version: '0.2.0-alpha.0' },
  { capabilities: { tools: {} } },
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: listTools() }));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  const { name, arguments: args } = req.params;
  try {
    const result = await callTool(name, args, ctx);
    return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
  } catch (err) {
    return {
      isError: true,
      content: [{ type: 'text', text: err instanceof Error ? err.message : String(err) }],
    };
  }
});

const transport = new StdioServerTransport();
await server.connect(transport);

process.on('SIGINT', () => { store.close(); process.exit(0); });
process.on('SIGTERM', () => { store.close(); process.exit(0); });
