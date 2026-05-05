import Database from 'better-sqlite3';
import { dirname } from 'node:path';
import { mkdirSync } from 'node:fs';

const MIGRATIONS = [
  function v1(db) {
    db.exec(`
      CREATE TABLE clips (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        uuid TEXT NOT NULL UNIQUE,
        text TEXT,
        preview TEXT NOT NULL,
        length INTEGER NOT NULL,
        byte_length INTEGER NOT NULL,
        hash TEXT NOT NULL,
        primary_kind TEXT NOT NULL,
        source_app TEXT,
        window_title TEXT,
        first_copied_at INTEGER NOT NULL,
        last_copied_at INTEGER NOT NULL,
        copy_count INTEGER NOT NULL DEFAULT 1,
        paste_count INTEGER NOT NULL DEFAULT 0,
        is_pinned INTEGER NOT NULL DEFAULT 0
      );
      CREATE INDEX idx_clips_hash ON clips(hash);
      CREATE INDEX idx_clips_kind ON clips(primary_kind);
      CREATE INDEX idx_clips_last_copied ON clips(last_copied_at DESC);

      CREATE TABLE kinds (
        clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        PRIMARY KEY (clip_id, kind)
      );

      CREATE TABLE tags (
        clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
        tag TEXT NOT NULL,
        PRIMARY KEY (clip_id, tag)
      );

      CREATE TABLE secrets (
        clip_id INTEGER PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
        ciphertext BLOB,
        nonce BLOB,
        secret_kind TEXT NOT NULL,
        last_chars TEXT NOT NULL,
        unlock_count INTEGER NOT NULL DEFAULT 0
      );

      CREATE VIRTUAL TABLE clips_fts USING fts5(
        preview, window_title, primary_kind,
        content='clips', content_rowid='id', tokenize='porter unicode61'
      );

      CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN
        INSERT INTO clips_fts(rowid, preview, window_title, primary_kind)
        VALUES (new.id, new.preview, coalesce(new.window_title, ''), new.primary_kind);
      END;

      CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN
        INSERT INTO clips_fts(clips_fts, rowid, preview, window_title, primary_kind)
        VALUES ('delete', old.id, old.preview, coalesce(old.window_title, ''), old.primary_kind);
      END;

      CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN
        INSERT INTO clips_fts(clips_fts, rowid, preview, window_title, primary_kind)
        VALUES ('delete', old.id, old.preview, coalesce(old.window_title, ''), old.primary_kind);
        INSERT INTO clips_fts(rowid, preview, window_title, primary_kind)
        VALUES (new.id, new.preview, coalesce(new.window_title, ''), new.primary_kind);
      END;

      CREATE TABLE meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );
    `);
    db.prepare(`INSERT INTO meta(key, value) VALUES (?, ?)`).run('schema_version', '1');
  },
];

export function openDb(path) {
  mkdirSync(dirname(path), { recursive: true });
  const db = new Database(path);
  db.pragma('journal_mode = WAL');
  db.pragma('foreign_keys = ON');
  db.pragma('synchronous = NORMAL');

  let current = 0;
  try {
    const meta = db.prepare(
      `SELECT value FROM meta WHERE key='schema_version'`
    );
    /** @type {{ value: string } | undefined} */ const row = /** @type {any} */ (meta.get());
    if (row) current = parseInt(row.value, 10);
  } catch {
    current = 0;
  }

  for (let i = current; i < MIGRATIONS.length; i++) {
    const fn = MIGRATIONS[i];
    db.transaction(fn)(db);
  }

  return db;
}
