use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::Store;
use crate::packaging::types::{PackagingError, PackagingResult};
use crate::process::CleanupProof;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PackagedCleanupReport {
    pub run_id: Uuid,
    pub active_process_count: u32,
    pub zero_owned_processes: bool,
    pub proofs: Vec<CleanupProof>,
    pub artifact_path: Option<String>,
}

pub fn assert_zero_owned_processes(store: &Store, run_id: Option<Uuid>) -> PackagingResult<u32> {
    let count = store
        .active_process_count(run_id)
        .map_err(|e| PackagingError::Message(e.to_string()))?;
    if count != 0 {
        return Err(PackagingError::Message(format!(
            "owned process cleanup failed: {count} active process(es) remain"
        )));
    }
    Ok(count)
}

pub fn write_cleanup_proof_artifact(
    out_dir: &Path,
    report: &PackagedCleanupReport,
) -> PackagingResult<std::path::PathBuf> {
    fs::create_dir_all(out_dir)?;
    let path = out_dir.join(format!("cleanup-proof-{}.json", report.run_id));
    let body = serde_json::to_vec_pretty(report)?;
    fs::write(&path, body)?;
    let sidecar = out_dir.join(format!("cleanup-proof-{}.summary.json", report.run_id));
    fs::write(
        &sidecar,
        serde_json::to_vec_pretty(&json!({
            "zeroOwnedProcesses": report.zero_owned_processes,
            "activeProcessCount": report.active_process_count,
            "proofCount": report.proofs.len(),
        }))?,
    )?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_cleanup_proof_artifact() {
        let dir = tempdir().unwrap();
        let report = PackagedCleanupReport {
            run_id: Uuid::nil(),
            active_process_count: 0,
            zero_owned_processes: true,
            proofs: vec![],
            artifact_path: None,
        };
        let path = write_cleanup_proof_artifact(dir.path(), &report).unwrap();
        assert!(path.exists());
    }
}
