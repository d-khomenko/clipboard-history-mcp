-- Schema v3: audit-log table for tracking MCP reads + secret reveals.

CREATE TABLE IF NOT EXISTS audit_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    ts              INTEGER NOT NULL,                -- unix milliseconds
    actor           TEXT    NOT NULL,                -- e.g. 'mcp:claude-code', 'cli', 'daemon'
    verb            TEXT    NOT NULL,                -- 'read' | 'write' | 'reveal' | 'blocked'
    tool            TEXT    NOT NULL,                -- 'list_history' | 'search_history' | 'get_item' | 'unlock_secret' | etc.
    target_clip_id  INTEGER,                         -- nullable: list_history has no specific target
    detail          TEXT                              -- short context, e.g. search query, scope, or NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_log_ts ON audit_log(ts);
CREATE INDEX IF NOT EXISTS idx_audit_log_tool ON audit_log(tool);

ALTER TABLE secrets ADD COLUMN last_revealed_at INTEGER;
  -- unix milliseconds of the most recent successful reveal, NULL if never revealed.

UPDATE meta SET value = '3' WHERE key = 'schema_version';
