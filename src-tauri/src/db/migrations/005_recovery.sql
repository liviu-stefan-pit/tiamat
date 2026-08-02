-- P10 recovery: side-effect idempotency ledger, recovery offers, retention settings
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS side_effects (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    external_fact_json TEXT NOT NULL DEFAULT '{}',
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_side_effects_run_state
ON side_effects(run_id, state);

CREATE INDEX IF NOT EXISTS idx_side_effects_run_kind
ON side_effects(run_id, kind);

CREATE TABLE IF NOT EXISTS recovery_offers (
    run_id TEXT PRIMARY KEY NOT NULL REFERENCES runs(run_id),
    offer_id TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL,
    reason TEXT NOT NULL,
    db_integrity_ok INTEGER NOT NULL DEFAULT 1,
    process_hard_failure INTEGER NOT NULL DEFAULT 0,
    interrupted_attempt_count INTEGER NOT NULL DEFAULT 0,
    unreconciled_side_effects INTEGER NOT NULL DEFAULT 0,
    low_disk INTEGER NOT NULL DEFAULT 0,
    corrupt_db_backup_path TEXT,
    details_json TEXT NOT NULL DEFAULT '{}',
    created_at_utc TEXT NOT NULL,
    resolved_at_utc TEXT,
    resolution TEXT
);

CREATE TABLE IF NOT EXISTS retention_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    retain_run_metadata_days INTEGER NOT NULL DEFAULT 90,
    retain_redacted_logs_days INTEGER NOT NULL DEFAULT 30,
    retain_unpromoted_workspaces INTEGER NOT NULL DEFAULT 1,
    allow_destructive_cleanup INTEGER NOT NULL DEFAULT 0,
    updated_at_utc TEXT NOT NULL
);

INSERT OR IGNORE INTO retention_settings (
    id, retain_run_metadata_days, retain_redacted_logs_days,
    retain_unpromoted_workspaces, allow_destructive_cleanup, updated_at_utc
) VALUES (1, 90, 30, 1, 0, '1970-01-01T00:00:00Z');
