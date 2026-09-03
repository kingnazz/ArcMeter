use super::{diagnostic, parse_timestamp, value_i64, value_string};
use crate::domain::{CollectorOutput, TokenCounts, UsageEvent, fallback_event_fingerprint};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn parse_file(path: &Path, device_id: &str) -> CollectorOutput {
    match File::open(path) {
        Ok(file) => parse_reader(BufReader::new(file), device_id),
        Err(_) => CollectorOutput {
            diagnostics: vec![diagnostic(
                "error",
                "source_unreadable",
                "ArcMeter could not read a Codex session file",
                None,
            )],
            ..Default::default()
        },
    }
}

pub fn parse_reader<R: BufRead>(reader: R, device_id: &str) -> CollectorOutput {
    let mut output = CollectorOutput::default();
    let mut session_id: Option<String> = None;
    let mut project_name: Option<String> = None;
    let mut model: Option<String> = None;
    let mut turn_id: Option<String> = None;
    let mut ids = HashSet::new();

    for (index, line_result) in reader.lines().enumerate() {
        let record_number = index + 1;
        output.records_seen += 1;
        let line = match line_result {
            Ok(line) if !line.trim().is_empty() => line,
            Ok(_) => {
                output.records_ignored += 1;
                continue;
            }
            Err(_) => {
                output.diagnostics.push(diagnostic(
                    "warning",
                    "line_unreadable",
                    "A Codex record could not be read",
                    Some(record_number),
                ));
                continue;
            }
        };
        let record: Value = match serde_json::from_str(&line) {
            Ok(record) => record,
            Err(_) => {
                output.diagnostics.push(diagnostic(
                    "warning",
                    "malformed_json",
                    "Ignored a malformed Codex record",
                    Some(record_number),
                ));
                continue;
            }
        };

        match record.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                let payload = record.get("payload").unwrap_or(&Value::Null);
                session_id = value_string(payload.get("session_id"))
                    .or_else(|| value_string(payload.get("id")));
                project_name = value_string(payload.get("cwd"));
            }
            Some("turn_context") => {
                let payload = record.get("payload").unwrap_or(&Value::Null);
                turn_id = value_string(payload.get("turn_id"));
                model = value_string(payload.get("model")).or(model);
                project_name = value_string(payload.get("cwd")).or(project_name);
            }
            Some("event_msg") => {
                let payload = record.get("payload").unwrap_or(&Value::Null);
                if payload.get("type").and_then(Value::as_str) == Some("task_started") {
                    turn_id = value_string(payload.get("turn_id")).or(turn_id);
                }
                if payload.get("type").and_then(Value::as_str) != Some("token_count") {
                    output.records_ignored += 1;
                    continue;
                }
                let Some(info) = payload.get("info") else {
                    output.records_ignored += 1;
                    continue;
                };
                let Some(usage) = info.get("last_token_usage") else {
                    output.records_ignored += 1;
                    continue;
                };
                let Some(occurred_at) = parse_timestamp(record.get("timestamp")) else {
                    output.diagnostics.push(diagnostic(
                        "warning",
                        "missing_timestamp",
                        "Ignored a Codex token record without a valid timestamp",
                        Some(record_number),
                    ));
                    continue;
                };
                let Some(native_session_id) = session_id.clone() else {
                    output.diagnostics.push(diagnostic(
                        "warning",
                        "missing_session_id",
                        "Ignored a Codex token record without session identity",
                        Some(record_number),
                    ));
                    continue;
                };
                let tokens = TokenCounts {
                    input_tokens: value_i64(usage.get("input_tokens")),
                    cached_input_tokens: value_i64(usage.get("cached_input_tokens")),
                    cache_write_tokens: value_i64(
                        usage
                            .get("cache_write_input_tokens")
                            .or_else(|| usage.get("cacheWriteInputTokens")),
                    ),
                    cache_write_5m_tokens: 0,
                    cache_write_1h_tokens: 0,
                    output_tokens: value_i64(usage.get("output_tokens")),
                    reasoning_tokens: value_i64(usage.get("reasoning_output_tokens")),
                    total_tokens: value_i64(usage.get("total_tokens")),
                };
                let native_event_id = record
                    .get("ordinal")
                    .and_then(Value::as_i64)
                    .map(|ordinal| format!("ordinal:{ordinal}"))
                    .unwrap_or_else(|| {
                        let timestamp = occurred_at.to_rfc3339();
                        let total = tokens.total_tokens.to_string();
                        fallback_event_fingerprint(&[
                            &native_session_id,
                            turn_id.as_deref().unwrap_or("unknown-turn"),
                            &timestamp,
                            model.as_deref().unwrap_or("unknown-model"),
                            &total,
                            &record_number.to_string(),
                        ])
                    });
                let event = UsageEvent::measured(
                    "codex",
                    "codex_cli",
                    native_session_id,
                    native_event_id,
                    occurred_at,
                    model.clone(),
                    project_name.clone(),
                    tokens,
                    device_id,
                );
                if ids.insert(event.id.clone()) {
                    output.events.push(event);
                } else {
                    output.records_ignored += 1;
                }
            }
            Some(_) | None => output.records_ignored += 1,
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FIXTURE: &str = include_str!("../../tests/fixtures/codex.jsonl");
    const CACHE_WRITE_FIXTURE: &str = include_str!("../../tests/fixtures/codex_cache_write.jsonl");

    #[test]
    fn fixture_is_resilient_and_deduplicated() {
        let output = parse_reader(Cursor::new(FIXTURE), "device");
        assert_eq!(output.events.len(), 2);
        assert_eq!(output.events[0].tokens.total_tokens, 120);
        assert_eq!(output.events[0].project_name.as_deref(), Some("ArcMeter"));
        assert_eq!(output.events[1].tokens.cached_input_tokens, 0);
        assert_eq!(output.events[1].tokens.cache_write_tokens, 0);
        assert_eq!(output.events[1].tokens.total_tokens, 12);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|item| item.code == "malformed_json")
        );
    }

    #[test]
    fn empty_and_missing_identity_sources_are_safe() {
        assert!(parse_reader(Cursor::new(""), "device").events.is_empty());
        let record = r#"{"timestamp":"2026-08-20T12:00:00Z","ordinal":1,"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1,"total_tokens":1}}}}"#;
        let output = parse_reader(Cursor::new(record), "device");
        assert!(output.events.is_empty());
        assert!(
            output
                .diagnostics
                .iter()
                .any(|item| item.code == "missing_session_id")
        );
    }

    #[test]
    fn reingestion_produces_identical_ids() {
        let first = parse_reader(Cursor::new(FIXTURE), "device");
        let second = parse_reader(Cursor::new(FIXTURE), "device");
        assert_eq!(
            first
                .events
                .iter()
                .map(|event| &event.id)
                .collect::<Vec<_>>(),
            second
                .events
                .iter()
                .map(|event| &event.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn current_codex_token_usage_captures_aggregate_cache_writes() {
        let output = parse_reader(Cursor::new(CACHE_WRITE_FIXTURE), "device");
        assert_eq!(output.events.len(), 1);
        let tokens = &output.events[0].tokens;
        assert_eq!(tokens.input_tokens, 58_697);
        assert_eq!(tokens.cached_input_tokens, 40_000);
        assert_eq!(tokens.cache_write_tokens, 12_000);
        assert_eq!(tokens.cache_write_5m_tokens, 0);
        assert_eq!(tokens.cache_write_1h_tokens, 0);
        assert_eq!(tokens.output_tokens, 227);
        assert_eq!(tokens.reasoning_tokens, 9);
        assert_eq!(tokens.total_tokens, 58_924);
    }

    #[test]
    fn camel_case_cache_write_field_is_supported_without_ttl_attribution() {
        let record = r#"{"timestamp":"2026-08-20T12:00:00Z","ordinal":2,"type":"session_meta","payload":{"id":"session"}}
{"timestamp":"2026-08-20T12:00:01Z","ordinal":3,"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":60,"cacheWriteInputTokens":20,"output_tokens":20,"reasoning_output_tokens":5,"total_tokens":120}}}}"#;
        let output = parse_reader(Cursor::new(record), "device");
        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].tokens.cache_write_tokens, 20);
        assert_eq!(output.events[0].tokens.total_tokens, 120);
        assert_eq!(output.events[0].tokens.input_tokens, 100);
        assert_eq!(output.events[0].tokens.cache_write_5m_tokens, 0);
        assert_eq!(output.events[0].tokens.cache_write_1h_tokens, 0);
    }
}
