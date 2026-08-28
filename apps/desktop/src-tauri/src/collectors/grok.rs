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
                "ArcMeter could not read a Grok Build session file",
                None,
            )],
            ..Default::default()
        },
    }
}

pub fn parse_reader<R: BufRead>(reader: R, device_id: &str) -> CollectorOutput {
    let mut output = CollectorOutput::default();
    let mut ids = HashSet::new();
    for (index, line_result) in reader.lines().enumerate() {
        let record_number = index + 1;
        output.records_seen += 1;
        let Ok(line) = line_result else { continue };
        if line.trim().is_empty() {
            output.records_ignored += 1;
            continue;
        }
        let record: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                output.diagnostics.push(diagnostic(
                    "warning",
                    "malformed_json",
                    "Ignored a malformed Grok Build record",
                    Some(record_number),
                ));
                continue;
            }
        };
        let usage = record
            .get("usage")
            .or_else(|| record.get("token_usage"))
            .or_else(|| record.pointer("/response/usage"));
        let Some(usage) = usage else {
            output.records_ignored += 1;
            continue;
        };
        let Some(session_id) = value_string(record.get("session_id"))
            .or_else(|| value_string(record.get("sessionId")))
        else {
            output.diagnostics.push(diagnostic(
                "warning",
                "missing_session_id",
                "Ignored a Grok Build usage record without session identity",
                Some(record_number),
            ));
            continue;
        };
        let Some(occurred_at) =
            parse_timestamp(record.get("timestamp").or_else(|| record.get("created_at")))
        else {
            output.diagnostics.push(diagnostic(
                "warning",
                "missing_timestamp",
                "Ignored a Grok Build usage record without a valid timestamp",
                Some(record_number),
            ));
            continue;
        };
        let input = value_i64(
            usage
                .get("prompt_tokens")
                .or_else(|| usage.get("input_tokens")),
        );
        let cached = value_i64(
            usage
                .get("cached_tokens")
                .or_else(|| usage.get("cached_input_tokens")),
        );
        let output_tokens = value_i64(
            usage
                .get("completion_tokens")
                .or_else(|| usage.get("output_tokens")),
        );
        let reasoning = value_i64(usage.get("reasoning_tokens"));
        let tokens = TokenCounts {
            input_tokens: input,
            cached_input_tokens: cached,
            output_tokens,
            reasoning_tokens: reasoning,
            total_tokens: value_i64(usage.get("total_tokens")),
        };
        if tokens.total_tokens == 0 && input == 0 && output_tokens == 0 {
            output.records_ignored += 1;
            continue;
        }
        let model = value_string(record.get("model"))
            .or_else(|| value_string(record.pointer("/response/model")));
        let native_event_id = value_string(record.get("event_id"))
            .or_else(|| value_string(record.get("turn_id")))
            .or_else(|| value_string(record.get("id")))
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
            "grok",
            "grok_build",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FIXTURE: &str = include_str!("../../tests/fixtures/grok.jsonl");

    #[test]
    fn fixture_handles_optional_usage_fields_and_duplicates() {
        let output = parse_reader(Cursor::new(FIXTURE), "device");
        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].tokens.total_tokens, 27);
        assert_eq!(output.events[0].tokens.reasoning_tokens, 2);
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
