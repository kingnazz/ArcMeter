pub mod claude;
pub mod codex;
pub mod gemini;
pub mod grok;

use crate::db::{CollectorState, Database, DatabaseError};
use crate::domain::{CollectorDiagnostic, CollectorOutput};
use chrono::{DateTime, Utc};
use directories::UserDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const DEFAULT_PARSER_VERSION: i64 = 1;
const GROK_PARSER_VERSION: i64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScanResult {
    pub provider: String,
    pub label: String,
    pub detected: bool,
    pub files_seen: usize,
    pub records_seen: usize,
    pub records_inserted: usize,
    pub measured_records: i64,
    pub measured_sessions: i64,
    pub measured_turns: i64,
    pub measured_tokens: i64,
    pub native_cost_usd_ticks: Option<i64>,
    pub last_scan_at: DateTime<Utc>,
    pub last_usage_at: Option<DateTime<Utc>>,
    pub status: String,
    pub diagnostics: Vec<CollectorDiagnostic>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub sources: Vec<SourceScanResult>,
    pub total_inserted: usize,
}

pub fn scan_all(database: &Database, device_id: &str) -> ScanReport {
    let specifications = [
        CollectorSpec::new("codex", "Codex", codex_roots(), &["jsonl"]),
        CollectorSpec::new("claude", "Claude Code CLI", claude_roots(), &["jsonl"]),
        CollectorSpec::new("grok", "Grok Build", grok_roots(), &["jsonl"]),
        CollectorSpec::new("gemini", "Gemini CLI", gemini_roots(), &["jsonl", "json"]),
    ];

    let mut report = ScanReport::default();
    for specification in specifications {
        let result = scan_provider(database, device_id, specification);
        report.total_inserted += result.records_inserted;
        report.sources.push(result);
    }
    let _ = crate::pricing::reprice_events(database);
    report
}

struct CollectorSpec {
    provider: &'static str,
    label: &'static str,
    roots: Vec<PathBuf>,
    extensions: &'static [&'static str],
}

impl CollectorSpec {
    fn new(
        provider: &'static str,
        label: &'static str,
        roots: Vec<PathBuf>,
        extensions: &'static [&'static str],
    ) -> Self {
        Self {
            provider,
            label,
            roots,
            extensions,
        }
    }
}

fn scan_provider(
    database: &Database,
    device_id: &str,
    specification: CollectorSpec,
) -> SourceScanResult {
    let now = Utc::now();
    let parser_version = parser_version(specification.provider);
    let files = if specification.provider == "grok" {
        discover_grok_files(&specification.roots)
    } else {
        discover_files(&specification.roots, specification.extensions)
    };
    let detected = specification.roots.iter().any(|root| root.exists());
    let mut diagnostics = Vec::new();
    let mut inserted = 0;
    let mut records_seen = 0;
    let mut last_usage_at: Option<DateTime<Utc>> = None;

    for path in &files {
        let Ok(metadata) = fs::metadata(path) else {
            diagnostics.push(diagnostic(
                "warning",
                "metadata_unavailable",
                "A source file changed during scanning",
                None,
            ));
            continue;
        };
        let modified_at_ms = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as i64)
            .unwrap_or_default();
        let file_size = metadata.len() as i64;
        let source_key = safe_path_key(specification.provider, path);
        let fingerprint = safe_file_fingerprint(path, file_size, modified_at_ms);
        let unchanged = database
            .collector_state(&source_key)
            .ok()
            .flatten()
            .is_some_and(|state| {
                state.file_size == file_size
                    && state.modified_at_ms == modified_at_ms
                    && state.parser_version == parser_version
            });
        if unchanged {
            continue;
        }

        let output = parse_path(specification.provider, path, device_id);
        records_seen += output.records_seen;
        diagnostics.extend(output.diagnostics.clone());
        if let Some(latest) = output.events.iter().map(|event| event.occurred_at).max() {
            last_usage_at = Some(last_usage_at.map_or(latest, |current| current.max(latest)));
        }
        let mut persistence_succeeded = true;
        match database.insert_usage_events(&output.events) {
            Ok(count) => {
                inserted += count;
                if specification.provider == "grok"
                    && let Err(error) = database.reconcile_grok_events(&output.events)
                {
                    persistence_succeeded = false;
                    diagnostics.push(diagnostic(
                        "error",
                        "grok_reconciliation_failed",
                        &error.to_string(),
                        None,
                    ));
                }
            }
            Err(error) => {
                persistence_succeeded = false;
                diagnostics.push(diagnostic(
                    "error",
                    "database_write_failed",
                    &error.to_string(),
                    None,
                ));
            }
        }

        let status = if diagnostics.iter().any(|item| item.severity == "error") {
            "error"
        } else if diagnostics.is_empty() {
            "healthy"
        } else {
            "warning"
        };
        let summary = diagnostics
            .first()
            .map(|item| item.message.chars().take(240).collect::<String>());
        let state = CollectorState {
            source_key,
            provider: specification.provider.into(),
            safe_file_fingerprint: fingerprint,
            file_size,
            modified_at_ms,
            last_processed_offset: file_size,
            parser_version,
            last_scan_at: now,
            last_usage_at: output.events.iter().map(|event| event.occurred_at).max(),
            status: status.into(),
            diagnostic: summary,
        };
        if persistence_succeeded && let Err(error) = database.save_collector_state(&state) {
            diagnostics.push(diagnostic(
                "error",
                "state_write_failed",
                &error.to_string(),
                None,
            ));
        }
    }

    let summary = provider_summary(database, specification.provider).unwrap_or_default();
    if specification.provider == "grok" && summary.unreconciled_legacy_records > 0 {
        diagnostics.push(diagnostic(
            "warning",
            "legacy_grok_reconciliation_required",
            "Legacy Grok measurements remain active because ArcMeter could not map them uniquely to completed turns",
            None,
        ));
    }
    let status = if diagnostics.iter().any(|item| item.severity == "error") {
        "error"
    } else if diagnostics.is_empty() {
        "healthy"
    } else {
        "warning"
    };
    SourceScanResult {
        provider: specification.provider.into(),
        label: specification.label.into(),
        detected,
        files_seen: files.len(),
        records_seen,
        records_inserted: inserted,
        measured_records: summary.records,
        measured_sessions: summary.sessions,
        measured_turns: summary.turns,
        measured_tokens: summary.tokens,
        native_cost_usd_ticks: summary.native_cost_usd_ticks,
        last_scan_at: now,
        last_usage_at: summary.last_usage_at.or(last_usage_at),
        status: status.into(),
        diagnostics: diagnostics.into_iter().take(20).collect(),
    }
}

fn parser_version(provider: &str) -> i64 {
    if provider == "grok" {
        GROK_PARSER_VERSION
    } else {
        DEFAULT_PARSER_VERSION
    }
}

fn parse_path(provider: &str, path: &Path, device_id: &str) -> CollectorOutput {
    match provider {
        "codex" => codex::parse_file(path, device_id),
        "claude" => claude::parse_file(path, device_id),
        "grok" => grok::parse_file(path, device_id),
        "gemini" => gemini::parse_file(path, device_id),
        _ => CollectorOutput::default(),
    }
}

#[derive(Default)]
struct ProviderSummary {
    records: i64,
    sessions: i64,
    turns: i64,
    tokens: i64,
    native_cost_usd_ticks: Option<i64>,
    last_usage_at: Option<DateTime<Utc>>,
    unreconciled_legacy_records: i64,
}

fn provider_summary(database: &Database, provider: &str) -> crate::db::Result<ProviderSummary> {
    let connection = rusqlite::Connection::open(database.path())?;
    let (records, sessions, turns, tokens, native_cost_usd_ticks, last_usage_at, legacy) =
        connection
            .query_row(
                "SELECT COUNT(*), COUNT(DISTINCT native_session_id),
                    COUNT(DISTINCT CASE
                      WHEN provider = 'grok' AND native_event_id LIKE 'turn:%:model:%'
                      THEN substr(native_event_id, 1, instr(native_event_id, ':model:') - 1)
                      ELSE id END),
                    COALESCE(SUM(total_tokens), 0), SUM(native_cost_usd_ticks), MAX(occurred_at),
                    COALESCE(SUM(CASE WHEN provider = 'grok'
                      AND native_event_id NOT LIKE 'turn:%:model:%' THEN 1 ELSE 0 END), 0)
             FROM usage_events WHERE provider = ?1 AND measurement_kind = 'measured'
               AND superseded_by_event_id IS NULL",
                [provider],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(DatabaseError::from)?;
    let last_usage_at = last_usage_at
        .map(|value| {
            DateTime::parse_from_rfc3339(&value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|error| DatabaseError::Invalid(error.to_string()))
        })
        .transpose()?;
    Ok(ProviderSummary {
        records,
        sessions,
        turns,
        tokens,
        native_cost_usd_ticks,
        last_usage_at,
        unreconciled_legacy_records: legacy,
    })
}

fn discover_grok_files(session_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = BTreeMap::<String, PathBuf>::new();
    for sessions in session_roots.iter().filter(|path| path.is_dir()) {
        let Ok(projects) = fs::read_dir(sessions) else {
            continue;
        };
        for project in projects.flatten().filter(|entry| entry.path().is_dir()) {
            let Ok(session_entries) = fs::read_dir(project.path()) else {
                continue;
            };
            for session in session_entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
            {
                let updates = session.path().join("updates.jsonl");
                if updates.is_file() {
                    paths.insert(updates.to_string_lossy().to_lowercase(), updates);
                }
            }
        }
    }
    paths.into_values().collect()
}

fn discover_files(roots: &[PathBuf], extensions: &[&str]) -> Vec<PathBuf> {
    let mut paths = BTreeMap::<String, PathBuf>::new();
    let mut visited = HashSet::new();
    for root in roots {
        discover_recursive(root, extensions, &mut visited, &mut paths, 0);
    }
    paths.into_values().collect()
}

fn discover_recursive(
    path: &Path,
    extensions: &[&str],
    visited: &mut HashSet<PathBuf>,
    output: &mut BTreeMap<String, PathBuf>,
    depth: usize,
) {
    if depth > 12 || !path.exists() {
        return;
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical) {
        return;
    }
    if path.is_file() {
        let accepted = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                extensions
                    .iter()
                    .any(|candidate| value.eq_ignore_ascii_case(candidate))
            });
        if accepted {
            output.insert(path.to_string_lossy().to_lowercase(), path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        discover_recursive(&entry.path(), extensions, visited, output, depth + 1);
    }
}

fn codex_roots() -> Vec<PathBuf> {
    let mut roots = env_roots("CODEX_HOME")
        .into_iter()
        .flat_map(|root| [root.join("sessions"), root.join("archived_sessions")])
        .collect::<Vec<_>>();
    if let Some(home) = home_dir() {
        roots.push(home.join(".codex").join("sessions"));
        roots.push(home.join(".codex").join("archived_sessions"));
    }
    unique_roots(roots)
}

fn claude_roots() -> Vec<PathBuf> {
    let mut roots = env_roots("CLAUDE_CONFIG_DIR");
    if let Some(home) = home_dir() {
        roots.push(home.join(".claude").join("projects"));
    }
    unique_roots(roots)
}

fn grok_roots() -> Vec<PathBuf> {
    let mut roots = env_roots("GROK_HOME")
        .into_iter()
        .map(|root| root.join("sessions"))
        .collect::<Vec<_>>();
    if let Some(home) = home_dir() {
        roots.push(home.join(".grok").join("sessions"));
    }
    unique_roots(roots)
}

fn gemini_roots() -> Vec<PathBuf> {
    let mut roots = env_roots("GEMINI_DATA_DIR");
    if let Some(home) = home_dir() {
        roots.push(home.join(".gemini"));
    }
    unique_roots(roots)
}

fn env_roots(variable: &str) -> Vec<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .or_else(|| UserDirs::new().map(|directories| directories.home_dir().to_path_buf()))
}

fn unique_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots
        .into_iter()
        .fold(BTreeMap::<String, PathBuf>::new(), |mut output, path| {
            output.insert(path.to_string_lossy().to_lowercase(), path);
            output
        })
        .into_values()
        .collect()
}

fn safe_path_key(provider: &str, path: &Path) -> String {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    format!("{provider}:{}", hex::encode(digest))
}

fn safe_file_fingerprint(path: &Path, size: i64, modified: i64) -> String {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("source");
    let canonical = format!("{name}\u{1f}{size}\u{1f}{modified}");
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

pub(crate) fn diagnostic(
    severity: &str,
    code: &str,
    message: &str,
    record_number: Option<usize>,
) -> CollectorDiagnostic {
    CollectorDiagnostic {
        severity: severity.into(),
        code: code.into(),
        message: message.into(),
        record_number,
    }
}

pub(crate) fn parse_timestamp(value: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(serde_json::Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|date| date.with_timezone(&Utc))
}

pub(crate) fn value_i64(value: Option<&serde_json::Value>) -> i64 {
    value
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default()
        .max(0)
}

pub(crate) fn value_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_scan_keeps_persisted_last_usage() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("claude");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude.jsonl"),
            source.join("session.jsonl"),
        )
        .unwrap();
        let database = Database::open(temporary.path().join("arcmeter.db")).unwrap();
        let device = database.ensure_device("test").unwrap();

        let first = scan_provider(
            &database,
            &device.id,
            CollectorSpec::new(
                "claude",
                "Claude Code CLI",
                vec![source.clone()],
                &["jsonl"],
            ),
        );
        let second = scan_provider(
            &database,
            &device.id,
            CollectorSpec::new("claude", "Claude Code CLI", vec![source], &["jsonl"]),
        );

        assert!(first.last_usage_at.is_some());
        assert_eq!(second.records_inserted, 0);
        assert_eq!(second.last_usage_at, first.last_usage_at);
        assert_eq!(second.measured_records, first.measured_records);
        assert_eq!(second.measured_tokens, first.measured_tokens);
    }

    #[test]
    fn grok_discovery_accepts_only_session_update_streams() {
        let temporary = tempfile::tempdir().unwrap();
        let sessions = temporary.path().join("sessions");
        let valid = sessions.join("encoded-project").join("session-id");
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::write(valid.join("updates.jsonl"), "{}\n").unwrap();
        std::fs::write(valid.join("summary.json"), "{}").unwrap();
        std::fs::write(sessions.join("unified.jsonl"), "{}\n").unwrap();
        let unrelated = temporary.path().join("random").join("nested");
        std::fs::create_dir_all(&unrelated).unwrap();
        std::fs::write(unrelated.join("updates.jsonl"), "{}\n").unwrap();

        let files = discover_grok_files(&[sessions]);
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().and_then(|value| value.to_str()),
            Some("updates.jsonl")
        );
        assert_eq!(
            files[0]
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str()),
            Some("session-id")
        );
    }

    #[test]
    #[ignore = "reads the current user's real Codex history for an explicit end-to-end validation"]
    fn real_codex_snapshot_is_measured_and_idempotent() {
        let source_files = discover_files(&codex_roots(), &["jsonl"]);
        assert!(
            !source_files.is_empty(),
            "no real Codex JSONL session files were detected"
        );

        let temporary = tempfile::tempdir().expect("create temporary validation directory");
        let snapshot = temporary.path().join("codex-snapshot");
        std::fs::create_dir_all(&snapshot).expect("create snapshot directory");
        for (index, source) in source_files.iter().enumerate() {
            let destination = snapshot.join(format!("{index:06}.jsonl"));
            std::fs::copy(source, destination).expect("copy a read-only local snapshot");
        }

        let database = Database::open(temporary.path().join("arcmeter-validation.db"))
            .expect("open validation database");
        let device = database.ensure_device("test").expect("register device");
        let first = scan_provider(
            &database,
            &device.id,
            CollectorSpec::new("codex", "Codex", vec![snapshot.clone()], &["jsonl"]),
        );
        let first_totals = database
            .event_count_and_tokens()
            .expect("read measured totals");
        let second = scan_provider(
            &database,
            &device.id,
            CollectorSpec::new("codex", "Codex", vec![snapshot], &["jsonl"]),
        );
        let second_totals = database
            .event_count_and_tokens()
            .expect("read repeated totals");

        assert!(
            first.measured_records > 0,
            "no measured Codex records parsed"
        );
        assert!(first.measured_tokens > 0, "no measured Codex tokens parsed");
        assert_eq!(second.records_inserted, 0);
        assert_eq!(first_totals, second_totals);
        println!(
            "ARCMETER_REAL_CODEX files={} records={} tokens={} first_inserted={} second_inserted={}",
            source_files.len(),
            first_totals.0,
            first_totals.1,
            first.records_inserted,
            second.records_inserted
        );
    }
}
