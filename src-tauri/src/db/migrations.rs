use crate::db::error::{DbError, DbResult};
use rusqlite::{Connection, OptionalExtension};

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "001_initial",
        sql: include_str!("migrations/001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "002_artifacts",
        sql: include_str!("migrations/002_artifacts.sql"),
    },
    Migration {
        version: 3,
        name: "003_scheduler",
        sql: include_str!("migrations/003_scheduler.sql"),
    },
    Migration {
        version: 4,
        name: "004_processes",
        sql: include_str!("migrations/004_processes.sql"),
    },
    Migration {
        version: 5,
        name: "005_recovery",
        sql: include_str!("migrations/005_recovery.sql"),
    },
    Migration {
        version: 6,
        name: "006_app_settings",
        sql: include_str!("migrations/006_app_settings.sql"),
    },
];

pub fn latest_migration_version() -> i64 {
    MIGRATIONS.last().map(|m| m.version).unwrap_or(0)
}

pub fn migrate(conn: &Connection) -> DbResult<i64> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at_utc TEXT NOT NULL
        );",
    )?;

    let current = current_version(conn)?;
    for migration in MIGRATIONS {
        if migration.version <= current {
            continue;
        }
        apply_migration(conn, migration)?;
    }

    let after = current_version(conn)?;
    if after != latest_migration_version() {
        return Err(DbError::Migration(format!(
            "expected schema version {}, found {}",
            latest_migration_version(),
            after
        )));
    }
    Ok(after)
}

pub fn current_version(conn: &Connection) -> DbResult<i64> {
    let version: Option<i64> = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(version.unwrap_or(0))
}

fn apply_migration(conn: &Connection, migration: &Migration) -> DbResult<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(migration.sql)?;
    let applied_at = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO schema_migrations (version, name, applied_at_utc) VALUES (?1, ?2, ?3)",
        rusqlite::params![migration.version, migration.name, applied_at],
    )?;
    tx.commit()?;
    Ok(())
}

/// Apply only migrations up to `max_version` (inclusive). Used to build prior-version fixtures.
pub fn migrate_up_to(conn: &Connection, max_version: i64) -> DbResult<i64> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at_utc TEXT NOT NULL
        );",
    )?;

    let current = current_version(conn)?;
    for migration in MIGRATIONS {
        if migration.version <= current || migration.version > max_version {
            continue;
        }
        apply_migration(conn, migration)?;
    }
    current_version(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrate_empty_database_to_latest() {
        let conn = Connection::open_in_memory().unwrap();
        let version = migrate(&conn).unwrap();
        assert_eq!(version, latest_migration_version());
        assert_eq!(current_version(&conn).unwrap(), latest_migration_version());
    }

    #[test]
    fn migrate_from_every_prior_version() {
        let latest = latest_migration_version();
        assert!(latest >= 1, "expected at least one migration");

        // From empty (version 0) and every applied prior version 1..latest-1.
        for prior in 0..latest {
            let conn = Connection::open_in_memory().unwrap();
            let applied = if prior == 0 {
                0
            } else {
                migrate_up_to(&conn, prior).unwrap()
            };
            assert_eq!(applied, prior, "fixture stop at {prior}");

            let version = migrate(&conn).unwrap();
            assert_eq!(
                version, latest,
                "migrate from prior={prior} must reach latest={latest}"
            );
            assert_eq!(current_version(&conn).unwrap(), latest);

            // Spot-check tables introduced by later migrations still exist.
            for table in [
                "runs",
                "events",
                "artifacts",
                "scheduler_leases",
                "processes",
                "app_settings",
            ] {
                if table_expected_at_latest(table, latest) {
                    let exists: bool = conn
                        .query_row(
                            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                            [table],
                            |row| row.get::<_, i64>(0).map(|n| n > 0),
                        )
                        .unwrap();
                    assert!(exists, "missing table {table} after migrate from {prior}");
                }
            }
        }
    }

    fn table_expected_at_latest(table: &str, latest: i64) -> bool {
        match table {
            "runs" | "events" => latest >= 1,
            "artifacts" => latest >= 2,
            "scheduler_leases" => latest >= 3,
            "processes" => latest >= 4,
            "app_settings" => latest >= 6,
            _ => false,
        }
    }

    #[test]
    fn migrate_from_prior_version_fixture() {
        let conn = Connection::open_in_memory().unwrap();
        let prior = migrate_up_to(&conn, 1).unwrap();
        assert_eq!(prior, 1);

        let has_artifacts: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='artifacts'",
                [],
                |row| row.get::<_, i64>(0).map(|n| n > 0),
            )
            .unwrap();
        assert!(!has_artifacts);

        let version = migrate(&conn).unwrap();
        assert_eq!(version, latest_migration_version());

        let has_artifacts: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='artifacts'",
                [],
                |row| row.get::<_, i64>(0).map(|n| n > 0),
            )
            .unwrap();
        assert!(has_artifacts);
    }
}
