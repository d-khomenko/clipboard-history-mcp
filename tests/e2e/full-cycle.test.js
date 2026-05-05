import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Store } from '../../src/core/store.js';
import { startWatcher } from '../../src/daemon/watcher.js';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';

describe('e2e: copy → secret detected → unlock', () => {
  let tmpDir, store, stop, key;
  beforeAll(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'cbhist-e2e-'));
    key = randomBytes(32);
    store = new Store(join(tmpDir, 'test.db'), { masterKey: key });
    stop = startWatcher(store, { intervalMs: 200, log: () => {} });
  });
  afterAll(() => {
    stop();
    store.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('captures a copied OpenAI key as a secret and unlocks it', async () => {
    // Use a fixture matching the gitleaks openai rule shape (sk- prefix + ...T3BlbkFJ... + 20 alphanum)
    const fake = 'sk-' + 'A'.repeat(20) + 'T3BlbkFJ' + 'B'.repeat(20);
    spawnSync('pbcopy', [], { input: fake });
    await new Promise((r) => setTimeout(r, 600));
    const items = store.list({ limit: 5 });
    const secret = items.find((i) => i.primaryKind?.startsWith('secret:'));
    expect(secret).toBeTruthy();
    expect(secret.text).toBeNull();
    expect(secret.preview).toMatch(/REDACTED/);
    const value = store.unlockSecret(secret.id);
    expect(value).toBe(fake);
  });
});
