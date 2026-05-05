import { createHash, randomUUID } from 'node:crypto';
import { openDb } from './db.js';
import { encrypt, decrypt } from './crypto.js';

export class Store {
  constructor(path, { masterKey } = {}) {
    this.db = openDb(path);
    this.masterKey = masterKey;
    this.prepared = {
      insertClip: this.db.prepare(`
        INSERT INTO clips
          (uuid, text, preview, length, byte_length, hash, primary_kind,
           source_app, window_title, first_copied_at, last_copied_at)
        VALUES
          (@uuid, @text, @preview, @length, @byteLength, @hash, @primaryKind,
           @sourceApp, @windowTitle, @now, @now)
      `),
      bumpDuplicate: this.db.prepare(`
        UPDATE clips SET copy_count = copy_count + 1, last_copied_at = ?
        WHERE id = ?
      `),
      findHash: this.db.prepare(`SELECT id FROM clips WHERE hash = ? LIMIT 1`),
      insertKind: this.db.prepare(`INSERT OR IGNORE INTO kinds(clip_id, kind) VALUES (?, ?)`),
      insertTag: this.db.prepare(`INSERT OR IGNORE INTO tags(clip_id, tag) VALUES (?, ?)`),
      removeTag: this.db.prepare(`DELETE FROM tags WHERE clip_id = ? AND tag = ?`),
      insertSecret: this.db.prepare(`
        INSERT INTO secrets(clip_id, ciphertext, nonce, secret_kind, last_chars)
        VALUES (?, ?, ?, ?, ?)
      `),
      getSecret: this.db.prepare(`SELECT * FROM secrets WHERE clip_id = ?`),
      bumpUnlock: this.db.prepare(
        `UPDATE secrets SET unlock_count = unlock_count + 1 WHERE clip_id = ?`
      ),
      getById: this.db.prepare(`SELECT * FROM clips WHERE id = ?`),
      getKinds: this.db.prepare(`SELECT kind FROM kinds WHERE clip_id = ?`),
      getTags: this.db.prepare(`SELECT tag FROM tags WHERE clip_id = ?`),
      pin: this.db.prepare(`UPDATE clips SET is_pinned = ? WHERE id = ?`),
      bumpPaste: this.db.prepare(
        `UPDATE clips SET paste_count = paste_count + 1 WHERE id = ?`
      ),
      remove: this.db.prepare(`DELETE FROM clips WHERE id = ?`),
    };
  }

  close() {
    this.db.close();
  }

  addClip({ text, primaryKind, kinds = [], sourceApp = null, windowTitle = null }) {
    const hash = sha256(text);
    const existing = this.prepared.findHash.get(hash);
    const now = Date.now();
    if (existing) {
      this.prepared.bumpDuplicate.run(now, existing.id);
      return existing.id;
    }
    const insert = this.db.transaction(() => {
      const r = this.prepared.insertClip.run({
        uuid: randomUUID(),
        text,
        preview: text.slice(0, 200),
        length: text.length,
        byteLength: Buffer.byteLength(text, 'utf8'),
        hash,
        primaryKind,
        sourceApp,
        windowTitle,
        now,
      });
      const id = Number(r.lastInsertRowid);
      for (const k of new Set(kinds.length ? kinds : [primaryKind])) {
        this.prepared.insertKind.run(id, k);
      }
      return id;
    });
    return insert();
  }

  addSecret({ text, secretKind, sourceApp = null, windowTitle = null }) {
    if (!this.masterKey) throw new Error('master key not provided');
    const lastChars = text.slice(-Math.min(6, text.length));
    const preview = `[REDACTED:${secretKind}]`;
    const hash = sha256(text);
    const now = Date.now();
    const insert = this.db.transaction(() => {
      const r = this.prepared.insertClip.run({
        uuid: randomUUID(),
        text: null,
        preview,
        length: text.length,
        byteLength: Buffer.byteLength(text, 'utf8'),
        hash,
        primaryKind: `secret:${secretKind}`,
        sourceApp,
        windowTitle,
        now,
      });
      const id = Number(r.lastInsertRowid);
      this.prepared.insertKind.run(id, `secret:${secretKind}`);
      const { ciphertext, nonce } = encrypt(text, this.masterKey);
      this.prepared.insertSecret.run(id, ciphertext, nonce, secretKind, lastChars);
      return id;
    });
    return insert();
  }

  getItem(id) {
    const row = this.prepared.getById.get(id);
    if (!row) return null;
    const kinds = this.prepared.getKinds.all(id).map((r) => r.kind);
    const tags = this.prepared.getTags.all(id).map((r) => r.tag);
    return rowToItem(row, kinds, tags);
  }

  unlockSecret(id) {
    const row = this.prepared.getSecret.get(id);
    if (!row) throw new Error(`No secret for clip ${id}`);
    if (!row.ciphertext) throw new Error('Secret stored with NEVER_STORE_SECRETS=1');
    if (!this.masterKey) throw new Error('master key not provided');
    const value = decrypt(row.ciphertext, row.nonce, this.masterKey);
    this.prepared.bumpUnlock.run(id);
    return value;
  }

  list({ limit = 20, offset = 0, kind = null, sourceApp = null, since = null, pinnedOnly = false } = {}) {
    const where = [];
    const params = {};
    if (kind) {
      where.push(
        `id IN (SELECT clip_id FROM kinds WHERE kind = @kind)`
      );
      params.kind = kind;
    }
    if (sourceApp) { where.push(`source_app = @sourceApp`); params.sourceApp = sourceApp; }
    if (since) { where.push(`last_copied_at >= @since`); params.since = since; }
    if (pinnedOnly) where.push(`is_pinned = 1`);
    const sql = `
      SELECT * FROM clips
      ${where.length ? 'WHERE ' + where.join(' AND ') : ''}
      ORDER BY is_pinned DESC, last_copied_at DESC
      LIMIT @limit OFFSET @offset
    `;
    params.limit = limit;
    params.offset = offset;
    const rows = this.db.prepare(sql).all(params);
    return rows.map((r) => {
      const kinds = this.prepared.getKinds.all(r.id).map((x) => x.kind);
      const tags = this.prepared.getTags.all(r.id).map((x) => x.tag);
      return rowToItem(r, kinds, tags);
    });
  }

  search({ query, limit = 20 }) {
    const ftsQuery = sanitizeFts(query);
    const rows = this.db.prepare(`
      SELECT clips.*, bm25(clips_fts) AS rank
      FROM clips_fts
      JOIN clips ON clips.id = clips_fts.rowid
      WHERE clips_fts MATCH ?
      ORDER BY rank
      LIMIT ?
    `).all(ftsQuery, limit);
    return rows.map((r) => {
      const kinds = this.prepared.getKinds.all(r.id).map((x) => x.kind);
      const tags = this.prepared.getTags.all(r.id).map((x) => x.tag);
      return rowToItem(r, kinds, tags);
    });
  }

  pin(id, value = true) { this.prepared.pin.run(value ? 1 : 0, id); }
  unpin(id) { this.pin(id, false); }
  tag(id, tag) { this.prepared.insertTag.run(id, tag); }
  untag(id, tag) { this.prepared.removeTag.run(id, tag); }
  bumpPaste(id) { this.prepared.bumpPaste.run(id); }
  delete(id) { this.prepared.remove.run(id); }

  pruneOldest({ keep }) {
    return this.db.prepare(
      `DELETE FROM clips WHERE id IN (SELECT id FROM clips ORDER BY is_pinned DESC, last_copied_at DESC LIMIT -1 OFFSET ?)`
    ).run(keep).changes;
  }

  clear({ scope = 'all' } = {}) {
    if (scope === 'all') {
      const r = this.db.prepare(`DELETE FROM clips`).run();
      return r.changes;
    }
    if (scope.startsWith('older_than_days:')) {
      const days = parseInt(scope.split(':')[1], 10);
      const cutoff = Date.now() - days * 86_400_000;
      return this.db.prepare(`DELETE FROM clips WHERE last_copied_at < ?`).run(cutoff).changes;
    }
    if (scope.startsWith('kind:')) {
      const k = scope.split(':')[1];
      return this.db.prepare(
        `DELETE FROM clips WHERE primary_kind = ? OR id IN (SELECT clip_id FROM kinds WHERE kind = ?)`
      ).run(k, k).changes;
    }
    throw new Error(`Unknown clear scope: ${scope}`);
  }

  stats() {
    const total = this.db.prepare(`SELECT COUNT(*) AS n FROM clips`).get().n;
    const oldest = this.db.prepare(`SELECT MIN(first_copied_at) AS t FROM clips`).get().t;
    const newest = this.db.prepare(`SELECT MAX(last_copied_at) AS t FROM clips`).get().t;
    const byKind = this.db.prepare(
      `SELECT primary_kind AS kind, COUNT(*) AS n FROM clips GROUP BY primary_kind`
    ).all();
    return { count: total, oldest, newest, byKind };
  }
}

function sha256(text) {
  return createHash('sha256').update(text).digest('hex');
}

function rowToItem(row, kinds, tags) {
  return {
    id: row.id,
    uuid: row.uuid,
    text: row.text,
    preview: row.preview,
    length: row.length,
    primaryKind: row.primary_kind,
    kinds,
    tags,
    sourceApp: row.source_app,
    windowTitle: row.window_title,
    firstCopiedAt: row.first_copied_at,
    lastCopiedAt: row.last_copied_at,
    copyCount: row.copy_count,
    pasteCount: row.paste_count,
    isPinned: row.is_pinned === 1,
  };
}

function sanitizeFts(q) {
  return q.replace(/["*()]/g, ' ').trim().split(/\s+/).filter(Boolean).join(' ');
}
