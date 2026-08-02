-- P06 durable scheduler: phases, attempts, leases, resource locks
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS phases (
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    phase_id TEXT NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    project_ids_json TEXT NOT NULL DEFAULT '[]',
    write_roots_json TEXT NOT NULL DEFAULT '[]',
    resource_locks_json TEXT NOT NULL DEFAULT '[]',
    dependencies_json TEXT NOT NULL DEFAULT '[]',
    model_tier TEXT NOT NULL,
    estimated_minutes INTEGER NOT NULL DEFAULT 10,
    critical_path_length INTEGER NOT NULL DEFAULT 0,
    ready_at_utc TEXT,
    queued_at_utc TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_failure_kind TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (run_id, phase_id)
);

CREATE INDEX IF NOT EXISTS idx_phases_run_status ON phases(run_id, status);

CREATE TABLE IF NOT EXISTS attempts (
    attempt_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    phase_id TEXT NOT NULL,
    attempt_number INTEGER NOT NULL,
    status TEXT NOT NULL,
    terminal_result TEXT,
    requested_tier TEXT NOT NULL,
    requested_model TEXT NOT NULL,
    selected_model TEXT NOT NULL,
    selection_reason TEXT NOT NULL,
    availability_json TEXT NOT NULL DEFAULT '[]',
    resume_parent_attempt_id TEXT,
    progress_useful INTEGER NOT NULL DEFAULT 0,
    failure_kind TEXT,
    started_at_utc TEXT,
    finished_at_utc TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE (run_id, phase_id, attempt_number),
    FOREIGN KEY (run_id, phase_id) REFERENCES phases(run_id, phase_id)
);

-- At most one non-terminal attempt per phase (restart idempotency).
CREATE UNIQUE INDEX IF NOT EXISTS idx_attempts_one_active
ON attempts(run_id, phase_id)
WHERE status IN ('starting', 'running', 'stopping');

CREATE INDEX IF NOT EXISTS idx_attempts_run_phase ON attempts(run_id, phase_id, attempt_number);

CREATE TABLE IF NOT EXISTS scheduler_leases (
    run_id TEXT PRIMARY KEY NOT NULL REFERENCES runs(run_id),
    lease_holder TEXT NOT NULL,
    epoch INTEGER NOT NULL DEFAULT 0,
    renewed_at_utc TEXT NOT NULL,
    expires_at_utc TEXT NOT NULL,
    paused INTEGER NOT NULL DEFAULT 0,
    max_concurrent INTEGER NOT NULL DEFAULT 3,
    cleanup_incomplete INTEGER NOT NULL DEFAULT 0,
    low_disk INTEGER NOT NULL DEFAULT 0,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS resource_locks (
    lock_name TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    phase_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    acquired_at_utc TEXT NOT NULL,
    PRIMARY KEY (lock_name),
    FOREIGN KEY (attempt_id) REFERENCES attempts(attempt_id)
);

CREATE INDEX IF NOT EXISTS idx_resource_locks_run ON resource_locks(run_id, phase_id);
