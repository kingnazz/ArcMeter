use super::{diagnostic, parse_timestamp, value_i64, value_string};
use crate::domain::{CollectorOutput, TokenCounts, UsageEvent, fallback_event_fingerprint};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn parse_file(path: &Path, device_id: &str) -> CollectorOutput {
    let context = path_context(path);
    match File::open(path) {
        Ok(file) => parse_reader_with_context(
            BufReader::new(file),
            device_id,
            context.session_id.as_deref(),
            context.project.as_deref(),
        ),
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

#[cfg(test)]
pub fn parse_reader<R: BufRead>(reader: R, device_id: &str) -> CollectorOutput {
    parse_reader_with_context(reader, device_id, None, None)
}

fn parse_reader_with_context<R: BufRead>(
    reader: R,
    device_id: &str,
    path_session_id: Option<&str>,
    path_project: Option<&str>,
) -> CollectorOutput {
    let mut output = CollectorOutput::default();
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
                    "A Grok Build record could not be read",
                    Some(record_number),
                ));
                continue;
            }
        };
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
        let update = record
            .pointer("/params/update")
            .or_else(|| record.get("update"))
            .unwrap_or(&record);
        if value_string(update.get("sessionUpdate")).as_deref() != Some("turn_completed") {
            output.records_ignored += 1;
            continue;
        }
        let Some(usage) = update.get("usage") else {
            output.records_ignored += 1;
            continue;
        };
        let Some(session_id) =
            session_id(&record, update).or_else(|| path_session_id.map(str::to_owned))
        else {
            output.diagnostics.push(diagnostic(
                "warning",
                "missing_session_id",
                "Ignored a completed Grok Build turn without session identity",
                Some(record_number),
            ));
            continue;
        };
        let Some(occurred_at) = event_timestamp(&record, update) else {
            output.diagnostics.push(diagnostic(
                "warning",
                "missing_timestamp",
                "Ignored a completed Grok Build turn without a valid timestamp",
                Some(record_number),
            ));
            continue;
        };

        let project = project_name(&record, update).or_else(|| path_project.map(str::to_owned));
        let top_tokens = parse_tokens(usage);
        let top_cost = complete_cost(usage);
        let mut measured = model_measurements(usage);
        if measured.is_empty() && has_usage(&top_tokens) {
            measured.push((
                fallback_model(&record, update, usage),
                top_tokens.clone(),
                top_cost,
            ));
        }
        if measured.is_empty() {
            output.records_ignored += 1;
            continue;
        }

        let source_turn_id = turn_id(update)
            .unwrap_or_else(|| fallback_turn_id(&session_id, occurred_at, &top_tokens, &measured));
        let turn_hash = stable_component(&[&session_id, &source_turn_id]);

        if measured.len() == 1 && measured[0].2.is_none() {
            measured[0].2 = top_cost;
        } else if measured.len() > 1
            && top_cost.is_some()
            && measured.iter().any(|(_, _, cost)| cost.is_none())
        {
            output.diagnostics.push(diagnostic(
                "warning",
                "unallocated_native_cost",
                "A multi-model Grok Build turn had native cost that could not be allocated safely",
                Some(record_number),
            ));
        }

        let mut inserted_for_record = 0;
        for (model, tokens, native_cost) in measured {
            if !has_usage(&tokens) {
                continue;
            }
            let model_identity = model.as_deref().unwrap_or("unknown-model");
            let native_event_id = format!(
                "turn:{turn_hash}:model:{}",
                stable_component(&[model_identity])
            );
            let event = UsageEvent::measured(
                "grok",
                "grok_build",
                session_id.clone(),
                native_event_id,
                occurred_at,
                model,
                project.clone(),
                tokens,
                device_id,
            )
            .with_native_cost_usd_ticks(native_cost);
            if ids.insert(event.id.clone()) {
                output.events.push(event);
                inserted_for_record += 1;
            }
        }
        if inserted_for_record == 0 {
            output.records_ignored += 1;
        }
    }
    output
}

fn fallback_turn_id(
    session_id: &str,
    occurred_at: DateTime<Utc>,
    top_tokens: &TokenCounts,
    measured: &[(Option<String>, TokenCounts, Option<i64>)],
) -> String {
    let mut parts = vec![
        session_id.to_owned(),
        occurred_at.to_rfc3339(),
        top_tokens.input_tokens.to_string(),
        top_tokens.cached_input_tokens.to_string(),
        top_tokens.cache_write_tokens.to_string(),
        top_tokens.output_tokens.to_string(),
        top_tokens.reasoning_tokens.to_string(),
        top_tokens.total_tokens.to_string(),
    ];
    for (model, tokens, _) in measured {
        parts.extend([
            model.as_deref().unwrap_or("unknown-model").to_owned(),
            tokens.input_tokens.to_string(),
            tokens.cached_input_tokens.to_string(),
            tokens.cache_write_tokens.to_string(),
            tokens.output_tokens.to_string(),
            tokens.reasoning_tokens.to_string(),
            tokens.total_tokens.to_string(),
        ]);
    }
    fallback_event_fingerprint(&parts.iter().map(String::as_str).collect::<Vec<_>>())
}

fn model_measurements(usage: &Value) -> Vec<(Option<String>, TokenCounts, Option<i64>)> {
    let Some(models) = usage.get("modelUsage").and_then(Value::as_object) else {
        return Vec::new();
    };
    let sorted = models
        .iter()
        .map(|(model, usage)| (model.clone(), usage))
        .collect::<BTreeMap<_, _>>();
    sorted
        .into_iter()
        .filter_map(|(model, usage)| {
            let tokens = parse_tokens(usage);
            has_usage(&tokens).then(|| (Some(model), tokens, complete_cost(usage)))
        })
        .collect()
}

fn parse_tokens(usage: &Value) -> TokenCounts {
    let input = first_i64(
        usage,
        &[
            "inputTokens",
            "input_tokens",
            "promptTokens",
            "prompt_tokens",
        ],
    );
    let output = first_i64(
        usage,
        &[
            "outputTokens",
            "output_tokens",
            "completionTokens",
            "completion_tokens",
        ],
    );
    TokenCounts {
        input_tokens: input,
        cached_input_tokens: first_i64(
            usage,
            &[
                "cachedReadTokens",
                "cachedInputTokens",
                "cached_input_tokens",
                "cache_read_input_tokens",
                "cached_prompt_tokens",
            ],
        ),
        cache_write_tokens: first_i64(
            usage,
            &[
                "cacheCreationTokens",
                "cacheWriteTokens",
                "cache_creation_tokens",
                "cache_creation_input_tokens",
                "cache_creation_prompt_tokens",
            ],
        ),
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: output,
        reasoning_tokens: first_i64(usage, &["reasoningTokens", "reasoning_tokens"]),
        total_tokens: first_i64(usage, &["totalTokens", "total_tokens"]),
    }
}

fn first_i64(value: &Value, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|key| value.get(*key))
        .map_or(0, |value| value_i64(Some(value)))
}

fn has_usage(tokens: &TokenCounts) -> bool {
    tokens.total_tokens > 0 || tokens.input_tokens > 0 || tokens.output_tokens > 0
}

fn complete_cost(usage: &Value) -> Option<i64> {
    if usage
        .get("costIsPartial")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || usage
            .get("incomplete")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return None;
    }
    usage
        .get("costUsdTicks")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
}

fn session_id(record: &Value, update: &Value) -> Option<String> {
    value_string(record.pointer("/params/sessionId"))
        .or_else(|| value_string(record.pointer("/params/session_id")))
        .or_else(|| value_string(update.get("sessionId")))
        .or_else(|| value_string(update.get("session_id")))
        .or_else(|| value_string(record.get("sessionId")))
        .or_else(|| value_string(record.get("session_id")))
}

fn project_name(record: &Value, update: &Value) -> Option<String> {
    value_string(update.get("cwd"))
        .or_else(|| value_string(record.pointer("/params/cwd")))
        .or_else(|| value_string(record.get("cwd")))
        .or_else(|| value_string(record.get("project")))
}

fn fallback_model(record: &Value, update: &Value, usage: &Value) -> Option<String> {
    value_string(usage.get("model"))
        .or_else(|| value_string(usage.get("modelId")))
        .or_else(|| value_string(update.get("model")))
        .or_else(|| value_string(update.get("modelId")))
        .or_else(|| value_string(record.pointer("/params/_meta/modelId")))
        .or_else(|| value_string(record.pointer("/_meta/modelId")))
}

fn turn_id(update: &Value) -> Option<String> {
    value_string(update.get("prompt_id"))
        .or_else(|| value_string(update.get("promptId")))
        .or_else(|| value_string(update.get("turn_id")))
        .or_else(|| value_string(update.get("turnId")))
        .or_else(|| value_string(update.get("id")))
}

fn event_timestamp(record: &Value, update: &Value) -> Option<DateTime<Utc>> {
    parse_timestamp(record.get("timestamp"))
        .or_else(|| parse_timestamp(update.get("timestamp")))
        .or_else(|| parse_timestamp(record.pointer("/params/timestamp")))
        .or_else(|| {
            [
                update.pointer("/_meta/agentTimestampMs"),
                record.pointer("/params/_meta/agentTimestampMs"),
                record.pointer("/_meta/agentTimestampMs"),
            ]
            .into_iter()
            .flatten()
            .find_map(timestamp_millis)
        })
}

fn timestamp_millis(value: &Value) -> Option<DateTime<Utc>> {
    let millis = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))?;
    DateTime::from_timestamp_millis(millis)
}

fn stable_component(parts: &[&str]) -> String {
    fallback_event_fingerprint(parts)
        .strip_prefix("fingerprint:")
        .unwrap_or("unknown")
        .to_owned()
}

#[derive(Default)]
struct PathContext {
    session_id: Option<String>,
    project: Option<String>,
}

fn path_context(path: &Path) -> PathContext {
    if !path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("updates.jsonl"))
    {
        return PathContext::default();
    }
    let Some(session) = path.parent() else {
        return PathContext::default();
    };
    let Some(project) = session.parent() else {
        return PathContext::default();
    };
    let Some(sessions) = project.parent() else {
        return PathContext::default();
    };
    if sessions.file_name().and_then(|value| value.to_str()) != Some("sessions") {
        return PathContext::default();
    }
    PathContext {
        session_id: session
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_owned),
        project: project
            .file_name()
            .and_then(|value| value.to_str())
            .map(percent_decode),
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FIXTURE: &str = include_str!("../../tests/fixtures/grok.jsonl");

    #[test]
    fn fixture_counts_only_completed_turns_and_splits_models() {
        let output = parse_reader(Cursor::new(FIXTURE), "device");
        assert_eq!(output.events.len(), 4);
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.tokens.total_tokens)
                .sum::<i64>(),
            460
        );
        assert_eq!(output.events[0].model.as_deref(), Some("grok-4.5-build"));
        assert_eq!(output.events[0].tokens.input_tokens, 100);
        assert_eq!(output.events[0].tokens.cached_input_tokens, 40);
        assert_eq!(output.events[0].tokens.cache_write_tokens, 10);
        assert_eq!(output.events[0].tokens.output_tokens, 20);
        assert_eq!(output.events[0].tokens.reasoning_tokens, 8);
        assert_eq!(output.events[0].native_cost_usd_ticks, Some(120_000_000));
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
        assert!(
            output
                .diagnostics
                .iter()
                .any(|item| item.code == "missing_timestamp")
        );
    }

    #[test]
    fn replay_and_duplicate_completed_turns_are_stable() {
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
        assert_eq!(first.events.len(), 4);
    }

    #[test]
    fn file_path_supplies_session_and_sanitized_project_metadata() {
        let temporary = tempfile::tempdir().unwrap();
        let session = temporary
            .path()
            .join("sessions")
            .join("C%3A%5Cwork%5CArcMeter")
            .join("session-from-path");
        std::fs::create_dir_all(&session).unwrap();
        let path = session.join("updates.jsonl");
        std::fs::write(
            &path,
            r#"{"timestamp":"2026-08-20T12:00:00Z","params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"path-turn","usage":{"inputTokens":2,"outputTokens":1,"totalTokens":3,"modelUsage":{"future-grok-build":{"inputTokens":2,"outputTokens":1,"totalTokens":3}}}}}}"#,
        )
        .unwrap();
        let output = parse_file(&path, "device");
        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].native_session_id, "session-from-path");
        assert_eq!(output.events[0].project_name.as_deref(), Some("ArcMeter"));
        assert_eq!(output.events[0].model.as_deref(), Some("future-grok-build"));
    }

    #[test]
    fn partial_native_cost_is_unavailable_not_guessed() {
        let record = r#"{"timestamp":"2026-08-20T12:00:00Z","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","prompt_id":"p","usage":{"inputTokens":2,"outputTokens":1,"totalTokens":3,"costUsdTicks":100,"costIsPartial":true}}}}"#;
        let output = parse_reader(Cursor::new(record), "device");
        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].native_cost_usd_ticks, None);
    }
}
