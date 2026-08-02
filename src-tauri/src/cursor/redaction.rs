use crate::security::redact_line;

const SECRET_FLAGS: &[&str] = &["--api-key"];
const REDACTION_TOKEN: &str = "***";

/// Copy argv with secret flag values replaced.
pub fn redact_argv(argv: &[String]) -> Vec<String> {
    let mut redacted = Vec::with_capacity(argv.len());
    let mut hide_next = false;
    for item in argv {
        if hide_next {
            redacted.push(REDACTION_TOKEN.to_string());
            hide_next = false;
            continue;
        }
        if SECRET_FLAGS.iter().any(|flag| item == *flag) {
            redacted.push(item.clone());
            hide_next = true;
            continue;
        }
        let mut matched = false;
        for flag in SECRET_FLAGS {
            let prefix = format!("{flag}=");
            if let Some(rest) = item.strip_prefix(&prefix) {
                if !rest.is_empty() {
                    redacted.push(format!("{prefix}{REDACTION_TOKEN}"));
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            redacted.push(item.clone());
        }
    }
    redacted
}

pub fn redact_text_secrets(text: &str, secrets: &[&str]) -> String {
    let mut result = redact_line(text);
    for secret in secrets {
        if !secret.is_empty() {
            result = result.replace(secret, REDACTION_TOKEN);
        }
    }
    result
}

/// Quote argv for Windows display only — never used for execution.
pub fn quote_windows_command(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| quote_windows_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn quote_windows_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    if !arg.chars().any(|c| c.is_whitespace() || c == '"') {
        return arg.to_string();
    }

    let mut result = String::from("\"");
    let mut num_backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => num_backslashes += 1,
            '"' => {
                result.push_str(&"\\".repeat(num_backslashes * 2 + 1));
                result.push('"');
                num_backslashes = 0;
            }
            _ => {
                if num_backslashes > 0 {
                    result.push_str(&"\\".repeat(num_backslashes));
                    num_backslashes = 0;
                }
                result.push(ch);
            }
        }
    }
    if num_backslashes > 0 {
        result.push_str(&"\\".repeat(num_backslashes * 2));
    }
    result.push('"');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_api_key_flag_and_equals_form() {
        let argv = vec![
            "agent".into(),
            "--api-key".into(),
            "super-secret".into(),
            "--api-key=also-secret".into(),
            "--model".into(),
            "composer-2.5".into(),
        ];
        let redacted = redact_argv(&argv);
        assert_eq!(redacted[2], "***");
        assert_eq!(redacted[3], "--api-key=***");
        assert!(!redacted.join(" ").contains("super-secret"));
        assert!(!redacted.join(" ").contains("also-secret"));
    }

    #[test]
    fn redacts_known_secret_substrings_and_token_shapes() {
        let text = "key=AKIAIOSFODNN7EXAMPLE and secret=fixture-secret-value";
        let out = redact_text_secrets(text, &["fixture-secret-value"]);
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!out.contains("fixture-secret-value"));
        assert!(out.contains("[REDACTED_AWS_KEY]") || out.contains("***"));
    }

    #[test]
    fn quotes_spaces_for_display_only() {
        assert_eq!(quote_windows_arg("plain"), "plain");
        assert_eq!(quote_windows_arg("a b"), "\"a b\"");
    }
}
