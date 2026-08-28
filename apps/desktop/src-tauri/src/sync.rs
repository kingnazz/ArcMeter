use crate::auth::{self, SupabaseConfig};
use crate::db::Database;
use chrono::{DateTime, Utc};
use reqwest::{Client, Response};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use thiserror::Error;

const BATCH_SIZE: i64 = 250;
const PULL_PAGE_SIZE: usize = 1_000;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("{0}")]
    Auth(#[from] auth::AuthError),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Supabase returned HTTP {0}")]
    Remote(reqwest::StatusCode),
    #[error("invalid remote metadata: {0}")]
    InvalidRemote(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub uploaded_events: usize,
    pub downloaded_events: usize,
    pub downloaded_devices: usize,
    pub synced_subscriptions: usize,
    pub completed_at: DateTime<Utc>,
}

pub async fn sync_now(database: &Database) -> Result<SyncReport, SyncError> {
    let config = SupabaseConfig::load()?;
    let session = auth::valid_session().await?;
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let mut report = SyncReport {
        uploaded_events: 0,
        downloaded_events: 0,
        downloaded_devices: 0,
        synced_subscriptions: 0,
        completed_at: Utc::now(),
    };

    // Bound the pull with a cursor captured before the request. Advancing to the
    // completion time would skip rows committed while this sync is in flight.
    let remote_cursor = Utc::now();
    let (devices, events, subscriptions) = pull_remote(
        &client,
        &config,
        &session.access_token,
        database,
        remote_cursor,
    )
    .await?;
    report.downloaded_devices = apply_remote_devices(database, &devices)?;
    report.downloaded_events = apply_remote_events(database, &events)?;
    report.synced_subscriptions += apply_remote_subscriptions(database, &subscriptions)?;

    upload_device(&client, &config, &session.access_token, database).await?;
    loop {
        let batch = pending_events(database, BATCH_SIZE)?;
        if batch.is_empty() {
            break;
        }
        post_upsert(
            &client,
            &config,
            &session.access_token,
            "usage_events",
            &batch,
        )
        .await?;
        let ids = batch
            .iter()
            .filter_map(|event| event.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        mark_events_synced(database, &ids)?;
        report.uploaded_events += ids.len();
        if batch.len() < BATCH_SIZE as usize {
            break;
        }
    }
    let pending_subscriptions = pending_subscriptions(database)?;
    if !pending_subscriptions.is_empty() {
        post_upsert(
            &client,
            &config,
            &session.access_token,
            "subscriptions",
            &pending_subscriptions,
        )
        .await?;
        mark_subscriptions_synced(database)?;
        report.synced_subscriptions += pending_subscriptions.len();
    }
    report.completed_at = Utc::now();
    mark_sync_complete(database, remote_cursor, report.completed_at)?;
    Ok(report)
}

async fn pull_remote(
    client: &Client,
    config: &SupabaseConfig,
    access_token: &str,
    database: &Database,
    remote_cursor: DateTime<Utc>,
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>), SyncError> {
    let last_sync = Connection::open(database.path())?
        .query_row(
            "SELECT value FROM sync_state WHERE key = 'last_remote_sync'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
    let devices = get_rows(
        client,
        config,
        access_token,
        "devices",
        "select=*&order=updated_at.asc,id.asc",
    )
    .await?;
    let encoded = last_sync.replace(':', "%3A").replace('+', "%2B");
    let encoded_cursor = remote_cursor
        .to_rfc3339()
        .replace(':', "%3A")
        .replace('+', "%2B");
    let events = get_rows(
        client,
        config,
        access_token,
        "usage_events",
        &format!(
            "select=*&updated_at=gt.{encoded}&updated_at=lte.{encoded_cursor}&order=updated_at.asc,id.asc"
        ),
    )
    .await?;
    let subscriptions = get_rows(
        client,
        config,
        access_token,
        "subscriptions",
        "select=*&order=updated_at.asc,id.asc",
    )
    .await?;
    Ok((devices, events, subscriptions))
}

async fn get_rows(
    client: &Client,
    config: &SupabaseConfig,
    access_token: &str,
    table: &str,
    query: &str,
) -> Result<Vec<Value>, SyncError> {
    let mut output = Vec::new();
    loop {
        let first = output.len();
        let last = first.saturating_add(PULL_PAGE_SIZE).saturating_sub(1);
        let response = client
            .get(format!("{}/rest/v1/{table}?{query}", config.url))
            .header("apikey", &config.client_key)
            .header("Range-Unit", "items")
            .header("Range", format!("{first}-{last}"))
            .bearer_auth(access_token)
            .send()
            .await?;
        let page: Vec<Value> = ensure_success(response).await?.json().await?;
        let page_len = page.len();
        output.extend(page);
        if page_len < PULL_PAGE_SIZE {
            return Ok(output);
        }
    }
}

async fn upload_device(
    client: &Client,
    config: &SupabaseConfig,
    token: &str,
    database: &Database,
) -> Result<(), SyncError> {
    let device = database
        .device()
        .map_err(|error| SyncError::InvalidRemote(error.to_string()))?;
    let payload = vec![json!({
        "id": device.id,
        "friendly_name": device.friendly_name,
        "os": device.os,
        "architecture": device.architecture,
        "app_version": device.app_version,
        "created_at": device.created_at,
        "last_seen_at": Utc::now(),
        "last_sync_at": Utc::now()
    })];
    post_upsert(client, config, token, "devices", &payload).await
}

async fn post_upsert(
    client: &Client,
    config: &SupabaseConfig,
    access_token: &str,
    table: &str,
    payload: &[Value],
) -> Result<(), SyncError> {
    let response = client
        .post(format!(
            "{}/rest/v1/{table}?on_conflict={}",
            config.url,
            if table == "subscriptions" {
                "user_id,id"
            } else {
                "id"
            }
        ))
        .header("apikey", &config.client_key)
        .header(
            "Prefer",
            if table == "usage_events" {
                "resolution=ignore-duplicates,return=minimal"
            } else {
                "resolution=merge-duplicates,return=minimal"
            },
        )
        .bearer_auth(access_token)
        .json(payload)
        .send()
        .await?;
    ensure_success(response).await?;
    Ok(())
}

async fn ensure_success(response: Response) -> Result<Response, SyncError> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(SyncError::Remote(response.status()))
    }
}

fn pending_events(database: &Database, limit: i64) -> Result<Vec<Value>, SyncError> {
    let connection = Connection::open(database.path())?;
    let mut statement = connection.prepare(
        "SELECT id, device_id, provider, source, source_type, native_session_id, native_event_id, occurred_at,
                model, project_name, input_tokens, cached_input_tokens, output_tokens, reasoning_tokens,
                total_tokens, estimated_api_value_usd_micros, pricing_status, measurement_kind, created_at, updated_at
         FROM usage_events WHERE sync_status IN ('pending', 'error') ORDER BY created_at LIMIT ?1",
    )?;
    let rows = statement.query_map([limit], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?, "device_id": row.get::<_, String>(1)?,
            "provider": row.get::<_, String>(2)?, "source": row.get::<_, String>(3)?,
            "source_type": row.get::<_, String>(4)?, "native_session_id": row.get::<_, String>(5)?,
            "native_event_id": row.get::<_, String>(6)?, "occurred_at": row.get::<_, String>(7)?,
            "model": row.get::<_, Option<String>>(8)?, "project_name": row.get::<_, Option<String>>(9)?,
            "input_tokens": row.get::<_, i64>(10)?, "cached_input_tokens": row.get::<_, i64>(11)?,
            "output_tokens": row.get::<_, i64>(12)?, "reasoning_tokens": row.get::<_, i64>(13)?,
            "total_tokens": row.get::<_, i64>(14)?, "estimated_api_value_usd_micros": row.get::<_, Option<i64>>(15)?,
            "pricing_status": row.get::<_, String>(16)?, "measurement_kind": row.get::<_, String>(17)?,
            "created_at": row.get::<_, String>(18)?, "updated_at": row.get::<_, String>(19)?
        }))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn pending_subscriptions(database: &Database) -> Result<Vec<Value>, SyncError> {
    let connection = Connection::open(database.path())?;
    let mut statement = connection.prepare(
        "SELECT id, provider, plan_name, monthly_price_usd_cents, billing_cadence, active, created_at, updated_at
         FROM subscriptions WHERE sync_status IN ('pending', 'error')",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?, "provider": row.get::<_, String>(1)?,
            "plan_name": row.get::<_, String>(2)?, "monthly_price_usd_cents": row.get::<_, i64>(3)?,
            "billing_cadence": row.get::<_, String>(4)?, "active": row.get::<_, bool>(5)?,
            "created_at": row.get::<_, String>(6)?, "updated_at": row.get::<_, String>(7)?
        }))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn apply_remote_devices(database: &Database, rows: &[Value]) -> Result<usize, SyncError> {
    let local_device_id = database
        .device()
        .map_err(|error| SyncError::InvalidRemote(error.to_string()))?
        .id;
    let mut connection = Connection::open(database.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut applied = 0;
    for row in rows {
        let Some(id) = string(row, "id") else {
            continue;
        };
        if id == local_device_id {
            continue;
        }
        applied += transaction.execute(
            "INSERT INTO devices(id, friendly_name, os, architecture, app_version, created_at, last_seen_at, last_sync_at, sync_status)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'synced')
             ON CONFLICT(id) DO UPDATE SET friendly_name=excluded.friendly_name, os=excluded.os,
               architecture=excluded.architecture, app_version=excluded.app_version, last_seen_at=excluded.last_seen_at,
               last_sync_at=excluded.last_sync_at, sync_status='synced'",
            params![id, string(row, "friendly_name").unwrap_or("ArcMeter device"), string(row, "os").unwrap_or("unknown"),
                string(row, "architecture").unwrap_or("unknown"), string(row, "app_version").unwrap_or("unknown"),
                string(row, "created_at").unwrap_or("1970-01-01T00:00:00Z"), string(row, "last_seen_at").unwrap_or("1970-01-01T00:00:00Z"),
                row.get("last_sync_at").and_then(Value::as_str)],
        )?;
    }
    transaction.commit()?;
    Ok(applied)
}

fn apply_remote_events(database: &Database, rows: &[Value]) -> Result<usize, SyncError> {
    let mut connection = Connection::open(database.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut applied = 0;
    for row in rows {
        let Some(id) = string(row, "id") else {
            continue;
        };
        if id.len() != 64
            || row
                .get("project_name")
                .and_then(Value::as_str)
                .is_some_and(|value| value.contains(['/', '\\']))
        {
            continue;
        }
        applied += transaction.execute(
            "INSERT INTO usage_events(id, provider, source, source_type, native_session_id, native_event_id, occurred_at,
               model, project_name, input_tokens, cached_input_tokens, output_tokens, reasoning_tokens, total_tokens,
               estimated_api_value_usd_micros, pricing_status, measurement_kind, device_id, created_at, updated_at, sync_status)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,'synced')
             ON CONFLICT(id) DO NOTHING",
            params![id, string(row,"provider"), string(row,"source"), string(row,"source_type"), string(row,"native_session_id"),
              string(row,"native_event_id"), string(row,"occurred_at"), row.get("model").and_then(Value::as_str),
              row.get("project_name").and_then(Value::as_str), integer(row,"input_tokens"), integer(row,"cached_input_tokens"),
              integer(row,"output_tokens"), integer(row,"reasoning_tokens"), integer(row,"total_tokens"),
              row.get("estimated_api_value_usd_micros").and_then(Value::as_i64), string(row,"pricing_status"),
              string(row,"measurement_kind"), string(row,"device_id"), string(row,"created_at"), string(row,"updated_at")],
        )?;
    }
    transaction.commit()?;
    Ok(applied)
}

fn apply_remote_subscriptions(database: &Database, rows: &[Value]) -> Result<usize, SyncError> {
    let mut connection = Connection::open(database.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut applied = 0;
    for row in rows {
        let Some(id) = string(row, "id") else {
            continue;
        };
        applied += transaction.execute(
            "INSERT INTO subscriptions(id, provider, plan_name, monthly_price_usd_cents, billing_cadence, active, created_at, updated_at, sync_status)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'synced')
             ON CONFLICT(id) DO UPDATE SET provider=excluded.provider, plan_name=excluded.plan_name,
               monthly_price_usd_cents=excluded.monthly_price_usd_cents, billing_cadence=excluded.billing_cadence,
               active=excluded.active, updated_at=excluded.updated_at, sync_status='synced'
             WHERE excluded.updated_at > subscriptions.updated_at",
            params![id, string(row,"provider"), string(row,"plan_name"), integer(row,"monthly_price_usd_cents"),
              string(row,"billing_cadence"), row.get("active").and_then(Value::as_bool).unwrap_or(false),
              string(row,"created_at"), string(row,"updated_at")],
        )?;
    }
    transaction.commit()?;
    Ok(applied)
}

fn mark_events_synced(database: &Database, ids: &[&str]) -> Result<(), SyncError> {
    let mut connection = Connection::open(database.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for id in ids {
        transaction.execute(
            "UPDATE usage_events SET sync_status='synced', last_sync_error=NULL WHERE id=?1",
            [id],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn mark_subscriptions_synced(database: &Database) -> Result<(), SyncError> {
    Connection::open(database.path())?.execute(
        "UPDATE subscriptions SET sync_status='synced' WHERE sync_status IN ('pending','error')",
        [],
    )?;
    Ok(())
}

fn mark_sync_complete(
    database: &Database,
    remote_cursor: DateTime<Utc>,
    completed_at: DateTime<Utc>,
) -> Result<(), SyncError> {
    let connection = Connection::open(database.path())?;
    connection.execute(
        "INSERT INTO sync_state(key,value,updated_at) VALUES('last_remote_sync',?1,?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        [remote_cursor.to_rfc3339()],
    )?;
    connection.execute(
        "UPDATE devices SET last_sync_at=?1, sync_status='synced' WHERE id=(SELECT value FROM app_settings WHERE key='local_device_id')",
        [completed_at.to_rfc3339()],
    )?;
    Ok(())
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}
fn integer(value: &Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub base_seconds: u64,
    pub max_seconds: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            base_seconds: 2,
            max_seconds: 300,
        }
    }
}

impl RetryPolicy {
    pub fn delay(&self, attempt: u32) -> Duration {
        Duration::from_secs(
            self.base_seconds
                .saturating_mul(2_u64.saturating_pow(attempt.min(16)))
                .min(self.max_seconds),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{TokenCounts, UsageEvent};

    #[test]
    fn retry_is_exponential_and_capped() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.delay(0), Duration::from_secs(2));
        assert_eq!(policy.delay(3), Duration::from_secs(16));
        assert_eq!(policy.delay(20), Duration::from_secs(300));
    }

    #[test]
    fn remote_event_upsert_is_idempotent_and_local_queue_survives_offline() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("sync.db")).unwrap();
        let device = database.ensure_device("test").unwrap();
        let local = UsageEvent::measured(
            "codex",
            "codex_cli",
            "local-session",
            "local-event",
            Utc::now(),
            Some("gpt-5.6-sol".into()),
            Some("ArcMeter".into()),
            TokenCounts {
                input_tokens: 10,
                output_tokens: 2,
                total_tokens: 12,
                ..Default::default()
            },
            device.id.clone(),
        );
        database.insert_usage_events(&[local]).unwrap();
        assert_eq!(pending_events(&database, 250).unwrap().len(), 1);

        let timestamp = Utc::now().to_rfc3339();
        let remote = json!({
            "id": "b".repeat(64),
            "device_id": device.id,
            "provider": "claude",
            "source": "claude_code",
            "source_type": "local_cli",
            "native_session_id": "remote-session",
            "native_event_id": "remote-event",
            "occurred_at": timestamp,
            "model": "claude-sonnet-5",
            "project_name": "ArcMeter",
            "input_tokens": 20,
            "cached_input_tokens": 5,
            "output_tokens": 4,
            "reasoning_tokens": 0,
            "total_tokens": 24,
            "estimated_api_value_usd_micros": 65,
            "pricing_status": "available",
            "measurement_kind": "measured",
            "created_at": timestamp,
            "updated_at": timestamp
        });
        assert_eq!(
            apply_remote_events(&database, std::slice::from_ref(&remote)).unwrap(),
            1
        );
        assert_eq!(apply_remote_events(&database, &[remote]).unwrap(), 0);

        let connection = Connection::open(database.path()).unwrap();
        let (events, tokens): (i64, i64) = connection
            .query_row(
                "SELECT COUNT(*), SUM(total_tokens) FROM usage_events",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((events, tokens), (2, 36));
    }
}
