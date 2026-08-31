use super::{diagnostic, parse_timestamp, value_i64, value_string};
use crate::domain::{
    CollectorOutput, EventReconciliation, TokenCounts, UsageEvent, deterministic_event_id,
    fallback_event_fingerprint, is_more_authoritative_snapshot,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

struct Candidate {
    event: UsageEvent,
}

impl Candidate {
    fn is_more_authoritative_than(&self, other: &Self) -> bool {
        is_more_authoritative_snapshot(
            &self.event.tokens,
            &self.event.occurred_at,
            self.event.model.as_deref(),
            self.event.project_name.as_deref(),
            &other.event.tokens,
            &other.event.occurred_at,
            other.event.model.as_deref(),
            other.event.project_name.as_deref(),
        )
    }
}

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
    let mut requests = BTreeMap::<String, Candidate>::new();
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
        if record
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|record_type| record_type != "assistant")
        {
            output.records_ignored += 1;
            continue;
        }
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

        // Anthropic reports fresh input, cache reads, cache creation, and output as
        // separate additive counters. Output already includes any thinking tokens.
        let input = value_i64(usage.get("input_tokens"));
        let cached = value_i64(usage.get("cache_read_input_tokens"));
        let aggregate_cache_write = value_i64(usage.get("cache_creation_input_tokens"));
        let cache_creation = usage.get("cache_creation");
        let cache_write_5m = cache_creation
            .map(|value| value_i64(value.get("ephemeral_5m_input_tokens")))
            .unwrap_or_default();
        let cache_write_1h = cache_creation
            .map(|value| value_i64(value.get("ephemeral_1h_input_tokens")))
            .unwrap_or_default();
        let detailed_cache_tokens = cache_write_5m.saturating_add(cache_write_1h);
        let cache_write = aggregate_cache_write.max(detailed_cache_tokens);
        let output_tokens = value_i64(usage.get("output_tokens"));
        let reasoning_tokens = usage
            .get("output_tokens_details")
            .map(|value| value_i64(value.get("thinking_tokens")))
            .unwrap_or_default()
            .min(output_tokens);
        let tokens = TokenCounts {
            input_tokens: input,
            cached_input_tokens: cached,
            cache_write_tokens: cache_write,
            cache_write_5m_tokens: cache_write_5m,
            cache_write_1h_tokens: cache_write_1h,
            output_tokens,
            reasoning_tokens,
            total_tokens: input
                .saturating_add(cached)
                .saturating_add(cache_write)
                .saturating_add(output_tokens),
        };
        let model = value_string(message.get("model"));
        let parent_identity = value_string(record.get("parentUuid"))
            .or_else(|| value_string(record.get("parent_uuid")));
        let old_native_event_id = value_string(record.get("uuid"))
            .or_else(|| value_string(message.get("id")))
            .unwrap_or_else(|| {
                fallback_event_fingerprint(&[
                    &session_id,
                    &occurred_at.to_rfc3339(),
                    model.as_deref().unwrap_or("unknown-model"),
                    &tokens.total_tokens.to_string(),
                    &record_number.to_string(),
                ])
            });
        let native_event_id = value_string(record.get("requestId"))
            .or_else(|| value_string(record.get("request_id")))
            .map(|value| format!("request:{value}"))
            .or_else(|| value_string(message.get("id")).map(|value| format!("message:{value}")))
            .or_else(|| value_string(record.get("uuid")).map(|value| format!("uuid:{value}")))
            .unwrap_or_else(|| {
                let fingerprint = fallback_event_fingerprint(&[
                    &session_id,
                    parent_identity.as_deref().unwrap_or("no-parent"),
                    &occurred_at.to_rfc3339(),
                    model.as_deref().unwrap_or("unknown-model"),
                ]);
                format!("request_{fingerprint}")
            });
        let legacy_event_id = deterministic_event_id("claude", &session_id, &old_native_event_id);
        let event = UsageEvent::measured(
            "claude",
            "claude_code",
            session_id,
            native_event_id,
            occurred_at,
            model,
            value_string(record.get("cwd")),
            tokens,
            device_id,
        );
        let candidate = Candidate { event };
        if legacy_event_id != candidate.event.id {
            output.reconciliation_hints.push(EventReconciliation {
                legacy_event_id,
                replacement_event_id: candidate.event.id.clone(),
            });
        }
        match requests.entry(candidate.event.id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(candidate);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                output.records_ignored += 1;
                if candidate.is_more_authoritative_than(entry.get()) {
                    entry.insert(candidate);
                }
                if !output
                    .diagnostics
                    .iter()
                    .any(|item| item.code == "duplicate_request_record")
                {
                    output.diagnostics.push(diagnostic(
                        "warning",
                        "duplicate_request_record",
                        "Collapsed multiple Claude Code CLI records for one request",
                        Some(record_number),
                    ));
                }
            }
        }
    }
    output.events = requests
        .into_values()
        .map(|candidate| candidate.event)
        .collect();
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FIXTURE: &str = include_str!("../../tests/fixtures/claude.jsonl");

    #[test]
    fn fixture_deduplicates_requests_and_preserves_additive_cache_semantics() {
        let output = parse_reader(Cursor::new(FIXTURE), "device");
        assert_eq!(output.events.len(), 9);
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.tokens.input_tokens)
                .sum::<i64>(),
            57
        );
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.tokens.cached_input_tokens)
                .sum::<i64>(),
            20
        );
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.tokens.cache_write_tokens)
                .sum::<i64>(),
            33
        );
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.tokens.cache_write_5m_tokens)
                .sum::<i64>(),
            15
        );
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.tokens.cache_write_1h_tokens)
                .sum::<i64>(),
            10
        );
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.tokens.total_tokens)
                .sum::<i64>(),
            139
        );
        assert!(output.events.iter().any(|event| {
            event.native_event_id == "request:req-mixed"
                && event.tokens.cache_write_tokens == 12
                && event.tokens.reasoning_tokens == 2
        }));
        assert!(
            output
                .events
                .iter()
                .any(|event| event.native_event_id == "message:msg-fallback")
        );
        assert!(
            output
                .events
                .iter()
                .any(|event| event.native_event_id == "uuid:uuid-fallback")
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|item| item.code == "duplicate_request_record")
        );
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
                .any(|item| item.code == "missing_timestamp")
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|item| item.code == "missing_session_id")
        );
        assert_eq!(output.reconciliation_hints.len(), 10);
    }

    #[test]
    fn fallback_identity_is_stable_and_does_not_depend_on_row_number() {
        let record = r#"{"type":"assistant","sessionId":"session","cwd":"Fixture","timestamp":"2026-08-29T00:00:00Z","message":{"model":"unknown-future-model","usage":{"input_tokens":1,"output_tokens":1}}}"#;
        let first = parse_reader(Cursor::new(format!("{record}\n")), "device");
        let second = parse_reader(
            Cursor::new(format!("{{\"type\":\"metadata\"}}\n{record}\n")),
            "device",
        );
        assert_eq!(first.events[0].id, second.events[0].id);
        assert!(
            first.events[0]
                .native_event_id
                .starts_with("request_fingerprint:")
        );
    }

    #[test]
    fn empty_and_reingested_sources_are_stable() {
        assert!(parse_reader(Cursor::new(""), "device").events.is_empty());
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
}
