use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceFingerprint {
    pub path: String,
    pub kind: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub status_porcelain: String,
    pub status_hash: String,
    pub tree_hash: String,
    pub captured_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintPair {
    pub before: SourceFingerprint,
    pub after: SourceFingerprint,
    pub unchanged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ManagedProjectKind {
    GitClone,
    NonGitCopy,
    NotesSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirtyOverlayMetadata {
    pub source_head: String,
    pub had_staged: bool,
    pub had_unstaged: bool,
    pub had_untracked: bool,
    pub staged_patch_bytes: u64,
    pub unstaged_patch_bytes: u64,
    pub untracked_files: Vec<String>,
    pub overlay_artifact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedProject {
    pub project_id: String,
    pub source_root: String,
    pub managed_root: String,
    pub kind: ManagedProjectKind,
    pub baseline_commit: Option<String>,
    pub baseline_branch: String,
    pub worktree_path: Option<String>,
    pub write_root: String,
    pub read_roots: Vec<String>,
    pub dirty_overlay: Option<DirtyOverlayMetadata>,
    pub source_fingerprint: SourceFingerprint,
    pub lock_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointRecord {
    pub checkpoint_id: String,
    pub project_id: String,
    pub commit: String,
    pub branch: String,
    pub message: String,
    pub created_at_utc: DateTime<Utc>,
    pub parent_checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineRecord {
    pub quarantine_id: String,
    pub project_id: String,
    pub reason: String,
    pub source_path: String,
    pub quarantine_path: String,
    pub created_at_utc: DateTime<Utc>,
    pub from_checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PromotionStatus {
    Unpromoted,
    Exported,
    Promoted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromotionMetadata {
    pub status: PromotionStatus,
    pub export_path: Option<String>,
    pub promoted_at_utc: Option<DateTime<Utc>>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicy {
    pub retain_unpromoted: bool,
    pub max_quarantine_entries: u32,
    pub allow_destructive_cleanup: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            retain_unpromoted: true,
            max_quarantine_entries: 32,
            allow_destructive_cleanup: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunWorkspaceManifest {
    pub schema_version: u32,
    pub run_id: Uuid,
    pub intake_id: Uuid,
    pub managed_run_root: String,
    pub control_root: String,
    pub projects: Vec<ManagedProject>,
    pub notes_roots: Vec<String>,
    pub checkpoints: Vec<CheckpointRecord>,
    pub quarantines: Vec<QuarantineRecord>,
    pub promotion: PromotionMetadata,
    pub retention: RetentionPolicy,
    pub fingerprint_pairs: Vec<FingerprintPair>,
    pub created_at_utc: DateTime<Utc>,
    pub source_unchanged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RootValidationResult {
    pub ok: bool,
    pub write_errors: Vec<String>,
    pub read_errors: Vec<String>,
}

impl RunWorkspaceManifest {
    pub fn validate_write_root(&self, candidate: &str) -> Result<(), String> {
        // Notes snapshots are read-only intake; only NonGit/Git projects are writable.
        let ok = self.projects.iter().any(|p| {
            !matches!(p.kind, ManagedProjectKind::NotesSnapshot)
                && (crate::workspace::roots::is_within_managed(&p.write_root, candidate)
                    || crate::workspace::roots::is_within_managed(&p.managed_root, candidate))
        });
        if ok {
            Ok(())
        } else if self
            .notes_roots
            .iter()
            .any(|n| crate::workspace::roots::is_within_managed(n, candidate) || n == candidate)
            || self.projects.iter().any(|p| {
                matches!(p.kind, ManagedProjectKind::NotesSnapshot)
                    && (crate::workspace::roots::is_within_managed(&p.write_root, candidate)
                        || crate::workspace::roots::is_within_managed(&p.managed_root, candidate)
                        || p.write_root == candidate
                        || p.managed_root == candidate)
            })
        {
            Err(format!(
                "notes roots are read-only; cannot be writeRoots: {candidate}"
            ))
        } else {
            Err(format!("write root not in managed projects: {candidate}"))
        }
    }

    pub fn validate_read_root(&self, candidate: &str) -> Result<(), String> {
        if crate::workspace::roots::is_within_managed(&self.managed_run_root, candidate) {
            return Ok(());
        }
        for project in &self.projects {
            if project
                .read_roots
                .iter()
                .any(|r| crate::workspace::roots::is_within_managed(r, candidate) || r == candidate)
                || crate::workspace::roots::is_within_managed(&project.managed_root, candidate)
                || crate::workspace::roots::is_within_managed(&project.source_root, candidate)
                || project.source_root == candidate
            {
                return Ok(());
            }
        }
        for notes in &self.notes_roots {
            if crate::workspace::roots::is_within_managed(notes, candidate) || notes == candidate {
                return Ok(());
            }
        }
        Err(format!("read root not approved: {candidate}"))
    }

    pub fn has_unpromoted_work(&self) -> bool {
        !matches!(
            self.promotion.status,
            PromotionStatus::Promoted | PromotionStatus::Exported
        ) && !self.projects.is_empty()
    }
}
