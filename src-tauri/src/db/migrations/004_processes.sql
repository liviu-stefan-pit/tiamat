-- P07 durable process registry, cleanup evidence, abort preference
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS processes (
    process_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    phase_id TEXT,
    attempt_id TEXT,
    executable TEXT NOT NULL,
    args_redacted TEXT NOT NULL DEFAULT '[]',
    pid INTEGER,
    creation_time_100ns INTEGER,
    executable_identity TEXT,
    job_name TEXT,
    job_associated INTEGER NOT NULL DEFAULT 0,
    parent_pid INTEGER,
    workspace TEXT,
    state TEXT NOT NULL,
    heartbeat_at_utc TEXT,
    registered_at_utc TEXT NOT NULL,
    spawned_at_utc TEXT,
    stopped_at_utc TEXT,
    reaped_at_utc TEXT,
    exit_code INTEGER,
    terminal_reason TEXT,
    chat_id TEXT,
    resume_metadata_json TEXT NOT NULL DEFAULT '{}',
    cleanup_evidence_json TEXT NOT NULL DEFAULT '{}',
    metadata_json TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (attempt_id) REFERENCES attempts(attempt_id)
);

CREATE INDEX IF NOT EXISTS idx_processes_run_state ON processes(run_id, state);
CREATE INDEX IF NOT EXISTS idx_processes_attempt ON processes(attempt_id);
CREATE INDEX IF NOT EXISTS idx_processes_active
ON processes(run_id)
WHERE state NOT IN ('reaped');

CREATE TABLE IF NOT EXISTS process_cleanup_proofs (
    proof_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    process_id TEXT REFERENCES processes(process_id),
    observed_at_utc TEXT NOT NULL,
    active_process_count INTEGER NOT NULL,
    job_handle_open INTEGER NOT NULL,
    handles_closed INTEGER NOT NULL DEFAULT 0,
    zero_active_observed INTEGER NOT NULL DEFAULT 0,
    success INTEGER NOT NULL DEFAULT 0,
    detail_json TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_cleanup_proofs_run ON process_cleanup_proofs(run_id, observed_at_utc);

CREATE TABLE IF NOT EXISTS abort_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    shortcut TEXT NOT NULL DEFAULT 'Ctrl+Shift+F12',
    registered INTEGER NOT NULL DEFAULT 0,
    degraded INTEGER NOT NULL DEFAULT 0,
    collision_reason TEXT,
    degraded_acknowledged INTEGER NOT NULL DEFAULT 0,
    tray_fallback_enabled INTEGER NOT NULL DEFAULT 1,
    second_press_force_ms INTEGER NOT NULL DEFAULT 3000,
    updated_at_utc TEXT NOT NULL
);

INSERT OR IGNORE INTO abort_settings (
    id, shortcut, registered, degraded, collision_reason,
    degraded_acknowledged, tray_fallback_enabled, second_press_force_ms, updated_at_utc
) VALUES (
    1, 'Ctrl+Shift+F12', 0, 0, NULL, 0, 1, 3000, '1970-01-01T00:00:00Z'
);
