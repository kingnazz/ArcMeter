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
                "ArcMeter could not read a Claude Code CLI session file",
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
        let Ok(line) = line_result else {
            output.diagnostics.push(diagnostic(
                "warning",
                "line_unreadable",
                "A Claude Code CLI record could not be read",
                Some(record_number),
            ));
            continue;
        };
        if line.trim().is_empty() {
            output.records_ignored += 1;
            continue;
        }
        let record: Value = match serde_json::from_str(&line) {
            Ok(record) => record,
            Err(_) => {
                output.diagnostics.push(diagnostic(
                    "warning",
                    "malformed_json",
                    "Ignored a malformed Claude Code CLI record",
                    Some(record_number),
                ));
                continue;
            }
        };
        let Some(message) = record.get("message") else {
            output.records_ignored += 1;
            continue;
        };
        let Some(usage) = message.get("usage") else {
            output.records_ignored += 1;
            continue;
        };
        let Some(session_id) = value_string(record.get("sessionId"))
            .or_else(|| value_string(record.get("session_id")))
        else {
            output.diagnostics.push(diagnostic(
                "warning",
                "missing_session_id",
                "Ignored a Claude Code CLI usage record without session identity",
                Some(record_number),
            ));
            continue;
        };
        let Some(occurred_at) = parse_timestamp(record.get("timestamp")) else {
            output.diagnostics.push(diagnostic(
                "warning",
                "missing_timestamp",
                "Ignored a Claude Code CLI usage record without a valid timestamp",
                Some(record_number),
            ));
            continue;
        };
        let input = value_i64(usage.get("input_tokens"));
        let cached = value_i64(usage.get("cache_read_input_tokens"));
        let cache_write = value_i64(usage.get("cache_creation_input_tokens"));
        let output_tokens = value_i64(usage.get("output_tokens"));
        let tokens = TokenCounts {
            input_tokens: input.saturating_add(cached).saturating_add(cache_write),
            cached_input_tokens: cached,
            cache_write_tokens: 0,
            output_tokens,
            reasoning_tokens: 0,
            total_tokens: input
                .saturating_add(cached)
                .saturating_add(cache_write)
                .saturating_add(output_tokens),
        };
        let native_event_id = value_string(record.get("uuid"))
            .or_else(|| value_string(message.get("id")))
            .unwrap_or_else(|| {
                fallback_event_fingerprint(&[
                    &session_id,
                    &occurred_at.to_rfc3339(),
                    value_string(message.get("model"))
                        .as_deref()
                        .unwrap_or("unknown-model"),
                    &tokens.total_tokens.to_string(),
                    &record_number.to_string(),
                ])
            });
        let event = UsageEvent::measured(
            "claude",
            "claude_code",
            session_id,
            native_event_id,
            occurred_at,
            value_string(message.get("model")),
            value_string(record.get("cwd")),
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

    const FIXTURE: &str = include_str!("../../tests/fixtures/claude.jsonl");

    #[test]
    fn fixture_handles_cache_malformed_duplicate_and_missing_identity() {
        let output = parse_reader(Cursor::new(FIXTURE), "device");
        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].tokens.input_tokens, 42);
        assert_eq!(output.events[0].tokens.cached_input_tokens, 30);
        assert_eq!(output.events[0].tokens.total_tokens, 47);
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
        let first = parse_reader(Cursor::new(FIXTURE), "device");
        let second = parse_reader(Cursor::new(FIXTURE), "device");
        assert_eq!(first.events[0].id, second.events[0].id);
    }
}
