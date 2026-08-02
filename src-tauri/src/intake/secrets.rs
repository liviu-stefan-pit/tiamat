use std::fs;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

use crate::intake::limits::IntakeLimits;

/// Secret-risk metadata only — never stores matched secret values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecretRiskFinding {
    pub relative_path: String,
    pub pattern_id: String,
    pub match_hash: String,
    pub match_byte_len: usize,
}

#[derive(Debug, Clone)]
struct PatternDef {
    id: &'static str,
    regex: Regex,
}

fn patterns() -> &'static [PatternDef] {
    static PATTERNS: OnceLock<Vec<PatternDef>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            PatternDef {
                id: "aws_access_key_id",
                regex: Regex::new(r"(?i)\bAKIA[0-9A-Z]{16}\b").expect("regex"),
            },
            PatternDef {
                id: "generic_api_key_assignment",
                regex: Regex::new(
                    r#"(?i)\b(api[_-]?key|secret[_-]?key|access[_-]?token)\b\s*[=:]\s*['"][^'"]{8,}['"]"#,
                )
                .expect("regex"),
            },
            PatternDef {
                id: "pem_private_key_header",
                regex: Regex::new(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----")
                    .expect("regex"),
            },
            PatternDef {
                id: "connection_string_password",
                regex: Regex::new(r"(?i)(postgres|mysql|mongodb|redis)://[^\s:]+:[^\s@]+@")
                    .expect("regex"),
            },
            PatternDef {
                id: "github_pat",
                regex: Regex::new(r"\bghp_[A-Za-z0-9]{20,}\b").expect("regex"),
            },
        ]
    })
}

const RISKY_NAME_HINTS: &[&str] = &[
    ".env",
    ".env.local",
    ".env.production",
    "credentials",
    "secrets",
    "id_rsa",
    "id_ed25519",
    ".pem",
    ".p12",
    ".pfx",
    "service-account",
];

pub fn filename_risk_hint(file_name: &str) -> Option<&'static str> {
    let lower = file_name.to_ascii_lowercase();
    for hint in RISKY_NAME_HINTS {
        if lower == *hint || lower.ends_with(hint) || lower.contains(hint) {
            return Some("risky_filename");
        }
    }
    None
}

pub fn scan_file_for_secret_risks(
    absolute: &Path,
    relative: &str,
    limits: &IntakeLimits,
) -> Vec<SecretRiskFinding> {
    let mut findings = Vec::new();
    if let Some(pattern_id) = filename_risk_hint(
        absolute
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default(),
    ) {
        findings.push(SecretRiskFinding {
            relative_path: relative.to_string(),
            pattern_id: pattern_id.to_string(),
            match_hash: hash_bytes(relative.as_bytes()),
            match_byte_len: 0,
        });
    }

    let Ok(meta) = fs::metadata(absolute) else {
        return findings;
    };
    if !meta.is_file() || meta.len() > limits.max_secret_scan_bytes {
        return findings;
    }

    let Ok(bytes) = fs::read(absolute) else {
        return findings;
    };
    // Only scan text-ish content.
    if bytes.iter().filter(|b| **b == 0).count() > 0 {
        return findings;
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return findings;
    };

    for pattern in patterns() {
        if let Some(mat) = pattern.regex.find(text) {
            findings.push(SecretRiskFinding {
                relative_path: relative.to_string(),
                pattern_id: pattern.id.to_string(),
                match_hash: hash_bytes(mat.as_str().as_bytes()),
                match_byte_len: mat.as_str().len(),
            });
        }
    }

    findings
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Assert a redacted payload/message never contains raw secret fixture values.
pub fn assert_no_secret_leak(haystack: &str, forbidden: &[&str]) -> Result<(), String> {
    for value in forbidden {
        if !value.is_empty() && haystack.contains(value) {
            return Err("secret value leaked into output".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn detects_aws_key_without_storing_value() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.txt");
        let secret = "AKIAIOSFODNN7EXAMPLE";
        let mut file = fs::File::create(&path).unwrap();
        writeln!(file, "key={secret}").unwrap();

        let findings = scan_file_for_secret_risks(&path, "config.txt", &IntakeLimits::default());
        assert!(findings.iter().any(|f| f.pattern_id == "aws_access_key_id"));
        let serialized = serde_json::to_string(&findings).unwrap();
        assert!(!serialized.contains(secret));
        assert!(assert_no_secret_leak(&serialized, &[secret]).is_ok());
    }

    #[test]
    fn risky_filename_emits_metadata_only() {
        assert_eq!(filename_risk_hint(".env"), Some("risky_filename"));
        assert_eq!(filename_risk_hint("readme.md"), None);
    }
}
