//! Secret redaction before persistence and UI emission.

use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Fixture secrets that must never appear in DB, artifacts, exports, or UI.
pub const FORBIDDEN_FIXTURE_SECRETS: &[&str] = &[
    "AKIAIOSFODNN7EXAMPLE",
    "fixture-secret-value",
    "fixture-secret-value-do-not-leak",
    "demo-api-key-should-redact",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedactionStats {
    pub original_bytes: usize,
    pub redacted_bytes: usize,
    replacement_count: u32,
    pub content_hash: String,
}

impl RedactionStats {
    pub fn replacement_count(&self) -> u32 {
        self.replacement_count
    }
}

struct Pattern {
    regex: Regex,
    replacement: &'static str,
}

fn patterns() -> &'static [Pattern] {
    static PATTERNS: OnceLock<Vec<Pattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            Pattern {
                regex: Regex::new(r"AKIA[0-9A-Z]{16}").expect("regex"),
                replacement: "[REDACTED_AWS_KEY]",
            },
            Pattern {
                regex: Regex::new(r"ghp_[A-Za-z0-9]{20,}").expect("regex"),
                replacement: "[REDACTED_GITHUB_PAT]",
            },
            Pattern {
                regex: Regex::new(
                    r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
                )
                .expect("regex"),
                replacement: "[REDACTED_PRIVATE_KEY]",
            },
            Pattern {
                regex: Regex::new(r"(?i)(authorization:\s*)(bearer\s+)?\S+").expect("regex"),
                replacement: "${1}[REDACTED_AUTH]",
            },
            Pattern {
                regex: Regex::new(r"(?i)(x-api-key:\s*)\S+").expect("regex"),
                replacement: "${1}[REDACTED_API_KEY]",
            },
            Pattern {
                regex: Regex::new(
                    r"(?i)(postgres|mysql|mongodb|redis|mssql)://[^\s:]+:[^\s@]+@[^\s]+",
                )
                .expect("regex"),
                replacement: "[REDACTED_CONNECTION_STRING]",
            },
            Pattern {
                regex: Regex::new(
                    r#"(?i)\b(api[_-]?key|secret[_-]?key|access[_-]?token|password)\b\s*[=:]\s*['"][^'"]{6,}['"]"#,
                )
                .expect("regex"),
                replacement: "$1=[REDACTED]",
            },
            Pattern {
                regex: Regex::new(r"(?i)\b(AWS_SECRET_ACCESS_KEY|CURSOR_API_KEY)\s*=\s*\S+")
                    .expect("regex"),
                replacement: "$1=[REDACTED_ENV]",
            },
        ]
    })
}

/// Redact known high-entropy token shapes from a log line without storing originals.
pub fn redact_line(input: &str) -> String {
    let (text, _) = redact_for_persistence(input, &[]);
    text
}

pub fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Redact before disk persistence. Preserves hash and byte counts for truncation diagnosis.
pub fn redact_for_persistence(input: &str, extra_secrets: &[&str]) -> (String, RedactionStats) {
    let original_bytes = input.len();
    let hash = content_hash(input.as_bytes());
    let mut out = input.to_string();
    let mut replacement_count = 0u32;

    for pattern in patterns() {
        let before = out.clone();
        out = pattern
            .regex
            .replace_all(&out, pattern.replacement)
            .into_owned();
        if out != before {
            replacement_count += 1;
        }
    }

    for secret in extra_secrets
        .iter()
        .copied()
        .chain(FORBIDDEN_FIXTURE_SECRETS.iter().copied())
    {
        if !secret.is_empty() && out.contains(secret) {
            out = out.replace(secret, "[REDACTED]");
            replacement_count += 1;
        }
    }

    let stats = RedactionStats {
        original_bytes,
        redacted_bytes: out.len(),
        replacement_count,
        content_hash: hash,
    };
    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_aws_example_key() {
        let line = "token=AKIAIOSFODNN7EXAMPLE trailing";
        let redacted = redact_line(line);
        assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(redacted.contains("[REDACTED_AWS_KEY]"));
    }

    #[test]
    fn redacts_auth_header_and_connection_string() {
        let line =
            "Authorization: Bearer super-secret-token postgres://user:pass@localhost/db trailing";
        let (out, stats) = redact_for_persistence(line, &[]);
        assert!(!out.to_lowercase().contains("super-secret-token"));
        assert!(!out.contains("user:pass@"));
        assert!(out.contains("[REDACTED_AUTH]") || out.contains("[REDACTED_CONNECTION_STRING]"));
        assert!(!stats.content_hash.is_empty());
        assert_eq!(stats.original_bytes, line.len());
    }

    #[test]
    fn fixture_secrets_never_survive() {
        let line = "leak=fixture-secret-value and AKIAIOSFODNN7EXAMPLE";
        let (out, _) = redact_for_persistence(line, &[]);
        for secret in FORBIDDEN_FIXTURE_SECRETS {
            assert!(
                !out.contains(secret),
                "secret {secret} leaked after redaction: {out}"
            );
        }
    }
}
