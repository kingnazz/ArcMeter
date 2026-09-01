pub mod claude;

use crate::db::Database;
use chrono::{DateTime, TimeDelta, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const PROVIDER: &str = "claude";
const ENABLED_SETTING: &str = "claude_live_quota_enabled";
pub const POLL_INTERVAL: Duration = Duration::from_secs(300);
const MAX_BACKOFF_SECONDS: u64 = 3_600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowKind {
    Rolling,
    Weekly,
    ModelWeekly,
    Other,
}

impl QuotaWindowKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Rolling => "rolling",
            Self::Weekly => "weekly",
            Self::ModelWeekly => "model_weekly",
            Self::Other => "other",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "rolling" => Self::Rolling,
            "weekly" => Self::Weekly,
            "model_weekly" => Self::ModelWeekly,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuotaWindow {
    pub key: String,
    pub label: String,
    pub kind: QuotaWindowKind,
    pub scope: Option<String>,
    pub utilization_bps: i64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraUsage {
    pub enabled: bool,
    pub monthly_limit_minor: Option<i64>,
    pub used_credits_minor: Option<i64>,
    pub utilization_bps: Option<i64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaHealth {
    NotConfigured,
    CredentialUnavailable,
    PermissionDenied,
    ExpiredLogin,
    Forbidden,
    RateLimited,
    ProviderUnavailable,
    Offline,
    InvalidResponse,
    Healthy,
}

impl QuotaHealth {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::CredentialUnavailable => "credential_unavailable",
            Self::PermissionDenied => "permission_denied",
            Self::ExpiredLogin => "expired_login",
            Self::Forbidden => "forbidden",
            Self::RateLimited => "rate_limited",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::Offline => "offline",
            Self::InvalidResponse => "invalid_response",
            Self::Healthy => "healthy",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "credential_unavailable" => Self::CredentialUnavailable,
            "permission_denied" => Self::PermissionDenied,
            "expired_login" => Self::ExpiredLogin,
            "forbidden" => Self::Forbidden,
            "rate_limited" => Self::RateLimited,
            "provider_unavailable" => Self::ProviderUnavailable,
            "offline" => Self::Offline,
            "invalid_response" => Self::InvalidResponse,
            "healthy" => Self::Healthy,
            _ => Self::NotConfigured,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuotaState {
    pub provider: String,
    pub enabled: bool,
    pub status: QuotaHealth,
    pub message: String,
    pub stale: bool,
    pub windows: Vec<ProviderQuotaWindow>,
    pub extra_usage: Option<ExtraUsage>,
    pub observed_at: Option<DateTime<Utc>>,
    pub attempted_at: Option<DateTime<Utc>>,
    pub retry_at: Option<DateTime<Utc>>,
    pub source_device_id: Option<String>,
    pub source_device_name: Option<String>,
}

#[derive(Default)]
pub struct QuotaRuntime {
    refreshing: AtomicBool,
}

struct RefreshGuard<'a>(&'a AtomicBool);

impl Drop for RefreshGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl QuotaRuntime {
    fn try_begin(&self) -> Option<RefreshGuard<'_>> {
        self.refreshing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| RefreshGuard(&self.refreshing))
    }
}

pub fn is_enabled(database: &Database) -> bool {
    database
        .setting(ENABLED_SETTING)
        .ok()
        .flatten()
        .is_some_and(|value| value == "true")
}

pub fn set_enabled(database: &Database, enabled: bool) -> crate::db::Result<()> {
    database.set_setting(ENABLED_SETTING, if enabled { "true" } else { "false" })?;
    if !enabled {
        Connection::open(database.path())?.execute(
            "DELETE FROM provider_quota_refresh_state WHERE provider = ?1",
            [PROVIDER],
        )?;
    }
    Ok(())
}

pub async fn refresh_claude(database: &Database, runtime: &QuotaRuntime) -> ProviderQuotaState {
    if !is_enabled(database) {
        return load_state(database).unwrap_or_else(|_| empty_state(false));
    }
    if cooldown_active(database).unwrap_or(false) {
        return load_state(database).unwrap_or_else(|_| empty_state(true));
    }
    let Some(_guard) = runtime.try_begin() else {
        return load_state(database).unwrap_or_else(|_| empty_state(true));
    };
    let attempted_at = Utc::now();
    let credential = match claude::discover_credential() {
        Ok(value) => value,
        Err(error) => {
            let health = match error {
                claude::CredentialError::Unavailable | claude::CredentialError::Invalid => {
                    QuotaHealth::CredentialUnavailable
                }
                claude::CredentialError::PermissionDenied => QuotaHealth::PermissionDenied,
                claude::CredentialError::Expired => QuotaHealth::ExpiredLogin,
            };
            let retry_after = (health == QuotaHealth::PermissionDenied).then_some(3_600);
            let _ = record_failure(database, health, attempted_at, retry_after);
            return load_state(database).unwrap_or_else(|_| empty_state(true));
        }
    };
    let _credential_source = credential.source;
    match claude::fetch_usage(&credential).await {
        Ok(usage) => {
            let _ = persist_success(
                database,
                &usage.windows,
                usage.extra_usage.as_ref(),
                attempted_at,
            );
        }
        Err(failure) => {
            let (health, retry_after) = classify_failure(&failure);
            let _ = record_failure(database, health, attempted_at, retry_after);
        }
    }
    load_state(database).unwrap_or_else(|_| empty_state(true))
}

fn classify_failure(failure: &claude::FetchFailure) -> (QuotaHealth, Option<u64>) {
    match failure {
        claude::FetchFailure::ExpiredLogin => (QuotaHealth::ExpiredLogin, None),
        claude::FetchFailure::Forbidden => (QuotaHealth::Forbidden, None),
        claude::FetchFailure::RateLimited(value) => (QuotaHealth::RateLimited, *value),
        claude::FetchFailure::ProviderUnavailable => (QuotaHealth::ProviderUnavailable, None),
        claude::FetchFailure::Offline => (QuotaHealth::Offline, None),
        claude::FetchFailure::InvalidResponse => (QuotaHealth::InvalidResponse, None),
    }
}

pub fn load_state(database: &Database) -> crate::db::Result<ProviderQuotaState> {
    let enabled = is_enabled(database);
    if !enabled {
        return Ok(empty_state(false));
    }
    let connection = Connection::open(database.path())?;
    let refresh = connection
        .query_row(
            "SELECT status, message, attempted_at, retry_at FROM provider_quota_refresh_state WHERE provider = ?1",
            [PROVIDER],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    let snapshot = connection
        .query_row(
            "SELECT q.snapshot_id, q.observed_at, q.source_device_id, d.friendly_name,
                    q.extra_usage_enabled, q.extra_monthly_limit_minor, q.extra_used_credits_minor,
                    q.extra_utilization_bps, q.extra_currency
             FROM provider_quota_snapshots q
             LEFT JOIN devices d ON d.id = q.source_device_id
             WHERE q.provider = ?1
             ORDER BY q.observed_at DESC, q.updated_at DESC, q.snapshot_id DESC LIMIT 1",
            [PROVIDER],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<bool>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()?;
    let (windows, extra_usage, observed_at, source_device_id, source_device_name) = if let Some((
        snapshot_id,
        observed,
        device_id,
        device_name,
        extra_enabled,
        monthly,
        used,
        extra_bps,
        currency,
    )) = snapshot
    {
        let mut statement = connection.prepare(
                "SELECT window_key, label, kind, scope, utilization_bps, resets_at
                 FROM provider_quota_snapshots WHERE snapshot_id = ?1
                 ORDER BY CASE kind WHEN 'rolling' THEN 0 WHEN 'weekly' THEN 1 WHEN 'model_weekly' THEN 2 ELSE 3 END, window_key",
            )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok(ProviderQuotaWindow {
                key: row.get(0)?,
                label: row.get(1)?,
                kind: QuotaWindowKind::parse(&row.get::<_, String>(2)?),
                scope: row.get(3)?,
                utilization_bps: row.get(4)?,
                resets_at: parse_optional_datetime(row.get::<_, Option<String>>(5)?),
            })
        })?;
        let windows = rows.collect::<Result<Vec<_>, _>>()?;
        let extra = extra_enabled.map(|enabled| ExtraUsage {
            enabled,
            monthly_limit_minor: monthly,
            used_credits_minor: used,
            utilization_bps: extra_bps,
            currency,
        });
        (
            windows,
            extra,
            parse_optional_datetime(Some(observed)),
            Some(device_id),
            device_name,
        )
    } else {
        (Vec::new(), None, None, None, None)
    };
    let (status, message, attempted_at, retry_at) = refresh
        .map(|(status, message, attempted, retry)| {
            (
                QuotaHealth::parse(&status),
                message,
                parse_optional_datetime(Some(attempted)),
                parse_optional_datetime(retry),
            )
        })
        .unwrap_or((
            QuotaHealth::CredentialUnavailable,
            status_message(QuotaHealth::CredentialUnavailable).into(),
            None,
            None,
        ));
    Ok(ProviderQuotaState {
        provider: PROVIDER.into(),
        enabled,
        status,
        message,
        stale: !windows.is_empty() && status != QuotaHealth::Healthy,
        windows,
        extra_usage,
        observed_at,
        attempted_at,
        retry_at,
        source_device_id,
        source_device_name,
    })
}

fn empty_state(enabled: bool) -> ProviderQuotaState {
    let status = if enabled {
        QuotaHealth::CredentialUnavailable
    } else {
        QuotaHealth::NotConfigured
    };
    ProviderQuotaState {
        provider: PROVIDER.into(),
        enabled,
        status,
        message: status_message(status).into(),
        stale: false,
        windows: Vec::new(),
        extra_usage: None,
        observed_at: None,
        attempted_at: None,
        retry_at: None,
        source_device_id: None,
        source_device_name: None,
    }
}

fn status_message(status: QuotaHealth) -> &'static str {
    match status {
        QuotaHealth::NotConfigured => "Claude live limits are off.",
        QuotaHealth::CredentialUnavailable => {
            "Claude Code sign-in not found. Open Claude Code to sign in."
        }
        QuotaHealth::PermissionDenied => {
            "Claude Code credentials could not be read. Check local credential permissions."
        }
        QuotaHealth::ExpiredLogin => {
            "Claude Code sign-in expired. Open Claude Code to refresh your sign-in."
        }
        QuotaHealth::Forbidden => "Anthropic did not allow this usage request.",
        QuotaHealth::RateLimited => "Temporarily rate limited. Last good limits remain visible.",
        QuotaHealth::ProviderUnavailable => "Anthropic usage reporting is temporarily unavailable.",
        QuotaHealth::Offline => "Claude limits could not refresh while this device is offline.",
        QuotaHealth::InvalidResponse => "Anthropic returned an unsupported usage response.",
        QuotaHealth::Healthy => "Connected through Claude Code.",
    }
}

fn cooldown_active(database: &Database) -> crate::db::Result<bool> {
    let retry_at = Connection::open(database.path())?
        .query_row(
            "SELECT retry_at FROM provider_quota_refresh_state WHERE provider = ?1",
            [PROVIDER],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
        .and_then(|value| parse_optional_datetime(Some(value)));
    Ok(retry_at.is_some_and(|value| value > Utc::now()))
}

fn record_failure(
    database: &Database,
    status: QuotaHealth,
    attempted_at: DateTime<Utc>,
    retry_after: Option<u64>,
) -> crate::db::Result<()> {
    let connection = Connection::open(database.path())?;
    let failures = connection
        .query_row(
            "SELECT consecutive_failures FROM provider_quota_refresh_state WHERE provider = ?1",
            [PROVIDER],
            |row| row.get::<_, u32>(0),
        )
        .optional()?
        .unwrap_or(0)
        .saturating_add(1);
    let exponential = 300_u64
        .saturating_mul(2_u64.saturating_pow(failures.saturating_sub(1).min(8)))
        .min(MAX_BACKOFF_SECONDS);
    let cooldown = retry_after
        .unwrap_or(exponential)
        .max(exponential)
        .min(86_400);
    let retry_at = attempted_at + TimeDelta::seconds(cooldown as i64);
    connection.execute(
        "INSERT INTO provider_quota_refresh_state(provider, status, message, attempted_at, retry_at, consecutive_failures)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(provider) DO UPDATE SET status=excluded.status, message=excluded.message,
           attempted_at=excluded.attempted_at, retry_at=excluded.retry_at,
           consecutive_failures=excluded.consecutive_failures",
        params![
            PROVIDER,
            status.as_str(),
            status_message(status),
            attempted_at.to_rfc3339(),
            retry_at.to_rfc3339(),
            failures,
        ],
    )?;
    Ok(())
}

pub(crate) fn persist_success(
    database: &Database,
    windows: &[ProviderQuotaWindow],
    extra_usage: Option<&ExtraUsage>,
    observed_at: DateTime<Utc>,
) -> crate::db::Result<()> {
    if windows.is_empty() {
        return Ok(());
    }
    let device = database.device()?;
    let bucket = observed_at
        .timestamp()
        .div_euclid(POLL_INTERVAL.as_secs() as i64);
    let mut material = windows
        .iter()
        .map(|window| {
            format!(
                "{}:{}",
                window.key,
                window
                    .resets_at
                    .map(|value| value.timestamp())
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    material.sort();
    let snapshot_id = hash(&format!(
        "{PROVIDER}|{}|{bucket}|{}",
        device.id,
        material.join("|")
    ));
    let now = Utc::now().to_rfc3339();
    let mut connection = Connection::open(database.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for window in windows {
        let id = hash(&format!("{snapshot_id}|{}", window.key));
        transaction.execute(
            "INSERT INTO provider_quota_snapshots(
               id, snapshot_id, provider, window_key, label, kind, scope, utilization_bps,
               resets_at, observed_at, source, source_device_id, extra_usage_enabled,
               extra_monthly_limit_minor, extra_used_credits_minor, extra_utilization_bps,
               extra_currency, created_at, updated_at, sync_status
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'provider_api',?11,?12,?13,?14,?15,?16,?17,?17,'pending')
             ON CONFLICT(id) DO UPDATE SET label=excluded.label, kind=excluded.kind,
               scope=excluded.scope, utilization_bps=excluded.utilization_bps,
               resets_at=excluded.resets_at, observed_at=excluded.observed_at,
               extra_usage_enabled=excluded.extra_usage_enabled,
               extra_monthly_limit_minor=excluded.extra_monthly_limit_minor,
               extra_used_credits_minor=excluded.extra_used_credits_minor,
               extra_utilization_bps=excluded.extra_utilization_bps,
               extra_currency=excluded.extra_currency, updated_at=excluded.updated_at,
               sync_status='pending'",
            params![
                id,
                snapshot_id,
                PROVIDER,
                window.key,
                window.label,
                window.kind.as_str(),
                window.scope,
                window.utilization_bps,
                window.resets_at.map(|value| value.to_rfc3339()),
                observed_at.to_rfc3339(),
                device.id,
                extra_usage.map(|value| value.enabled),
                extra_usage.and_then(|value| value.monthly_limit_minor),
                extra_usage.and_then(|value| value.used_credits_minor),
                extra_usage.and_then(|value| value.utilization_bps),
                extra_usage.and_then(|value| value.currency.as_deref()),
                now,
            ],
        )?;
    }
    transaction.execute(
        "INSERT INTO provider_quota_refresh_state(provider, status, message, attempted_at, retry_at, consecutive_failures)
         VALUES(?1,'healthy',?2,?3,NULL,0)
         ON CONFLICT(provider) DO UPDATE SET status='healthy', message=excluded.message,
           attempted_at=excluded.attempted_at, retry_at=NULL, consecutive_failures=0",
        params![PROVIDER, status_message(QuotaHealth::Healthy), observed_at.to_rfc3339()],
    )?;
    transaction.commit()?;
    Ok(())
}

fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn parse_optional_datetime(value: Option<String>) -> Option<DateTime<Utc>> {
    value
        .and_then(|item| DateTime::parse_from_rfc3339(&item).ok())
        .map(|item| item.with_timezone(&Utc))
}

pub(crate) fn pending_snapshots(database: &Database) -> Result<Vec<Value>, rusqlite::Error> {
    let connection = Connection::open(database.path())?;
    let mut statement = connection.prepare(
        "SELECT id, snapshot_id, provider, window_key, label, kind, scope, utilization_bps,
                resets_at, observed_at, source, source_device_id, extra_usage_enabled,
                extra_monthly_limit_minor, extra_used_credits_minor, extra_utilization_bps,
                extra_currency, created_at, updated_at
         FROM provider_quota_snapshots WHERE sync_status IN ('pending','error') ORDER BY created_at LIMIT 250",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?, "snapshot_id": row.get::<_, String>(1)?,
            "provider": row.get::<_, String>(2)?, "window_key": row.get::<_, String>(3)?,
            "label": row.get::<_, String>(4)?, "kind": row.get::<_, String>(5)?,
            "scope": row.get::<_, Option<String>>(6)?, "utilization_bps": row.get::<_, i64>(7)?,
            "resets_at": row.get::<_, Option<String>>(8)?, "observed_at": row.get::<_, String>(9)?,
            "source": row.get::<_, String>(10)?, "source_device_id": row.get::<_, String>(11)?,
            "extra_usage_enabled": row.get::<_, Option<bool>>(12)?,
            "extra_monthly_limit_minor": row.get::<_, Option<i64>>(13)?,
            "extra_used_credits_minor": row.get::<_, Option<i64>>(14)?,
            "extra_utilization_bps": row.get::<_, Option<i64>>(15)?,
            "extra_currency": row.get::<_, Option<String>>(16)?,
            "created_at": row.get::<_, String>(17)?, "updated_at": row.get::<_, String>(18)?
        }))
    })?;
    rows.collect()
}

pub(crate) fn apply_remote_snapshots(
    database: &Database,
    rows: &[Value],
) -> Result<usize, rusqlite::Error> {
    let mut connection = Connection::open(database.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut applied = 0;
    for row in rows {
        let Some(id) = string(row, "id").filter(|value| value.len() == 64) else {
            continue;
        };
        let Some(snapshot_id) = string(row, "snapshot_id").filter(|value| value.len() == 64) else {
            continue;
        };
        let utilization = row
            .get("utilization_bps")
            .and_then(Value::as_i64)
            .unwrap_or(-1);
        if !(0..=10_000).contains(&utilization) {
            continue;
        }
        applied += transaction.execute(
            "INSERT INTO provider_quota_snapshots(
               id,snapshot_id,provider,window_key,label,kind,scope,utilization_bps,resets_at,
               observed_at,source,source_device_id,extra_usage_enabled,extra_monthly_limit_minor,
               extra_used_credits_minor,extra_utilization_bps,extra_currency,created_at,updated_at,sync_status
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'cloud_sync',?11,?12,?13,?14,?15,?16,?17,?18,'synced')
             ON CONFLICT(id) DO UPDATE SET utilization_bps=excluded.utilization_bps,
               resets_at=excluded.resets_at, observed_at=excluded.observed_at,
               extra_usage_enabled=excluded.extra_usage_enabled,
               extra_monthly_limit_minor=excluded.extra_monthly_limit_minor,
               extra_used_credits_minor=excluded.extra_used_credits_minor,
               extra_utilization_bps=excluded.extra_utilization_bps,
               extra_currency=excluded.extra_currency, updated_at=excluded.updated_at,
               sync_status='synced' WHERE excluded.updated_at > provider_quota_snapshots.updated_at",
            params![id, snapshot_id, string(row,"provider"), string(row,"window_key"), string(row,"label"),
                string(row,"kind"), row.get("scope").and_then(Value::as_str), utilization,
                row.get("resets_at").and_then(Value::as_str), string(row,"observed_at"),
                string(row,"source_device_id"), row.get("extra_usage_enabled").and_then(Value::as_bool),
                row.get("extra_monthly_limit_minor").and_then(Value::as_i64),
                row.get("extra_used_credits_minor").and_then(Value::as_i64),
                row.get("extra_utilization_bps").and_then(Value::as_i64),
                row.get("extra_currency").and_then(Value::as_str), string(row,"created_at"), string(row,"updated_at")],
        )?;
    }
    transaction.commit()?;
    Ok(applied)
}

pub(crate) fn mark_snapshots_synced(
    database: &Database,
    ids: &[&str],
) -> Result<(), rusqlite::Error> {
    let mut connection = Connection::open(database.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for id in ids {
        transaction.execute(
            "UPDATE provider_quota_snapshots SET sync_status='synced' WHERE id=?1",
            [id],
        )?;
    }
    transaction.commit()
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> (tempfile::TempDir, Database) {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("quota.db")).unwrap();
        database.ensure_device("test").unwrap();
        database.set_setting(ENABLED_SETTING, "true").unwrap();
        (directory, database)
    }

    fn windows(utilization_bps: i64, reset: &str) -> Vec<ProviderQuotaWindow> {
        vec![ProviderQuotaWindow {
            key: "five_hour".into(),
            label: "5-hour".into(),
            kind: QuotaWindowKind::Rolling,
            scope: None,
            utilization_bps,
            resets_at: Some(reset.parse().unwrap()),
        }]
    }

    #[test]
    fn refresh_attempts_are_coalesced() {
        let runtime = QuotaRuntime::default();
        let first = runtime.try_begin().unwrap();
        assert!(runtime.try_begin().is_none());
        drop(first);
        assert!(runtime.try_begin().is_some());
    }

    #[test]
    fn persistence_is_normalized_sampled_and_never_contains_a_token() {
        let (_directory, database) = database();
        let observed = "2026-08-31T12:01:00Z".parse().unwrap();
        persist_success(
            &database,
            &windows(4_763, "2026-08-31T14:00:00Z"),
            None,
            observed,
        )
        .unwrap();
        persist_success(
            &database,
            &windows(4_800, "2026-08-31T14:00:00Z"),
            None,
            observed + TimeDelta::minutes(2),
        )
        .unwrap();
        let connection = Connection::open(database.path()).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM provider_quota_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        let state = load_state(&database).unwrap();
        assert_eq!(state.windows[0].utilization_bps, 4_800);
        let columns = connection
            .prepare("PRAGMA table_info(provider_quota_snapshots)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(!columns.iter().any(|name| name.contains("token")
            || name.contains("credential")
            || name.contains("response")));
        let bytes = std::fs::read(database.path()).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-private-access-value"));
    }

    #[test]
    fn reset_change_creates_material_history_and_latest_snapshot_wins() {
        let (_directory, database) = database();
        let observed = "2026-08-31T12:01:00Z".parse().unwrap();
        persist_success(
            &database,
            &windows(2_000, "2026-08-31T14:00:00Z"),
            None,
            observed,
        )
        .unwrap();
        persist_success(
            &database,
            &windows(100, "2026-08-31T19:00:00Z"),
            None,
            observed + TimeDelta::minutes(1),
        )
        .unwrap();
        let connection = Connection::open(database.path()).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM provider_quota_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            load_state(&database).unwrap().windows[0].utilization_bps,
            100
        );
    }

    #[test]
    fn failures_back_off_and_keep_last_good_snapshot_stale() {
        let (_directory, database) = database();
        let observed = Utc::now() - TimeDelta::minutes(10);
        persist_success(
            &database,
            &windows(4_200, "2026-09-01T14:00:00Z"),
            None,
            observed,
        )
        .unwrap();
        record_failure(&database, QuotaHealth::RateLimited, Utc::now(), Some(900)).unwrap();
        let state = load_state(&database).unwrap();
        assert_eq!(state.status, QuotaHealth::RateLimited);
        assert!(state.stale);
        assert_eq!(state.windows[0].utilization_bps, 4_200);
        assert!(state.retry_at.unwrap() >= Utc::now() + TimeDelta::minutes(14));
        record_failure(&database, QuotaHealth::Offline, Utc::now(), None).unwrap();
        let failures: (u32, String) = Connection::open(database.path()).unwrap().query_row(
            "SELECT consecutive_failures, status FROM provider_quota_refresh_state WHERE provider='claude'", [],
            |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(failures, (2, "offline".into()));
    }

    #[test]
    fn normalized_sync_moves_quota_to_a_second_device_and_freshest_is_not_summed() {
        let (_origin_directory, origin) = database();
        let observed = "2026-08-31T12:00:00Z".parse().unwrap();
        persist_success(
            &origin,
            &windows(3_100, "2026-09-01T14:00:00Z"),
            None,
            observed,
        )
        .unwrap();
        let payload = pending_snapshots(&origin).unwrap();
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains("accessToken") && !serialized.contains("sk-ant"));

        let (_receiver_directory, receiver) = database();
        let origin_device = origin.device().unwrap();
        let now = Utc::now().to_rfc3339();
        Connection::open(receiver.path()).unwrap().execute(
            "INSERT OR IGNORE INTO devices(id,friendly_name,os,architecture,app_version,created_at,last_seen_at,sync_status)
             VALUES(?1,'Source Mac','macos','aarch64','test',?2,?2,'synced')",
            params![origin_device.id, now],
        ).unwrap();
        assert_eq!(apply_remote_snapshots(&receiver, &payload).unwrap(), 1);
        let state = load_state(&receiver).unwrap();
        assert_eq!(state.windows.len(), 1);
        assert_eq!(state.windows[0].utilization_bps, 3_100);
        assert_eq!(state.source_device_name.as_deref(), Some("Source Mac"));
    }

    #[test]
    fn status_messages_cover_network_and_http_failures_without_raw_data() {
        for health in [
            QuotaHealth::ExpiredLogin,
            QuotaHealth::Forbidden,
            QuotaHealth::RateLimited,
            QuotaHealth::ProviderUnavailable,
            QuotaHealth::Offline,
            QuotaHealth::InvalidResponse,
        ] {
            let message = status_message(health);
            assert!(!message.contains("Bearer") && !message.contains('{'));
        }
    }

    #[test]
    fn polling_classifies_401_403_429_5xx_timeout_offline_and_invalid_responses() {
        let cases = [
            (
                claude::FetchFailure::ExpiredLogin,
                QuotaHealth::ExpiredLogin,
                None,
            ),
            (
                claude::FetchFailure::Forbidden,
                QuotaHealth::Forbidden,
                None,
            ),
            (
                claude::FetchFailure::RateLimited(Some(901)),
                QuotaHealth::RateLimited,
                Some(901),
            ),
            (
                claude::FetchFailure::RateLimited(None),
                QuotaHealth::RateLimited,
                None,
            ),
            (
                claude::FetchFailure::ProviderUnavailable,
                QuotaHealth::ProviderUnavailable,
                None,
            ),
            (claude::FetchFailure::Offline, QuotaHealth::Offline, None),
            (
                claude::FetchFailure::InvalidResponse,
                QuotaHealth::InvalidResponse,
                None,
            ),
        ];
        for (failure, expected_status, expected_retry) in cases {
            assert_eq!(
                classify_failure(&failure),
                (expected_status, expected_retry)
            );
        }
    }
}
