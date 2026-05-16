-- Schema v2: add payload metadata columns to support image and file clips.

ALTER TABLE clips ADD COLUMN payload_kind TEXT NOT NULL DEFAULT 'text';
  -- 'text' | 'image' | 'file'
ALTER TABLE clips ADD COLUMN blob_path TEXT;
  -- relative path under data_dir/blobs/ ('aa/<sha256>.<ext>'). NULL for text.
ALTER TABLE clips ADD COLUMN blob_size_bytes INTEGER;
  -- NULL for text. Size of the blob file on disk in bytes.
ALTER TABLE clips ADD COLUMN mime_type TEXT;
  -- e.g. 'image/png', 'image/tiff', 'application/octet-stream'. NULL for text.

CREATE INDEX IF NOT EXISTS idx_clips_payload_kind ON clips(payload_kind);

UPDATE meta SET value = '2' WHERE key = 'schema_version';
