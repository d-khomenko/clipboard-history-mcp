import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Store } from '../../src/core/store.js';
import { startWatcher } from '../../src/daemon/watcher.js';
import { spawnSync, spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { fileURLToPath } from 'node:url';

describe('daemon: end-to-end capture into Store', () => {
  let tmpDir, store, stop;
  beforeAll(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'cbhist-int-'));
    store = new Store(join(tmpDir, 'test.db'), { masterKey: randomBytes(32) });
    stop = startWatcher(store, { intervalMs: 250, log: () => {} });
  });
  afterAll(() => {
    stop();
    store.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('captures a copied URL', async () => {
    const url = `https://test.example/${Date.now()}`;
    spawnSync('pbcopy', [], { input: url });
    await new Promise((r) => setTimeout(r, 600));
    const items = store.list({ kind: 'url' });
    expect(items.some((i) => i.text === url)).toBe(true);
  });
});

describe('mcp: list_tools and tools/call through stdio', () => {
  let mcpTmpDir;
  it('handshake + tools/list', async () => {
    mcpTmpDir = mkdtempSync(join(tmpdir(), 'cbhist-mcp-'));
    const mcp = spawn('node', [
      fileURLToPath(new URL('../../src/mcp/index.js', import.meta.url)),
    ], {
      env: { ...process.env, CLIPBOARD_DATA_DIR: mcpTmpDir, CLIPBOARD_DB_PATH: join(mcpTmpDir, 'test.db') },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const responses = [];
    mcp.stdout.on('data', (chunk) => {
      for (const line of chunk.toString('utf8').split('\n').filter(Boolean)) {
        try { responses.push(JSON.parse(line)); } catch { /* */ }
      }
    });
    mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 't', version: '1' } } }) + '\n');
    mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
    mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list' }) + '\n');
    await new Promise((r) => setTimeout(r, 1500));
    mcp.kill();

    expect(responses.find((r) => r.id === 1)).toBeTruthy();
    const list = responses.find((r) => r.id === 2);
    expect(list?.result?.tools?.length).toBeGreaterThanOrEqual(15);
    rmSync(mcpTmpDir, { recursive: true, force: true });
  });
});
