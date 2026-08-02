use serde_json::Value;

use super::redaction::redact_text_secrets;
use super::types::{CursorUsage, ParsedStreamEvent, StreamEventKind, StreamParseResult};
use crate::security::redact_line;

/// Parse Cursor `stream-json` / JSONL stdout, preserving malformed lines as diagnostics.
pub fn parse_stream_json(stdout: &str, stderr: &str, extra_secrets: &[&str]) -> StreamParseResult {
    let mut result = StreamParseResult::default();

    for line in stdout.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.trim().is_empty() {
            continue;
        }
        let event = parse_stream_line(trimmed, extra_secrets);
        absorb_event(&mut result, event);
    }

    for line in stderr.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.trim().is_empty() {
            continue;
        }
        let redacted = redact_text_secrets(trimmed, extra_secrets);
        result.diagnostics.push(redacted.clone());
        result.events.push(ParsedStreamEvent {
            kind: StreamEventKind::Diagnostic,
            raw_line: trimmed.to_string(),
            redacted_line: redacted,
            chat_id: None,
            text: None,
            usage: None,
            ok: true,
            parse_error: None,
        });
    }

    result
}

pub fn parse_stream_line(line: &str, extra_secrets: &[&str]) -> ParsedStreamEvent {
    let redacted_line = redact_text_secrets(&redact_line(line), extra_secrets);
    match serde_json::from_str::<Value>(line) {
        Ok(value) => normalize_json_event(line, &redacted_line, &value),
        Err(err) => ParsedStreamEvent {
            kind: StreamEventKind::Diagnostic,
            raw_line: line.to_string(),
            redacted_line,
            chat_id: None,
            text: Some(line.to_string()),
            usage: None,
            ok: false,
            parse_error: Some(err.to_string()),
        },
    }
}

fn normalize_json_event(raw: &str, redacted: &str, value: &Value) -> ParsedStreamEvent {
    let type_name = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let chat_id = extract_chat_id(value);
    let usage = extract_usage(value);
    let text = extract_text(value);

    let (kind, ok) = match type_name.as_str() {
        "system" => (StreamEventKind::System, true),
        "assistant" | "message" | "agent_message" => (StreamEventKind::Assistant, true),
        "result" | "completion" | "final" => {
            let ok = value
                .get("subtype")
                .and_then(|v| v.as_str())
                .map(|s| s != "error" && s != "failure")
                .or_else(|| value.get("is_error").and_then(|v| v.as_bool()).map(|b| !b))
                .or_else(|| value.get("ok").and_then(|v| v.as_bool()))
                .unwrap_or(true);
            (StreamEventKind::Result, ok)
        }
        "usage" => (StreamEventKind::Usage, true),
        "" if usage.is_some() => (StreamEventKind::Usage, true),
        "" => (StreamEventKind::Unknown, true),
        _ => (StreamEventKind::Unknown, true),
    };

    ParsedStreamEvent {
        kind,
        raw_line: raw.to_string(),
        redacted_line: redacted.to_string(),
        chat_id,
        text,
        usage,
        ok,
        parse_error: None,
    }
}

fn absorb_event(result: &mut StreamParseResult, event: ParsedStreamEvent) {
    if let Some(chat_id) = &event.chat_id {
        result.chat_id = Some(chat_id.clone());
    }
    if let Some(usage) = &event.usage {
        result.usage = Some(usage.clone());
    }
    if matches!(event.kind, StreamEventKind::Assistant) {
        if let Some(text) = &event.text {
            if !result.assistant_text.is_empty() {
                result.assistant_text.push('\n');
            }
            result.assistant_text.push_str(text);
        }
    }
    if matches!(event.kind, StreamEventKind::Diagnostic) || event.parse_error.is_some() {
        result.diagnostics.push(event.redacted_line.clone());
    }
    if matches!(event.kind, StreamEventKind::Result) {
        result.terminal_ok = Some(event.ok);
    }
    result.events.push(event);
}

fn extract_chat_id(value: &Value) -> Option<String> {
    const KEYS: &[&str] = &[
        "chatId",
        "chat_id",
        "session_id",
        "sessionId",
        "conversationId",
        "conversation_id",
        "id",
    ];
    for key in KEYS {
        if let Some(v) = value.get(*key).and_then(|v| v.as_str()) {
            if !v.is_empty()
                && (*key != "id"
                    || value
                        .get("type")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| t == "system" || t == "result"))
            {
                return Some(v.to_string());
            }
        }
    }
    if let Some(obj) = value.get("message").and_then(|m| m.as_object()) {
        for key in KEYS {
            if let Some(v) = obj.get(*key).and_then(|v| v.as_str()) {
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn extract_usage(value: &Value) -> Option<CursorUsage> {
    let usage_value = value.get("usage").cloned().or_else(|| {
        if value.get("inputTokens").is_some()
            || value.get("input_tokens").is_some()
            || value.get("totalTokens").is_some()
            || value.get("total_tokens").is_some()
        {
            Some(value.clone())
        } else {
            None
        }
    })?;

    let input = first_u64(
        &usage_value,
        &[
            "inputTokens",
            "input_tokens",
            "promptTokens",
            "prompt_tokens",
        ],
    );
    let output = first_u64(
        &usage_value,
        &[
            "outputTokens",
            "output_tokens",
            "completionTokens",
            "completion_tokens",
        ],
    );
    let total = first_u64(&usage_value, &["totalTokens", "total_tokens"]);

    if input.is_none() && output.is_none() && total.is_none() {
        return None;
    }

    Some(CursorUsage {
        input_tokens: input,
        output_tokens: output,
        total_tokens: total.or_else(|| match (input, output) {
            (Some(i), Some(o)) => Some(i + o),
            _ => None,
        }),
        source: Some("stream-json".into()),
    })
}

fn first_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(n) = value.get(*key).and_then(|v| v.as_u64()) {
            return Some(n);
        }
        if let Some(n) = value
            .get(*key)
            .and_then(|v| v.as_i64())
            .filter(|n| *n >= 0)
            .map(|n| n as u64)
        {
            return Some(n);
        }
    }
    None
}

fn extract_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    if let Some(result) = value.get("result").and_then(|v| v.as_str()) {
        return Some(result.to_string());
    }
    if let Some(content) = value.pointer("/message/content").and_then(|v| v.as_array()) {
        let mut parts = Vec::new();
        for item in content {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                parts.push(text.to_string());
            }
        }
        if !parts.is_empty() {
            return Some(parts.join(""));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_success_stream_with_chat_id_and_usage() {
        let stdout = r#"
{"type":"system","subtype":"init","session_id":"chat-abc","model":"composer-2.5"}
{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"}]}}
{"type":"result","subtype":"success","session_id":"chat-abc","result":"done","usage":{"inputTokens":11,"outputTokens":7,"totalTokens":18}}
"#;
        let parsed = parse_stream_json(stdout, "", &[]);
        assert_eq!(parsed.chat_id.as_deref(), Some("chat-abc"));
        assert_eq!(parsed.assistant_text, "Hello");
        assert_eq!(parsed.terminal_ok, Some(true));
        let usage = parsed.usage.expect("usage");
        assert_eq!(usage.input_tokens, Some(11));
        assert_eq!(usage.output_tokens, Some(7));
        assert_eq!(usage.total_tokens, Some(18));
    }

    #[test]
    fn malformed_mixed_streams_preserve_diagnostics() {
        let stdout = r#"
{"type":"system","session_id":"chat-1"}
NOT JSON <<<
{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}
{broken
{"type":"result","subtype":"success","session_id":"chat-1","usage":{"input_tokens":1,"output_tokens":2}}
"#;
        let parsed = parse_stream_json(stdout, "warn: noisy", &[]);
        assert!(parsed.diagnostics.len() >= 3);
        assert_eq!(parsed.chat_id.as_deref(), Some("chat-1"));
        assert!(parsed.assistant_text.contains("ok"));
        assert!(parsed
            .events
            .iter()
            .any(|e| e.parse_error.is_some() && e.redacted_line.contains("NOT JSON")));
    }

    #[test]
    fn redacts_secrets_in_stream_lines() {
        let stdout = r#"{"type":"assistant","text":"leak AKIAIOSFODNN7EXAMPLE please"}"#;
        let parsed = parse_stream_json(stdout, "", &["fixture-secret-value"]);
        let line = &parsed.events[0].redacted_line;
        assert!(!line.contains("AKIAIOSFODNN7EXAMPLE"));
    }
}
