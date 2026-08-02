use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::intake::error::{IntakeError, IntakeResult};
use crate::intake::ignore::{is_ignored_dir_name, is_ignored_file_name};
use crate::intake::limits::IntakeLimits;
use crate::intake::paths::{
    assert_within_approved_roots, canonicalize_path, strip_verbatim_prefix,
};
use crate::intake::secrets::{scan_file_for_secret_risks, SecretRiskFinding};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InventoryEntry {
    pub relative_path: String,
    pub absolute_path: String,
    pub byte_size: u64,
    pub is_dir: bool,
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InventoryReport {
    pub entries: Vec<InventoryEntry>,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_bytes: u64,
    pub ignored_count: u64,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
    pub escape_attempts: Vec<String>,
    pub secret_risks: Vec<SecretRiskFinding>,
    pub content_hash: String,
}

pub fn inventory_roots(roots: &[PathBuf], limits: &IntakeLimits) -> IntakeResult<InventoryReport> {
    let mut approved = Vec::new();
    for root in roots {
        let canonical = canonicalize_path(&root.to_string_lossy())?;
        approved.push(canonical.final_path);
    }

    let mut entries = Vec::new();
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    let mut total_bytes = 0u64;
    let mut ignored_count = 0u64;
    let mut truncated = false;
    let mut truncation_reason = None;
    let mut escape_attempts = Vec::new();
    let mut secret_risks = Vec::new();

    for root in &approved {
        let root_label = root.clone();
        walk_one(
            root,
            &root_label,
            &approved,
            limits,
            0,
            &mut entries,
            &mut file_count,
            &mut dir_count,
            &mut total_bytes,
            &mut ignored_count,
            &mut truncated,
            &mut truncation_reason,
            &mut escape_attempts,
            &mut secret_risks,
        )?;
        if truncated {
            break;
        }
    }

    if file_count > limits.max_files {
        return Err(IntakeError::LimitsExceeded(format!(
            "file count {file_count} exceeds limit {}",
            limits.max_files
        )));
    }
    if total_bytes > limits.max_total_bytes {
        return Err(IntakeError::LimitsExceeded(format!(
            "total bytes {total_bytes} exceed limit {}",
            limits.max_total_bytes
        )));
    }

    let content_hash = hash_inventory(&entries, &secret_risks);
    Ok(InventoryReport {
        entries,
        file_count,
        dir_count,
        total_bytes,
        ignored_count,
        truncated,
        truncation_reason,
        escape_attempts,
        secret_risks,
        content_hash,
    })
}

#[allow(clippy::too_many_arguments)]
fn walk_one(
    current: &Path,
    root: &Path,
    approved: &[PathBuf],
    limits: &IntakeLimits,
    depth: u32,
    entries: &mut Vec<InventoryEntry>,
    file_count: &mut u64,
    dir_count: &mut u64,
    total_bytes: &mut u64,
    ignored_count: &mut u64,
    truncated: &mut bool,
    truncation_reason: &mut Option<String>,
    escape_attempts: &mut Vec<String>,
    secret_risks: &mut Vec<SecretRiskFinding>,
) -> IntakeResult<()> {
    if *truncated {
        return Ok(());
    }
    if depth > limits.max_depth {
        *truncated = true;
        *truncation_reason = Some(format!("max depth {} exceeded", limits.max_depth));
        return Ok(());
    }

    let meta = match fs::symlink_metadata(current) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };

    // Resolve reparse points and reject escapes.
    if meta.file_type().is_symlink() {
        match fs::canonicalize(current) {
            Ok(resolved) => {
                let resolved = strip_verbatim_prefix(resolved);
                if let Err(err) = assert_within_approved_roots(approved, &resolved) {
                    escape_attempts.push(format!("{} -> {}", current.display(), err));
                    *ignored_count += 1;
                    entries.push(InventoryEntry {
                        relative_path: rel(root, current),
                        absolute_path: current.display().to_string(),
                        byte_size: 0,
                        is_dir: false,
                        skipped_reason: Some("symlink_escape".into()),
                    });
                    return Ok(());
                }
            }
            Err(_) => {
                escape_attempts.push(format!(
                    "{} could not be resolved safely",
                    current.display()
                ));
                *ignored_count += 1;
                return Ok(());
            }
        }
    }

    let relative = rel(root, current);
    if meta.is_dir() {
        *dir_count += 1;
        entries.push(InventoryEntry {
            relative_path: relative.clone(),
            absolute_path: current.display().to_string(),
            byte_size: 0,
            is_dir: true,
            skipped_reason: None,
        });

        let read = match fs::read_dir(current) {
            Ok(r) => r,
            Err(_) => return Ok(()),
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let child = entry.path();
            let child_meta = match fs::symlink_metadata(&child) {
                Ok(m) => m,
                Err(_) => continue,
            };

            if child_meta.is_dir() || child_meta.file_type().is_symlink() {
                if is_ignored_dir_name(&name) && !child_meta.file_type().is_symlink() {
                    *ignored_count += 1;
                    entries.push(InventoryEntry {
                        relative_path: rel(root, &child),
                        absolute_path: child.display().to_string(),
                        byte_size: 0,
                        is_dir: true,
                        skipped_reason: Some("ignored_directory".into()),
                    });
                    continue;
                }
                // Symlinks masquerading as ignored names still get escape checks.
                walk_one(
                    &child,
                    root,
                    approved,
                    limits,
                    depth + 1,
                    entries,
                    file_count,
                    dir_count,
                    total_bytes,
                    ignored_count,
                    truncated,
                    truncation_reason,
                    escape_attempts,
                    secret_risks,
                )?;
                if *truncated {
                    return Ok(());
                }
                continue;
            }

            if is_ignored_file_name(&name) {
                *ignored_count += 1;
                continue;
            }

            let size = child_meta.len();
            if size > limits.max_file_bytes {
                *ignored_count += 1;
                entries.push(InventoryEntry {
                    relative_path: rel(root, &child),
                    absolute_path: child.display().to_string(),
                    byte_size: size,
                    is_dir: false,
                    skipped_reason: Some("file_too_large".into()),
                });
                continue;
            }

            if *file_count + 1 > limits.max_files {
                *truncated = true;
                *truncation_reason = Some(format!(
                    "file count would exceed limit {}",
                    limits.max_files
                ));
                return Err(IntakeError::LimitsExceeded(
                    truncation_reason.clone().unwrap_or_default(),
                ));
            }
            if *total_bytes + size > limits.max_total_bytes {
                *truncated = true;
                *truncation_reason = Some(format!(
                    "total bytes would exceed limit {}",
                    limits.max_total_bytes
                ));
                return Err(IntakeError::LimitsExceeded(
                    truncation_reason.clone().unwrap_or_default(),
                ));
            }

            *file_count += 1;
            *total_bytes += size;
            let child_rel = rel(root, &child);
            entries.push(InventoryEntry {
                relative_path: child_rel.clone(),
                absolute_path: child.display().to_string(),
                byte_size: size,
                is_dir: false,
                skipped_reason: None,
            });
            secret_risks.extend(scan_file_for_secret_risks(&child, &child_rel, limits));
        }
        return Ok(());
    }

    // Single-file root
    let size = meta.len();
    if size > limits.max_file_bytes {
        return Err(IntakeError::LimitsExceeded(format!(
            "file {} exceeds max file size",
            current.display()
        )));
    }
    *file_count += 1;
    *total_bytes += size;
    entries.push(InventoryEntry {
        relative_path: relative.clone(),
        absolute_path: current.display().to_string(),
        byte_size: size,
        is_dir: false,
        skipped_reason: None,
    });
    secret_risks.extend(scan_file_for_secret_risks(current, &relative, limits));
    Ok(())
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| {
            if p.as_os_str().is_empty() {
                ".".into()
            } else {
                p.display().to_string()
            }
        })
        .unwrap_or_else(|_| path.display().to_string())
}

fn hash_inventory(entries: &[InventoryEntry], secrets: &[SecretRiskFinding]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.relative_path.as_bytes());
        hasher.update(entry.byte_size.to_le_bytes());
    }
    for secret in secrets {
        hasher.update(secret.pattern_id.as_bytes());
        hasher.update(secret.match_hash.as_bytes());
    }
    hex::encode(hasher.finalize())
}
