import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { openDb } from '../../src/core/db.js';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rmSync, mkdtempSync } from 'node:fs';

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
