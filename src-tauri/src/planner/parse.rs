use serde_json::Value;

/// Locate the final JSON object in architect stream/assistant text.
pub fn extract_final_json_object(text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("architect output was empty".into());
    }

    // Prefer fenced ```json blocks (last one wins).
    if let Some(from_fence) = extract_from_fences(trimmed) {
        return Ok(from_fence);
    }

    // Otherwise take the last top-level JSON object substring.
    extract_last_object(trimmed)
}

fn extract_from_fences(text: &str) -> Option<String> {
    let mut last = None;
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let after = after
            .strip_prefix("json")
            .or_else(|| after.strip_prefix("JSON"))
            .unwrap_or(after);
        let after = after.strip_prefix('\n').unwrap_or(after);
        let Some(end) = after.find("```") else {
            break;
        };
        let candidate = after[..end].trim();
        if candidate.starts_with('{') && serde_json::from_str::<Value>(candidate).is_ok() {
            last = Some(candidate.to_string());
        }
        rest = &after[end + 3..];
    }
    last
}

fn extract_last_object(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut end = None;
    for (idx, ch) in bytes.iter().enumerate().rev() {
        if *ch == b'}' {
            end = Some(idx);
            break;
        }
    }
    let end = end.ok_or_else(|| "no JSON object closing brace found".to_string())?;

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut start = None;
    for idx in (0..=end).rev() {
        let ch = bytes[idx] as char;
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '}' => depth += 1,
            '{' => {
                depth -= 1;
                if depth == 0 {
                    start = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let start = start.ok_or_else(|| "no matching JSON object opening brace found".to_string())?;
    let candidate = &text[start..=end];
    serde_json::from_str::<Value>(candidate)
        .map_err(|e| format!("extracted JSON is invalid: {e}"))?;
    Ok(candidate.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_json() {
        let text = "thinking...\n```json\n{\"a\":1}\n```\n";
        let got = extract_final_json_object(text).unwrap();
        assert_eq!(got, "{\"a\":1}");
    }

    #[test]
    fn extracts_last_object_when_mixed_prose() {
        let text = "Here is the plan:\n{\"schemaVersion\":1,\"ok\":true}\nThanks.";
        let got = extract_final_json_object(text).unwrap();
        assert!(got.contains("\"schemaVersion\":1"));
    }

    #[test]
    fn prefers_last_fenced_block() {
        let text = "```json\n{\"v\":1}\n```\nmore\n```json\n{\"v\":2}\n```";
        let got = extract_final_json_object(text).unwrap();
        assert!(got.contains("\"v\":2"));
    }
}
