use super::{diagnostic, parse_timestamp, value_i64, value_string};
use crate::domain::{CollectorOutput, TokenCounts, UsageEvent, fallback_event_fingerprint};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;

pub fn parse_file(path: &Path, device_id: &str) -> CollectorOutput {
    let Ok(bytes) = fs::read(path) else {
        return CollectorOutput {
            diagnostics: vec![diagnostic(
                "error",
                "source_unreadable",
                "ArcMeter could not read a Gemini CLI session file",
                None,
            )],
            ..Default::default()
        };
    };
    if let Ok(value) = serde_json::from_slice::<Value>(&bytes)
        && (value.is_array() || value.get("messages").is_some() || value.get("events").is_some())
    {
        return parse_document(value, device_id);
    }
    parse_reader(BufReader::new(Cursor::new(bytes)), device_id)
}

pub fn parse_reader<R: BufRead>(reader: R, device_id: &str) -> CollectorOutput {
    let mut values = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen = 0;
    for (index, line_result) in reader.lines().enumerate() {
        seen += 1;
        let Ok(line) = line_result else { continue };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => values.push(value),
            Err(_) => diagnostics.push(diagnostic(
                "warning",
                "malformed_json",
                "Ignored a malformed Gemini CLI record",
                Some(index + 1),
            )),
        }
    }
    let mut output = parse_values(&values, None, device_id);
    output.records_seen = seen;
    output.diagnostics.splice(0..0, diagnostics);
    output
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FIXTURE: &str = include_str!("../../tests/fixtures/gemini.jsonl");

    #[test]
    fn fixture_handles_native_usage_metadata_and_duplicates() {
        let output = parse_reader(Cursor::new(FIXTURE), "device");
        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].tokens.total_tokens, 38);
        assert_eq!(output.events[0].tokens.cached_input_tokens, 10);
        assert_eq!(output.events[0].tokens.reasoning_tokens, 3);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|item| item.code == "malformed_json")
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|item| item.code == "missing_session_id")
        );
    }

    #[test]
    fn empty_and_reingested_sources_are_stable() {
        assert!(parse_reader(Cursor::new(""), "device").events.is_empty());
        assert_eq!(
            parse_reader(Cursor::new(FIXTURE), "device").events[0].id,
            parse_reader(Cursor::new(FIXTURE), "device").events[0].id
        );
    }
}

fn parse_document(value: Value, device_id: &str) -> CollectorOutput {
    let session_id =
        value_string(value.get("sessionId")).or_else(|| value_string(value.get("session_id")));
    let values = value
        .as_array()
        .cloned()
        .or_else(|| value.get("messages").and_then(Value::as_array).cloned())
        .or_else(|| value.get("events").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    parse_values(&values, session_id, device_id)
}

fn parse_values(
    values: &[Value],
    document_session_id: Option<String>,
    device_id: &str,
) -> CollectorOutput {
    let mut output = CollectorOutput {
        records_seen: values.len(),
        ..Default::default()
    };
    let mut ids = HashSet::new();
    for (index, record) in values.iter().enumerate() {
        let record_number = index + 1;
        let usage = record
            .get("usageMetadata")
            .or_else(|| record.get("usage_metadata"))
            .or_else(|| record.pointer("/response/usageMetadata"));
        let Some(usage) = usage else {
            output.records_ignored += 1;
            continue;
        };
        let session_id = value_string(record.get("sessionId"))
            .or_else(|| value_string(record.get("session_id")))
            .or_else(|| document_session_id.clone());
        let Some(session_id) = session_id else {
            output.diagnostics.push(diagnostic(
                "warning",
                "missing_session_id",
                "Ignored a Gemini CLI usage record without session identity",
                Some(record_number),
            ));
            continue;
        };
        let Some(occurred_at) =
            parse_timestamp(record.get("timestamp").or_else(|| record.get("createdAt")))
        else {
            output.diagnostics.push(diagnostic(
                "warning",
                "missing_timestamp",
                "Ignored a Gemini CLI usage record without a valid timestamp",
                Some(record_number),
            ));
            continue;
        };
        let input = value_i64(
            usage
                .get("promptTokenCount")
                .or_else(|| usage.get("input_tokens")),
        );
        let cached = value_i64(
            usage
                .get("cachedContentTokenCount")
                .or_else(|| usage.get("cached_input_tokens")),
        );
        let output_tokens = value_i64(
            usage
                .get("candidatesTokenCount")
                .or_else(|| usage.get("output_tokens")),
        );
        let reasoning = value_i64(
            usage
                .get("thoughtsTokenCount")
                .or_else(|| usage.get("reasoning_tokens")),
        );
        let tokens = TokenCounts {
            input_tokens: input,
            cached_input_tokens: cached,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens,
            reasoning_tokens: reasoning,
            total_tokens: value_i64(
                usage
                    .get("totalTokenCount")
                    .or_else(|| usage.get("total_tokens")),
            ),
        };
        let model = value_string(record.get("model"))
            .or_else(|| value_string(record.pointer("/response/model")));
        let native_event_id = value_string(record.get("id"))
            .or_else(|| value_string(record.get("messageId")))
            .or_else(|| value_string(record.get("turnId")))
            .unwrap_or_else(|| {
                fallback_event_fingerprint(&[
                    &session_id,
                    &occurred_at.to_rfc3339(),
                    model.as_deref().unwrap_or("unknown-model"),
                    &tokens.total_tokens.to_string(),
                    &record_number.to_string(),
                ])
            });
        let event = UsageEvent::measured(
            "gemini",
            "gemini_cli",
            session_id,
            native_event_id,
            occurred_at,
            model,
            value_string(record.get("cwd")).or_else(|| value_string(record.get("project"))),
            tokens,
            device_id,
        );
        if ids.insert(event.id.clone()) {
            output.events.push(event);
        } else {
            output.records_ignored += 1;
        }
    }
    output
}
