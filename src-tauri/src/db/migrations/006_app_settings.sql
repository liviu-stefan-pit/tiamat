-- P11: persisted app settings (configured Cursor CLI path + canary consent metadata)
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS app_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    cursor_cli_path TEXT,
    canary_capability_hash TEXT,
    canary_consented_at_utc TEXT,
    canary_last_success_at_utc TEXT,
    canary_last_version TEXT,
    updated_at_utc TEXT NOT NULL
);

INSERT OR IGNORE INTO app_settings (
    id, cursor_cli_path, canary_capability_hash, canary_consented_at_utc,
    canary_last_success_at_utc, canary_last_version, updated_at_utc
) VALUES (1, NULL, NULL, NULL, NULL, NULL, '1970-01-01T00:00:00Z');
