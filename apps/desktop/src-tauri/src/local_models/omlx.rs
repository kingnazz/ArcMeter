//! oMLX schema v1 hourly usage is the only token source here. Application logs
//! contain messages and optional ambiguous `tokens`; they are never ingested.
use crate::db::{Database, Result};
use crate::domain::{SourceType, TokenCounts, UsageEvent};
use chrono::{DateTime, Utc};
use directories::UserDirs;
use rusqlite::{Connection, OpenFlags, params};
use std::path::Path;

pub const SOURCE: &str = "omlx_usage_hourly";

pub fn collect(database: &Database, device: &str) -> Result<()> {
    let base = std::env::var_os("OMLX_BASE_PATH")
        .map(std::path::PathBuf::from)
        .or_else(|| UserDirs::new().map(|d| d.home_dir().join(".omlx")));
    let status = match base {
        Some(base) if base.join("usage.sqlite3").is_file() => {
            match collect_path(database, device, &base.join("usage.sqlite3")) {
                Ok(()) => "available",
                Err(_) => "unavailable",
            }
        }
        _ => "not_found",
    };
    // No source path or underlying SQLite error reaches diagnostics or sync.
    Connection::open(database.path())?.execute("INSERT INTO app_settings(key,value,updated_at)
        VALUES('local_omlx_history_status',?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        params![status, Utc::now().to_rfc3339()])?;
    Ok(())
}

pub fn collect_path(database: &Database, device: &str, path: &Path) -> Result<()> {
    let source = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    source.busy_timeout(std::time::Duration::from_millis(250))?;
    let version: i64 = source.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version != 1 {
        return Err(crate::db::DatabaseError::Invalid(
            "Unsupported oMLX history version".into(),
        ));
    }
    let mut statement = source.prepare(
        "SELECT timestamp_hour, model_id, requests, prompt_tokens, completion_tokens, cached_tokens
        FROM model_usage_hourly ORDER BY timestamp_hour, model_id LIMIT 250001",
    )?;
    let mut rows = statement.query([])?;
    let mut target = Connection::open(database.path())?;
    target.busy_timeout(std::time::Duration::from_secs(5))?;
    let transaction = target.transaction()?;
    let now = Utc::now();
    let mut seen = 0;
    while let Some(row) = rows.next()? {
        seen += 1;
        if seen > 250000 {
            return Err(crate::db::DatabaseError::Invalid(
                "oMLX history exceeds the safe limit".into(),
            ));
        }
        let hour: i64 = row.get(0)?;
        let Some(time) = DateTime::from_timestamp(hour, 0).filter(|t| *t <= now && hour >= 0)
        else {
            continue;
        };
        let raw_model: String = row.get(1)?;
        let Some(model) = super::providers::identifier(&raw_model) else {
            continue;
        };
        let requests: i64 = row.get(2)?;
        let input: i64 = row.get(3)?;
        let output: i64 = row.get(4)?;
        let cached: i64 = row.get(5)?;
        if requests < 0 || input < 0 || output < 0 || cached < 0 || cached > input {
            continue;
        }
        let Some(total) = input.checked_add(output) else {
            continue;
        };
        // This is a bucket identity, never a fabricated session or request identity.
        let mut event = UsageEvent::measured(
            "omlx",
            SOURCE,
            crate::domain::deterministic_event_id("omlx-hour", device, &model),
            hour.to_string(),
            time,
            Some(model),
            None,
            TokenCounts {
                input_tokens: input,
                output_tokens: output,
                cached_input_tokens: cached,
                total_tokens: total,
                ..Default::default()
            },
            device,
        );
        event.source_type = SourceType::Api;
        transaction.execute("INSERT INTO usage_events(id,provider,source,source_type,native_session_id,native_event_id,
            occurred_at,model,input_tokens,output_tokens,cached_input_tokens,total_tokens,measurement_kind,device_id,created_at,updated_at)
            VALUES(?1,'omlx',?2,'api',?3,?4,?5,?6,?7,?8,?9,?10,'measured',?11,?12,?12)
            ON CONFLICT(id) DO UPDATE SET input_tokens=excluded.input_tokens,output_tokens=excluded.output_tokens,
            cached_input_tokens=excluded.cached_input_tokens,total_tokens=excluded.total_tokens,updated_at=excluded.updated_at,sync_status='pending'
            WHERE excluded.input_tokens>=usage_events.input_tokens AND excluded.output_tokens>=usage_events.output_tokens
              AND excluded.cached_input_tokens>=usage_events.cached_input_tokens
              AND (excluded.total_tokens>usage_events.total_tokens OR excluded.cached_input_tokens>usage_events.cached_input_tokens)",
            params![event.id, SOURCE, event.native_session_id, event.native_event_id, time.to_rfc3339(), event.model,
                input, output, cached, total, device, now.to_rfc3339()])?;
        transaction.execute(
            "INSERT INTO local_usage_details(event_id,requests) VALUES(?1,?2)
            ON CONFLICT(event_id) DO UPDATE SET requests=MAX(requests,excluded.requests)",
            params![event.id, requests],
        )?;
    }
    transaction.commit()?;
    Ok(())
}
