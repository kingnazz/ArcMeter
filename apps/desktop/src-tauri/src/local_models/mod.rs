//! Local inference metadata. Inventory and runtime observations never enter cloud sync.
pub mod omlx;
pub mod providers;

use crate::db::{Database, DatabaseError, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModel {
    pub id: String,
    pub name: String,
    pub loaded: Option<bool>,
    pub context_length: Option<i64>,
    pub quantization: Option<String>,
    pub parameters: Option<String>,
    pub size_bytes: Option<i64>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub last_activity_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRuntime {
    pub provider: String,
    pub enabled: bool,
    pub endpoint: String,
    pub automatic: bool,
    pub status: String,
    pub checked_at: Option<String>,
    pub detected_at: Option<String>,
    pub diagnostic: String,
    pub models: Vec<LocalModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelsSnapshot {
    pub runtimes: Vec<LocalRuntime>,
    pub measured_tokens_today: i64,
    pub requests_today: Option<i64>,
    pub observations_today: i64,
    pub history_status: String,
}

pub fn runtimes(database: &Database) -> Result<Vec<LocalRuntime>> {
    let connection = Connection::open(database.path())?;
    let mut statement = connection.prepare("SELECT provider, enabled, endpoint_override, status, checked_at,
        detected_at, diagnostic, models_json FROM local_runtime_state ORDER BY CASE provider WHEN 'omlx' THEN 0 WHEN 'ollama' THEN 1 ELSE 2 END")?;
    let rows = statement.query_map([], |row| {
        let provider: String = row.get(0)?;
        let address: Option<String> = row.get(2)?;
        let checked_at: Option<String> = row.get(4)?;
        let mut status: String = row.get(3)?;
        if status == "healthy"
            && !checked_at
                .as_ref()
                .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
                .is_some_and(|t| Utc::now().signed_duration_since(t).num_seconds() <= 120)
        {
            status = "stale".into();
        }
        Ok(LocalRuntime {
            endpoint: address.clone().unwrap_or_else(|| {
                providers::default_endpoint(&provider)
                    .unwrap_or_default()
                    .into()
            }),
            automatic: address.is_none(),
            provider,
            enabled: row.get(1)?,
            status,
            checked_at,
            detected_at: row.get(5)?,
            diagnostic: row.get(6)?,
            models: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub fn configure(
    database: &Database,
    provider: &str,
    enabled: bool,
    address: Option<&str>,
) -> Result<()> {
    if !providers::PROVIDERS.contains(&provider) {
        return Err(DatabaseError::Invalid("Unsupported local runtime".into()));
    }
    let address = address.map(str::trim).filter(|v| !v.is_empty());
    if let Some(address) = address {
        providers::endpoint(address).map_err(|m| DatabaseError::Invalid(m.into()))?;
    }
    // An endpoint change invalidates live state immediately. Historical usage is retained.
    Connection::open(database.path())?.execute("UPDATE local_runtime_state SET enabled=?2,
        endpoint_override=?3, status=CASE WHEN ?2 THEN 'not_detected' ELSE 'disabled' END,
        models_json='[]', checked_at=NULL, detected_at=NULL, diagnostic='Configuration saved; awaiting detection' WHERE provider=?1",
        params![provider, enabled, address])?;
    Ok(())
}

pub fn save_detection(
    database: &Database,
    previous: &LocalRuntime,
    detected: std::result::Result<Vec<LocalModel>, &str>,
    device_id: &str,
) -> Result<()> {
    let now = Utc::now();
    let (status, message, mut models) = match detected {
        Ok(models) => ("healthy", "Detected · read-only connection", models),
        Err(message) => ("unavailable", message, previous.models.clone()),
    };
    for m in &mut models {
        if let Some(old) = previous.models.iter().find(|old| old.id == m.id) {
            m.first_seen = old.first_seen;
        }
        if status != "healthy" {
            m.loaded = None;
        }
    }
    let mut connection = Connection::open(database.path())?;
    let transaction = connection.transaction()?;
    // The caller serializes collection/configuration. One transition, not one event per poll.
    let mut observations: Vec<(Option<String>, &str)> = Vec::new();
    if previous.status != status && (status == "healthy" || previous.status == "healthy") {
        observations.push((
            None,
            if status == "healthy" {
                "server_online"
            } else {
                "server_unavailable"
            },
        ));
    }
    if status == "healthy" {
        for m in &models {
            let was = previous
                .models
                .iter()
                .find(|old| old.id == m.id)
                .and_then(|m| m.loaded);
            if m.loaded == Some(true) && was != Some(true) {
                observations.push((Some(m.id.clone()), "model_loaded"));
            }
            if m.loaded == Some(false) && was == Some(true) {
                observations.push((Some(m.id.clone()), "model_unloaded"));
            }
        }
        for old in previous.models.iter().filter(|m| m.loaded == Some(true)) {
            if !models.iter().any(|m| m.id == old.id) {
                observations.push((Some(old.id.clone()), "model_unloaded"));
            }
        }
    }
    for (model, state) in observations {
        let id = hex::encode(Sha256::digest(format!(
            "{}:{device_id}:{state}:{model:?}:{now}",
            previous.provider
        )));
        transaction.execute("INSERT OR IGNORE INTO local_runtime_observations(id, provider, model, state, observed_at, device_id)
            VALUES(?1,?2,?3,?4,?5,?6)", params![id, previous.provider, model, state, now.to_rfc3339(), device_id])?;
    }
    transaction.execute("UPDATE local_runtime_state SET status=?2, checked_at=?3,
        detected_at=CASE WHEN ?2='healthy' THEN ?3 ELSE detected_at END, diagnostic=?4, models_json=?5 WHERE provider=?1",
        params![previous.provider, status, now.to_rfc3339(), message,
            serde_json::to_string(&models).unwrap_or_else(|_| "[]".into())])?;
    transaction.execute("DELETE FROM local_runtime_observations WHERE observed_at < datetime('now','-90 days')
        OR id IN (SELECT id FROM local_runtime_observations ORDER BY observed_at DESC LIMIT -1 OFFSET 10000)", [])?;
    transaction.commit()?;
    Ok(())
}

pub async fn refresh(database: &Database, device_id: &str) -> Result<()> {
    for runtime in runtimes(database)? {
        if !runtime.enabled {
            continue;
        }
        let detected = providers::detect(&runtime.provider, &runtime.endpoint).await;
        save_detection(database, &runtime, detected, device_id)?;
    }
    let database = database.clone();
    let device_id = device_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let enabled = runtimes(&database)?
            .iter()
            .any(|r| r.provider == "omlx" && r.enabled);
        if enabled {
            omlx::collect(&database, &device_id)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| DatabaseError::Invalid("Local history worker unavailable".into()))?
}

pub fn snapshot(database: &Database) -> Result<LocalModelsSnapshot> {
    let connection = Connection::open(database.path())?;
    let device_id = database.device()?.id;
    let start = crate::analytics::range_start("today", Utc::now()).to_rfc3339();
    let tokens = connection.query_row(
        "SELECT COALESCE(SUM(total_tokens),0) FROM usage_events
        WHERE provider IN ('omlx','ollama','lmstudio') AND measurement_kind='measured'
        AND superseded_by_event_id IS NULL AND occurred_at>=?1 AND device_id=?2",
        params![&start, device_id],
        |r| r.get(0),
    )?;
    let requests: i64 = connection.query_row("SELECT COALESCE(SUM(l.requests),0) FROM local_usage_details l
        JOIN usage_events u ON u.id=l.event_id WHERE occurred_at>=?1 AND superseded_by_event_id IS NULL", [&start], |r| r.get(0))?;
    let observations = connection.query_row(
        "SELECT COUNT(*) FROM local_runtime_observations WHERE observed_at>=?1",
        [&start],
        |r| r.get(0),
    )?;
    let history_status = database
        .setting("local_omlx_history_status")?
        .unwrap_or_else(|| "not_checked".into());
    Ok(LocalModelsSnapshot {
        runtimes: runtimes(database)?,
        measured_tokens_today: tokens,
        requests_today: (history_status == "available").then_some(requests),
        observations_today: observations,
        history_status,
    })
}

#[cfg(test)]
mod tests;
