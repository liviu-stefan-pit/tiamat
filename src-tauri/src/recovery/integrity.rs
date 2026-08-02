//! DB integrity handling with corrupt-copy preservation.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::{current_version, latest_migration_version, migrate, DbError, DbResult, Store};
use crate::recovery::error::{RecoveryError, RecoveryResult};

pub struct IntegrityOpenResult {
    pub store: Option<Store>,
    pub ok: bool,
    pub backup_path: Option<PathBuf>,
    pub message: String,
}

/// Open a store; on integrity failure preserve a timestamped corrupt copy and do not guess state.
pub fn open_store_with_integrity_guard(
    db_path: impl AsRef<Path>,
    artifact_root: impl AsRef<Path>,
) -> RecoveryResult<IntegrityOpenResult> {
    let db_path = db_path.as_ref();
    let artifact_root = artifact_root.as_ref();
    match Store::open(db_path, artifact_root) {
        Ok(store) => Ok(IntegrityOpenResult {
            store: Some(store),
            ok: true,
            backup_path: None,
            message: "database integrity ok".into(),
        }),
        Err(DbError::Integrity(msg)) | Err(DbError::Migration(msg)) => {
            let backup = preserve_corrupt_copy(db_path)?;
            Ok(IntegrityOpenResult {
                store: None,
                ok: false,
                backup_path: Some(backup),
                message: format!("database integrity/migration failure: {msg}"),
            })
        }
        Err(other) => {
            // If the file is so corrupt that open itself fails, still try to preserve.
            if db_path.exists() {
                let backup = preserve_corrupt_copy(db_path)?;
                Ok(IntegrityOpenResult {
                    store: None,
                    ok: false,
                    backup_path: Some(backup),
                    message: format!("database open failure: {other}"),
                })
            } else {
                Err(RecoveryError::Db(other))
            }
        }
    }
}

/// Re-run PRAGMA integrity_check on an already-open store connection path.
pub fn verify_store_integrity(store: &Store) -> DbResult<()> {
    let result: String = store
        .conn()
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(DbError::Integrity(result));
    }
    let version = current_version(store.conn())?;
    if version != latest_migration_version() {
        return Err(DbError::Migration(format!(
            "schema version {version} != {}",
            latest_migration_version()
        )));
    }
    Ok(())
}

pub fn preserve_corrupt_copy(db_path: &Path) -> RecoveryResult<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = db_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tiamat.db");
    let backup = db_path.with_file_name(format!("{file_name}.corrupt.{stamp}"));
    fs::copy(db_path, &backup)?;
    // Also copy WAL/SHM if present.
    let wal = PathBuf::from(format!("{}-wal", db_path.display()));
    if wal.exists() {
        let _ = fs::copy(&wal, format!("{}-wal", backup.display()));
    }
    let shm = PathBuf::from(format!("{}-shm", db_path.display()));
    if shm.exists() {
        let _ = fs::copy(&shm, format!("{}-shm", backup.display()));
    }
    Ok(backup)
}

/// Build a deliberately malformed SQLite file for tests (not a valid DB).
pub fn write_malformed_db_fixture(path: &Path) -> RecoveryResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        b"NOT A SQLITE DATABASE - Tiamat P10 corrupt fixture\x00\x01\x02",
    )?;
    Ok(())
}

/// Create a valid DB then corrupt the header so integrity_check fails after open attempts.
pub fn write_header_corrupted_db_fixture(path: &Path) -> RecoveryResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    {
        let conn = Connection::open(path).map_err(DbError::from)?;
        migrate(&conn).map_err(RecoveryError::Db)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER); INSERT INTO t VALUES (1);")
            .map_err(DbError::from)?;
    }
    // Overwrite the SQLite magic header.
    let mut bytes = fs::read(path)?;
    if bytes.len() > 16 {
        bytes[..16].copy_from_slice(b"CORRUPT_HEADER!!");
        fs::write(path, bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn malformed_db_preserves_backup_and_blocks_open() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("tiamat.db");
        write_malformed_db_fixture(&db).unwrap();
        let result = open_store_with_integrity_guard(&db, dir.path().join("artifacts")).unwrap();
        assert!(!result.ok);
        assert!(result.store.is_none());
        assert!(result.backup_path.as_ref().unwrap().exists());
        assert!(result.message.contains("failure") || result.message.contains("integrity"));
    }

    #[test]
    fn healthy_open_succeeds() {
        let dir = tempdir().unwrap();
        let result =
            open_store_with_integrity_guard(dir.path().join("ok.db"), dir.path().join("a"))
                .unwrap();
        assert!(result.ok);
        assert!(result.store.is_some());
        verify_store_integrity(result.store.as_ref().unwrap()).unwrap();
    }
}
