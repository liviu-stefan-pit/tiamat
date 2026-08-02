use rusqlite::params;

use crate::db::error::{DbError, DbResult};
use crate::db::store::Store;
use crate::packaging::AppSettings;

impl Store {
    pub fn get_app_settings(&self) -> DbResult<AppSettings> {
        self.conn()
            .query_row(
                "SELECT cursor_cli_path, canary_capability_hash, canary_consented_at_utc,
                        canary_last_success_at_utc, canary_last_version, updated_at_utc
                 FROM app_settings WHERE id = 1",
                [],
                |row| {
                    Ok(AppSettings {
                        cursor_cli_path: row.get(0)?,
                        canary_capability_hash: row.get(1)?,
                        canary_consented_at_utc: row.get(2)?,
                        canary_last_success_at_utc: row.get(3)?,
                        canary_last_version: row.get(4)?,
                        updated_at_utc: row.get(5)?,
                    })
                },
            )
            .map_err(DbError::from)
    }

    pub fn save_app_settings(&self, settings: &AppSettings) -> DbResult<()> {
        self.conn().execute(
            "UPDATE app_settings SET
                cursor_cli_path = ?1,
                canary_capability_hash = ?2,
                canary_consented_at_utc = ?3,
                canary_last_success_at_utc = ?4,
                canary_last_version = ?5,
                updated_at_utc = ?6
             WHERE id = 1",
            params![
                settings.cursor_cli_path,
                settings.canary_capability_hash,
                settings.canary_consented_at_utc,
                settings.canary_last_success_at_utc,
                settings.canary_last_version,
                settings.updated_at_utc,
            ],
        )?;
        Ok(())
    }

    pub fn set_cursor_cli_path(&self, path: Option<String>) -> DbResult<AppSettings> {
        let mut settings = self.get_app_settings()?;
        settings.cursor_cli_path = path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        settings.updated_at_utc = chrono::Utc::now().to_rfc3339();
        self.save_app_settings(&settings)?;
        Ok(settings)
    }

    pub fn list_cleanup_proofs(
        &self,
        run_id: uuid::Uuid,
    ) -> DbResult<Vec<crate::process::CleanupProof>> {
        let mut stmt = self.conn().prepare(
            "SELECT proof_id, run_id, process_id, observed_at_utc, active_process_count,
                    job_handle_open, handles_closed, zero_active_observed, success, detail_json
             FROM process_cleanup_proofs WHERE run_id = ?1
             ORDER BY observed_at_utc ASC",
        )?;
        let rows = stmt.query_map(params![run_id.to_string()], |row| {
            Ok(crate::process::CleanupProof {
                proof_id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?)
                    .unwrap_or_else(|_| uuid::Uuid::nil()),
                run_id: uuid::Uuid::parse_str(&row.get::<_, String>(1)?)
                    .unwrap_or_else(|_| uuid::Uuid::nil()),
                process_id: row
                    .get::<_, Option<String>>(2)?
                    .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
                observed_at_utc: row.get(3)?,
                active_process_count: row.get::<_, i64>(4)? as u32,
                job_handle_open: row.get::<_, i64>(5)? != 0,
                handles_closed: row.get::<_, i64>(6)? != 0,
                zero_active_observed: row.get::<_, i64>(7)? != 0,
                success: row.get::<_, i64>(8)? != 0,
                detail: serde_json::from_str(&row.get::<_, String>(9)?)
                    .unwrap_or_else(|_| serde_json::json!({})),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
