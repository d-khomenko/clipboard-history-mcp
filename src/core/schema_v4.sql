-- Schema v4: add ocr_text column to clips and rebuild clips_fts to include it.

ALTER TABLE clips ADD COLUMN ocr_text TEXT;
  -- For image clips: text extracted via VisionKit OCR at ingest time, or NULL otherwise.

-- Drop the old FTS triggers before rebuilding the virtual table. They
-- reference the old clips_fts schema (without ocr_text) and must be
-- recreated to match the new column set.
DROP TRIGGER IF EXISTS clips_ai;
DROP TRIGGER IF EXISTS clips_ad;
DROP TRIGGER IF EXISTS clips_au;

-- SQLite FTS5 does not support ALTER TABLE, so we drop and recreate the
-- virtual table, then repopulate from existing clips.
DROP TABLE IF EXISTS clips_fts;
CREATE VIRTUAL TABLE clips_fts USING fts5(
  preview, window_title, primary_kind, ocr_text,
  content='clips', content_rowid='id', tokenize='porter unicode61'
);
INSERT INTO clips_fts(rowid, preview, window_title, primary_kind, ocr_text)
  SELECT id, preview, coalesce(window_title, ''), primary_kind, coalesce(ocr_text, '')
  FROM clips;

-- Recreate AFTER INSERT/UPDATE/DELETE triggers that keep clips_fts in sync.
-- (Mirrors the trigger definitions from schema_v1.sql, extended for ocr_text.)

CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN
  INSERT INTO clips_fts(rowid, preview, window_title, primary_kind, ocr_text)
  VALUES (new.id, new.preview, coalesce(new.window_title, ''), new.primary_kind, coalesce(new.ocr_text, ''));
END;

CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN
  INSERT INTO clips_fts(clips_fts, rowid, preview, window_title, primary_kind, ocr_text)
  VALUES ('delete', old.id, old.preview, coalesce(old.window_title, ''), old.primary_kind, coalesce(old.ocr_text, ''));
END;

CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN
  INSERT INTO clips_fts(clips_fts, rowid, preview, window_title, primary_kind, ocr_text)
  VALUES ('delete', old.id, old.preview, coalesce(old.window_title, ''), old.primary_kind, coalesce(old.ocr_text, ''));
  INSERT INTO clips_fts(rowid, preview, window_title, primary_kind, ocr_text)
  VALUES (new.id, new.preview, coalesce(new.window_title, ''), new.primary_kind, coalesce(new.ocr_text, ''));
END;

UPDATE meta SET value = '4' WHERE key = 'schema_version';
