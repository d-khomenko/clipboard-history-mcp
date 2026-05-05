#!/usr/bin/env node
import { join } from 'node:path';
import { mkdirSync } from 'node:fs';
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

mkdirSync(DEFAULT_DIR, { recursive: true });

const masterKey = getOrCreateMasterKey();
const store = new Store(DEFAULT_DB, { masterKey });

await loadAllTools();

const server = new Server(
  { name: 'clipboard-history-mcp', version: '0.2.0' },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools: listTools() };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  try {
    const result = await callTool(name, args, { store });
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
