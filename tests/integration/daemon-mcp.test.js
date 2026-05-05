import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Store } from '../../src/core/store.js';
import { startWatcher } from '../../src/daemon/watcher.js';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';

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
