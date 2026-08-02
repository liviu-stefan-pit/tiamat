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
    // Harvest CreatePlan bodies from raw JSON (plan mode puts MASTER-PLAN here).
    if let Ok(value) = serde_json::from_str::<Value>(&event.raw_line) {
        if let Some(plan) = harvest_plan_tool_markdown(&value) {
            result.plan_markdown = plan;
        }
    }
    result.events.push(event);
}

fn is_plan_tool_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace(['_', '-'], "");
    n == "createplan" || n == "writeplan" || n == "updateplan" || n == "editplan"
}

/// Pull structured MASTER-PLAN markdown out of CreatePlan-style tool arguments.
fn harvest_plan_tool_markdown(value: &Value) -> Option<String> {
    // Real Cursor CLI stream-json (2026+):
    // {"type":"tool_call","tool_call":{"createPlanToolCall":{"args":{"plan":"..."}}}}
    if let Some(tool_call) = value.get("tool_call").and_then(|v| v.as_object()) {
        for (key, body) in tool_call {
            let key_l = key.to_ascii_lowercase();
            if key_l.contains("createplan")
                || key_l.contains("writeplan")
                || key_l == "plantoolcall"
            {
                if let Some(plan) = body
                    .pointer("/args/plan")
                    .or_else(|| body.pointer("/input/plan"))
                    .or_else(|| body.pointer("/arguments/plan"))
                    .and_then(|v| v.as_str())
                {
                    let plan = plan.trim();
                    if !plan.is_empty() {
                        return Some(plan.to_string());
                    }
                }
            }
        }
    }

    if let Some(content) = value.pointer("/message/content").and_then(|v| v.as_array()) {
        for item in content {
            if let Some(plan) = plan_from_tool_item(item) {
                return Some(plan);
            }
        }
    }
    // Top-level tool_call / tool_use frames with name + input.
    let type_name = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        type_name.as_str(),
        "tool_call" | "tool-call" | "tool_use" | "tool-use" | "toolcall"
    ) {
        let name = value
            .get("name")
            .or_else(|| value.pointer("/tool/name"))
            .or_else(|| value.get("tool_name"))
            .or_else(|| value.get("toolName"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if is_plan_tool_name(name) {
            if let Some(plan) = plan_string_from_args(value) {
                return Some(plan);
            }
        }
    }
    None
}

fn plan_from_tool_item(item: &Value) -> Option<String> {
    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if !is_plan_tool_name(name) {
        return None;
    }
    plan_string_from_args(item)
}

fn plan_string_from_args(value: &Value) -> Option<String> {
    for path in [
        "/input/plan",
        "/arguments/plan",
        "/params/plan",
        "/input/contents",
        "/input/body",
        "/arguments/contents",
    ] {
        if let Some(plan) = value.pointer(path).and_then(|v| v.as_str()) {
            let plan = plan.trim();
            if plan.len() > 40
                && (plan.contains("## Phase")
                    || plan.to_ascii_lowercase().contains("## phase")
                    || plan.contains("# "))
            {
                return Some(plan.to_string());
            }
        }
    }
    // Nested input object with plan key.
    if let Some(input) = value.get("input").or_else(|| value.get("arguments")) {
        if let Some(plan) = input.get("plan").and_then(|v| v.as_str()) {
            let plan = plan.trim();
            if !plan.is_empty() {
                return Some(plan.to_string());
            }
        }
    }
    None
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

    #[test]
    fn harvests_createplan_from_cli_create_plan_tool_call() {
        // Actual Cursor agent stream-json shape (plan mode).
        let stdout = r##"
{"type":"system","subtype":"init","session_id":"chat-cli"}
{"type":"tool_call","subtype":"started","call_id":"tool_1","tool_call":{"createPlanToolCall":{"args":{"plan":"# Demo\n\n## Summary\ns\n\n## Assumptions\n- a\n\n## Risks\n- r\n\n## Phase: P01 - Slice\n\nbody","name":"Demo","overview":"o","todos":[]},"toolCallId":"tool_1"}},"session_id":"chat-cli"}
{"type":"tool_call","subtype":"completed","call_id":"tool_1","tool_call":{"createPlanToolCall":{"args":{"plan":"# Demo\n\n## Summary\ns\n\n## Assumptions\n- a\n\n## Risks\n- r\n\n## Phase: P01 - Slice\n\nbody","name":"Demo","overview":"o","todos":[]},"result":{"success":{}},"toolCallId":"tool_1"}},"session_id":"chat-cli"}
{"type":"result","subtype":"success","session_id":"chat-cli"}
"##;
        let parsed = parse_stream_json(stdout, "", &[]);
        assert_eq!(parsed.chat_id.as_deref(), Some("chat-cli"));
        assert!(
            parsed.plan_markdown.contains("## Phase: P01"),
            "createPlanToolCall.args.plan must be harvested: {}",
            parsed.plan_markdown.chars().take(200).collect::<String>()
        );
    }

    #[test]
    fn harvests_createplan_plan_field_from_assistant_tool_use() {
        // Use r## so embedded "# Title" / "# Demo" do not terminate the raw string.
        let stdout = r##"
{"type":"system","subtype":"init","session_id":"chat-plan","model":"cursor-grok-4.5-high"}
{"type":"assistant","message":{"content":[{"type":"text","text":"Drafting the plan now."},{"type":"tool_use","name":"CreatePlan","input":{"name":"Demo","overview":"o","plan":"# Demo\n\n## Summary\ns\n\n## Assumptions\n- a\n\n## Risks\n- r\n\n## Phase: P01 - Slice\n\nbody"}}]}}
{"type":"result","subtype":"success","session_id":"chat-plan","result":"ok"}
"##;
        let parsed = parse_stream_json(stdout, "", &[]);
        assert_eq!(parsed.chat_id.as_deref(), Some("chat-plan"));
        assert!(parsed.assistant_text.contains("Drafting the plan"));
        assert!(
            parsed.plan_markdown.contains("## Phase: P01"),
            "CreatePlan.plan must be harvested: {}",
            parsed.plan_markdown.chars().take(200).collect::<String>()
        );
        assert!(!parsed.assistant_text.contains("## Phase: P01"));
    }

    #[test]
    fn harvests_createplan_from_top_level_tool_call() {
        let stdout = r##"
{"type":"tool_call","session_id":"chat-2","name":"CreatePlan","input":{"plan":"# Title\n\n## Summary\nx\n\n## Assumptions\n- a\n\n## Risks\n- r\n\n## Phase: P01 - A\n\nbody"}}
{"type":"result","subtype":"success","session_id":"chat-2"}
"##;
        let parsed = parse_stream_json(stdout, "", &[]);
        assert!(parsed.plan_markdown.contains("## Phase: P01"));
    }
}
