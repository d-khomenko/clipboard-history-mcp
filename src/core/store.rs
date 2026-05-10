use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;

use super::crypto::{decrypt, encrypt};
use super::db::open_db;

pub struct ClipInput {
    pub text: String,
    pub primary_kind: String,
    pub kinds: Vec<String>,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
}

pub struct SecretInput {
    pub text: String,
    pub secret_kind: String,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct Item {
    pub id: i64,
    pub uuid: String,
    pub text: Option<String>,
    pub preview: String,
    pub length: i64,
    pub primary_kind: String,
    pub kinds: Vec<String>,
    pub tags: Vec<String>,
    pub source_app: Option<String>,
    pub window_title: Option<String>,
    pub first_copied_at: i64,
    pub last_copied_at: i64,
    pub copy_count: i64,
    pub paste_count: i64,
    pub is_pinned: bool,
}

pub struct Store {
    conn: Connection,
    master_key: [u8; 32],
}

impl Store {
    pub fn open<P: AsRef<Path>>(path: P, master_key: [u8; 32]) -> Result<Self> {
        let conn = open_db(path)?;
        Ok(Self { conn, master_key })
    }

    pub fn add_clip(&self, input: ClipInput) -> Result<i64> {
        let hash = sha256_hex(&input.text);
        let now = now_ms();
        if let Some(id) = self.find_by_hash(&hash)? {
            self.conn.execute(
                "UPDATE clips SET copy_count = copy_count + 1, last_copied_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            return Ok(id);
        }
        let preview: String = input.text.chars().take(200).collect();
        let length = input.text.chars().count() as i64;
        let byte_length = input.text.len() as i64;
        let uuid = Uuid::new_v4().to_string();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                source_app, window_title, first_copied_at, last_copied_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![uuid, input.text, preview, length, byte_length, hash, input.primary_kind,
                    input.source_app, input.window_title, now],
        )?;
        let id = tx.last_insert_rowid();
        let kinds = if input.kinds.is_empty() { vec![input.primary_kind.clone()] } else { input.kinds };
        for k in kinds.iter().collect::<std::collections::BTreeSet<_>>() {
            tx.execute("INSERT OR IGNORE INTO kinds(clip_id, kind) VALUES (?1, ?2)", params![id, k])?;
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn add_secret(&self, input: SecretInput) -> Result<i64> {
        let last_chars: String = input.text.chars().rev().take(6).collect::<String>().chars().rev().collect();
        let preview = format!("[REDACTED:{}]", input.secret_kind);
        let hash = sha256_hex(&input.text);
        let now = now_ms();
        let uuid = Uuid::new_v4().to_string();
        let primary_kind = format!("secret:{}", input.secret_kind);
        let length = input.text.chars().count() as i64;
        let byte_length = input.text.len() as i64;
        let (ciphertext, nonce) = encrypt(&input.text, &self.master_key)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                source_app, window_title, first_copied_at, last_copied_at)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![uuid, preview, length, byte_length, hash, primary_kind,
                    input.source_app, input.window_title, now],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT OR IGNORE INTO kinds(clip_id, kind) VALUES (?1, ?2)", params![id, &primary_kind])?;
        tx.execute(
            "INSERT INTO secrets(clip_id, ciphertext, nonce, secret_kind, last_chars) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, ciphertext, nonce.to_vec(), input.secret_kind, last_chars],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn get_item(&self, id: i64) -> Result<Option<Item>> {
        let row = self.conn.query_row(
            "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                    first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
             FROM clips WHERE id = ?1",
            params![id],
            |r| Ok(Item {
                id: r.get(0)?, uuid: r.get(1)?, text: r.get(2)?, preview: r.get(3)?,
                length: r.get(4)?, primary_kind: r.get(5)?, kinds: vec![], tags: vec![],
                source_app: r.get(6)?, window_title: r.get(7)?,
                first_copied_at: r.get(8)?, last_copied_at: r.get(9)?,
                copy_count: r.get(10)?, paste_count: r.get(11)?,
                is_pinned: r.get::<_, i64>(12)? == 1,
            }),
        ).optional()?;
        let Some(mut item) = row else { return Ok(None) };
        item.kinds = self.kinds_for(item.id)?;
        item.tags = self.tags_for(item.id)?;
        Ok(Some(item))
    }

    pub fn unlock_secret(&self, id: i64) -> Result<String> {
        let row: Option<(Vec<u8>, Vec<u8>)> = self.conn.query_row(
            "SELECT ciphertext, nonce FROM secrets WHERE clip_id = ?1",
            params![id], |r| Ok((r.get(0)?, r.get(1)?))
        ).optional()?;
        let (ct, nonce) = row.ok_or_else(|| anyhow!("no secret for clip {}", id))?;
        let value = decrypt(&ct, &nonce, &self.master_key)?;
        self.conn.execute("UPDATE secrets SET unlock_count = unlock_count + 1 WHERE clip_id = ?1", params![id])?;
        Ok(value)
    }

    pub fn list(&self, limit: i64) -> Result<Vec<Item>> {
        self.list_with(None, limit, 0, false)
    }

    pub fn list_with(&self, kind: Option<&str>, limit: i64, offset: i64, pinned_only: bool) -> Result<Vec<Item>> {
        let sql = match (kind.is_some(), pinned_only) {
            (true, true) => {
                "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                        first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
                 FROM clips WHERE is_pinned = 1
                   AND id IN (SELECT clip_id FROM kinds WHERE kind = ?1)
                 ORDER BY last_copied_at DESC LIMIT ?2 OFFSET ?3"
            }
            (true, false) => {
                "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                        first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
                 FROM clips WHERE id IN (SELECT clip_id FROM kinds WHERE kind = ?1)
                 ORDER BY is_pinned DESC, last_copied_at DESC LIMIT ?2 OFFSET ?3"
            }
            (false, true) => {
                "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                        first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
                 FROM clips WHERE is_pinned = 1
                 ORDER BY last_copied_at DESC LIMIT ?1 OFFSET ?2"
            }
            (false, false) => {
                "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                        first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
                 FROM clips ORDER BY is_pinned DESC, last_copied_at DESC LIMIT ?1 OFFSET ?2"
            }
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = if let Some(k) = kind {
            stmt.query_map(params![k, limit, offset], row_to_item)?.collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![limit, offset], row_to_item)?.collect::<Result<Vec<_>, _>>()?
        };
        let mut out = Vec::with_capacity(rows.len());
        for mut item in rows {
            item.kinds = self.kinds_for(item.id)?;
            item.tags = self.tags_for(item.id)?;
            out.push(item);
        }
        Ok(out)
    }

    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<Item>> {
        let sanitized: String = query.chars()
            .filter(|c| !"\"*()".contains(*c))
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if sanitized.is_empty() { return Ok(Vec::new()); }
        let mut stmt = self.conn.prepare(
            "SELECT clips.id, clips.uuid, clips.text, clips.preview, clips.length, clips.primary_kind,
                    clips.source_app, clips.window_title, clips.first_copied_at, clips.last_copied_at,
                    clips.copy_count, clips.paste_count, clips.is_pinned, bm25(clips_fts) AS rank
             FROM clips_fts JOIN clips ON clips.id = clips_fts.rowid
             WHERE clips_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        )?;
        let rows: Vec<Item> = stmt.query_map(params![sanitized, limit], row_to_item_with_rank)?
            .collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for mut item in rows {
            item.kinds = self.kinds_for(item.id)?;
            item.tags = self.tags_for(item.id)?;
            out.push(item);
        }
        Ok(out)
    }

    pub fn pin(&self, id: i64, pinned: bool) -> Result<()> {
        self.conn.execute("UPDATE clips SET is_pinned = ?1 WHERE id = ?2", params![pinned as i64, id])?;
        Ok(())
    }
    pub fn tag(&self, id: i64, tag: &str) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO tags(clip_id, tag) VALUES (?1, ?2)", params![id, tag])?;
        Ok(())
    }
    pub fn untag(&self, id: i64, tag: &str) -> Result<()> {
        self.conn.execute("DELETE FROM tags WHERE clip_id = ?1 AND tag = ?2", params![id, tag])?;
        Ok(())
    }
    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
        Ok(())
    }
    pub fn bump_paste(&self, id: i64) -> Result<()> {
        self.conn.execute("UPDATE clips SET paste_count = paste_count + 1 WHERE id = ?1", params![id])?;
        Ok(())
    }
    pub fn clear_all(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM clips", [])?)
    }
    pub fn clear_older_than_days(&self, days: i64) -> Result<usize> {
        let cutoff = now_ms() - days * 86_400_000;
        Ok(self.conn.execute("DELETE FROM clips WHERE last_copied_at < ?1", params![cutoff])?)
    }
    pub fn clear_kind(&self, kind: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM clips WHERE primary_kind = ?1 OR id IN (SELECT clip_id FROM kinds WHERE kind = ?1)",
            params![kind],
        )?)
    }
    pub fn prune_oldest(&self, keep: i64) -> Result<usize> {
        // Pinned clips are NEVER pruned — the ring buffer applies only to
        // unpinned items. The user's pin list is a contract.
        Ok(self.conn.execute(
            "DELETE FROM clips WHERE is_pinned = 0 AND id IN (
               SELECT id FROM clips WHERE is_pinned = 0
                 ORDER BY last_copied_at DESC LIMIT -1 OFFSET ?1
             )",
            params![keep],
        )?)
    }

    pub fn stats(&self) -> Result<Stats> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))?;
        let oldest: Option<i64> = self.conn.query_row("SELECT MIN(first_copied_at) FROM clips", [], |r| r.get(0)).ok().flatten();
        let newest: Option<i64> = self.conn.query_row("SELECT MAX(last_copied_at) FROM clips", [], |r| r.get(0)).ok().flatten();
        Ok(Stats { count, oldest, newest })
    }

    fn find_by_hash(&self, hash: &str) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT id FROM clips WHERE hash = ?1 LIMIT 1", params![hash], |r| r.get(0)).optional()?)
    }
    fn kinds_for(&self, id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT kind FROM kinds WHERE clip_id = ?1")?;
        let result = stmt.query_map(params![id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
        Ok(result)
    }
    fn tags_for(&self, id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT tag FROM tags WHERE clip_id = ?1")?;
        let result = stmt.query_map(params![id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
        Ok(result)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct Stats { pub count: i64, pub oldest: Option<i64>, pub newest: Option<i64> }

fn row_to_item(r: &rusqlite::Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: r.get(0)?, uuid: r.get(1)?, text: r.get(2)?, preview: r.get(3)?,
        length: r.get(4)?, primary_kind: r.get(5)?, kinds: vec![], tags: vec![],
        source_app: r.get(6)?, window_title: r.get(7)?,
        first_copied_at: r.get(8)?, last_copied_at: r.get(9)?,
        copy_count: r.get(10)?, paste_count: r.get(11)?,
        is_pinned: r.get::<_, i64>(12)? == 1,
    })
}
fn row_to_item_with_rank(r: &rusqlite::Row) -> rusqlite::Result<Item> { row_to_item(r) }

fn sha256_hex(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    hex::encode(h.finalize())
}
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

use rusqlite::OptionalExtension;
