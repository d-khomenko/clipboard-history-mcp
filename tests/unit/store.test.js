import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { openDb } from '../../src/core/db.js';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rmSync, mkdtempSync } from 'node:fs';
import { Store } from '../../src/core/store.js';
import { randomBytes } from 'node:crypto';

describe('db: schema and migrations', () => {
  let tmpDir, dbPath, db;
  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'cbhist-'));
    dbPath = join(tmpDir, 'test.db');
    db = openDb(dbPath);
  });
  afterEach(() => {
    db.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('creates clips, secrets, kinds, tags, meta tables', () => {
    const tables = db
      .prepare(`SELECT name FROM sqlite_master WHERE type='table' ORDER BY name`)
      .all()
      .map((r) => r.name);
    expect(tables).toEqual(
      expect.arrayContaining(['clips', 'secrets', 'kinds', 'tags', 'meta'])
    );
  });

  it('creates an FTS5 virtual table', () => {
    const fts = db
      .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name='clips_fts'`)
      .get();
    expect(fts).toBeTruthy();
  });

  it('records the schema version in meta', () => {
    const v = db.prepare(`SELECT value FROM meta WHERE key='schema_version'`).get();
    expect(parseInt(v.value, 10)).toBeGreaterThanOrEqual(1);
  });

  it('enables WAL journal mode', () => {
    const mode = db.pragma('journal_mode', { simple: true });
    expect(mode).toBe('wal');
  });
});

describe('Store', () => {
  let tmpDir, store;
  const key = randomBytes(32);

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'cbhist-store-'));
    store = new Store(join(tmpDir, 'test.db'), { masterKey: key });
  });
  afterEach(() => {
    store.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('inserts a clean clip', () => {
    const id = store.addClip({
      text: 'hello world',
      primaryKind: 'text',
      kinds: ['text'],
      sourceApp: 'Terminal',
    });
    expect(id).toBeGreaterThan(0);
    const item = store.getItem(id);
    expect(item.text).toBe('hello world');
    expect(item.primaryKind).toBe('text');
    expect(item.sourceApp).toBe('Terminal');
  });

  it('dedups identical text by hash', () => {
    const id1 = store.addClip({ text: 'same', primaryKind: 'text', kinds: ['text'] });
    const id2 = store.addClip({ text: 'same', primaryKind: 'text', kinds: ['text'] });
    expect(id2).toBe(id1);
    const item = store.getItem(id1);
    expect(item.copyCount).toBe(2);
  });

  it('stores secret with encryption and redacted preview', () => {
    const id = store.addSecret({
      text: 'sk-proj-XYZ',
      secretKind: 'openai_api_key',
      sourceApp: 'Safari',
      windowTitle: 'OpenAI Platform — API Keys',
    });
    const item = store.getItem(id);
    expect(item.text).toBeNull();
    expect(item.preview).toMatch(/REDACTED/);
    expect(item.windowTitle).toBe('OpenAI Platform — API Keys');
    expect(store.unlockSecret(id)).toBe('sk-proj-XYZ');
  });

  it('FTS search hits secret by window title', () => {
    store.addSecret({
      text: 'sk-proj-XYZ',
      secretKind: 'openai_api_key',
      sourceApp: 'Safari',
      windowTitle: 'OpenAI Platform — API Keys',
    });
    const results = store.search({ query: 'OpenAI' });
    expect(results.length).toBeGreaterThan(0);
    expect(results[0].text).toBeNull();
  });

  it('lists with kind filter', () => {
    store.addClip({ text: 'https://a.com', primaryKind: 'url', kinds: ['url'] });
    store.addClip({ text: 'plain', primaryKind: 'text', kinds: ['text'] });
    const onlyUrls = store.list({ kind: 'url' });
    expect(onlyUrls.length).toBe(1);
    expect(onlyUrls[0].primaryKind).toBe('url');
  });

  it('clear scopes', () => {
    store.addClip({ text: 'a', primaryKind: 'text', kinds: ['text'] });
    store.addClip({ text: 'b', primaryKind: 'text', kinds: ['text'] });
    expect(store.clear({ scope: 'all' })).toBe(2);
    expect(store.list({}).length).toBe(0);
  });
});
