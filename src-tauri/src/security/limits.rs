//! Resource and output limits.

use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_LINE_BYTES: usize = 64 * 1024;
pub const DEFAULT_MAX_TOTAL_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_MAX_PROMPT_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutputLimitConfig {
    pub max_line_bytes: usize,
    pub max_total_bytes: usize,
    pub max_prompt_bytes: usize,
}

impl Default for OutputLimitConfig {
    fn default() -> Self {
        Self {
            max_line_bytes: DEFAULT_MAX_LINE_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_OUTPUT_BYTES,
            max_prompt_bytes: DEFAULT_MAX_PROMPT_BYTES,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutputLimitResult {
    pub text: String,
    pub truncated: bool,
    pub original_bytes: usize,
    pub kept_bytes: usize,
    pub lines_dropped: u32,
    pub flood_detected: bool,
    pub message: Option<String>,
}

/// Apply line and total-byte caps. Floods fail visibly (truncated + flood flag).
pub fn apply_output_limits(input: &str, config: &OutputLimitConfig) -> OutputLimitResult {
    let original_bytes = input.len();
    let mut kept = String::new();
    let mut kept_bytes = 0usize;
    let mut lines_dropped = 0u32;
    let mut truncated = false;
    let mut flood_detected = false;

    for line in input.split_inclusive('\n') {
        let mut line_owned = line.to_string();
        if line_owned.len() > config.max_line_bytes {
            line_owned.truncate(config.max_line_bytes);
            line_owned.push_str("…[LINE_TRUNCATED]\n");
            truncated = true;
            flood_detected = true;
        }
        if kept_bytes + line_owned.len() > config.max_total_bytes {
            truncated = true;
            flood_detected = true;
            lines_dropped += 1;
            // Count remaining lines as dropped.
            lines_dropped += input[kept.len().min(input.len())..]
                .chars()
                .filter(|c| *c == '\n')
                .count() as u32;
            break;
        }
        kept_bytes += line_owned.len();
        kept.push_str(&line_owned);
    }

    let message = if flood_detected {
        Some(format!(
            "output flood/oversized stream truncated: kept {kept_bytes}/{original_bytes} bytes"
        ))
    } else if truncated {
        Some("output truncated to configured limits".into())
    } else {
        None
    };

    OutputLimitResult {
        text: kept,
        truncated,
        original_bytes,
        kept_bytes,
        lines_dropped,
        flood_detected,
        message,
    }
}

pub fn check_prompt_size(prompt: &str, config: &OutputLimitConfig) -> Result<(), String> {
    if prompt.len() > config.max_prompt_bytes {
        Err(format!(
            "prompt exceeds limit: {} > {} bytes",
            prompt.len(),
            config.max_prompt_bytes
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_oversized_line_and_marks_flood() {
        let huge = "x".repeat(DEFAULT_MAX_LINE_BYTES + 100);
        let result = apply_output_limits(&huge, &OutputLimitConfig::default());
        assert!(result.truncated);
        assert!(result.flood_detected);
        assert!(result.kept_bytes <= DEFAULT_MAX_LINE_BYTES + 32);
        assert!(result.text.contains("LINE_TRUNCATED"));
    }

    #[test]
    fn caps_total_bytes() {
        let config = OutputLimitConfig {
            max_total_bytes: 100,
            max_line_bytes: 50,
            ..Default::default()
        };
        let input = "abcdefghij\n".repeat(30);
        let result = apply_output_limits(&input, &config);
        assert!(result.truncated);
        assert!(result.kept_bytes <= 100 + 20);
        assert!(result.flood_detected);
    }
}
