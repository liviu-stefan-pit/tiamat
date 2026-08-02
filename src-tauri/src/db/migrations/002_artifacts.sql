-- Content-addressed artifact metadata (schema version 2)
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS artifacts (
    artifact_id TEXT PRIMARY KEY NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    byte_size INTEGER NOT NULL,
    media_type TEXT,
    relative_path TEXT,
    created_at_utc TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_artifacts_hash ON artifacts(content_hash);
