use crate::device::{Device, clean_name};
use crate::domain::UsageEvent;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

const INITIAL_MIGRATION: &str = include_str!("../migrations/0001_initial.sql");
const HISTORICAL_PRICING_MIGRATION: &str =
    include_str!("../migrations/0002_historical_pricing.sql");
const GROK_COMPLETED_TURNS_MIGRATION: &str =
    include_str!("../migrations/0003_grok_completed_turns.sql");

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("invalid value: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, DatabaseError>;

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub id: String,
    pub provider: String,
    pub plan_name: String,
    pub monthly_price_usd_cents: i64,
    pub billing_cadence: String,
    pub active: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorState {
    pub source_key: String,
    pub provider: String,
    pub safe_file_fingerprint: String,
    pub file_size: i64,
    pub modified_at_ms: i64,
    pub last_processed_offset: i64,
    pub parser_version: i64,
    pub last_scan_at: DateTime<Utc>,
    pub last_usage_at: Option<DateTime<Utc>>,
    pub status: String,
    pub diagnostic: Option<String>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| DatabaseError::Invalid(error.to_string()))?;
        }
        let db = Self { path };
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> Result<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(connection)
    }

    fn migrate(&self) -> Result<()> {
        let mut connection = self.connect()?;
        let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if current < 1 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(INITIAL_MIGRATION)?;
            transaction.pragma_update(None, "user_version", 1)?;
            transaction.execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES(1, ?1)",
                [Utc::now().to_rfc3339()],
            )?;
            transaction.commit()?;
        }
        if current < 2 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(HISTORICAL_PRICING_MIGRATION)?;
            transaction.pragma_update(None, "user_version", 2)?;
            transaction.execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES(2, ?1)",
                [Utc::now().to_rfc3339()],
            )?;
            transaction.commit()?;
        }
        if current < 3 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(GROK_COMPLETED_TURNS_MIGRATION)?;
            transaction.pragma_update(None, "user_version", 3)?;
            transaction.execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES(3, ?1)",
                [Utc::now().to_rfc3339()],
            )?;
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn ensure_device(&self, app_version: &str) -> Result<Device> {
        let connection = self.connect()?;
        let local_device_id: Option<String> = connection
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'local_device_id'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let existing = connection
            .query_row(
                "SELECT id, friendly_name, os, architecture, app_version, created_at, last_seen_at, last_sync_at, sync_status
                 FROM devices WHERE id = ?1",
                [local_device_id.as_deref().unwrap_or("")],
                map_device,
            )
            .optional()?;
        if let Some(mut device) = existing {
            device.last_seen_at = Utc::now();
            device.app_version = app_version.to_owned();
            connection.execute(
                "UPDATE devices SET app_version = ?2, last_seen_at = ?3 WHERE id = ?1",
                params![
                    device.id,
                    device.app_version,
                    device.last_seen_at.to_rfc3339()
                ],
            )?;
            return Ok(device);
        }

        let device = Device::new(app_version);
        connection.execute(
            "INSERT INTO devices(id, friendly_name, os, architecture, app_version, created_at, last_seen_at, sync_status)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                device.id,
                device.friendly_name,
                device.os,
                device.architecture,
                device.app_version,
                device.created_at.to_rfc3339(),
                device.last_seen_at.to_rfc3339(),
                device.sync_status,
            ],
        )?;
        connection.execute(
            "INSERT INTO app_settings(key, value, updated_at) VALUES('local_device_id', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![device.id, Utc::now().to_rfc3339()],
        )?;
        Ok(device)
    }

    pub fn device(&self) -> Result<Device> {
        self.connect()?
            .query_row(
                "SELECT id, friendly_name, os, architecture, app_version, created_at, last_seen_at, last_sync_at, sync_status
                 FROM devices WHERE id = (SELECT value FROM app_settings WHERE key = 'local_device_id')",
                [],
                map_device,
            )
            .map_err(Into::into)
    }

    pub fn rename_device(&self, name: &str) -> Result<Device> {
        let clean = clean_name(name)
            .ok_or_else(|| DatabaseError::Invalid("Device name cannot be empty".into()))?;
        let connection = self.connect()?;
        connection.execute(
            "UPDATE devices SET friendly_name = ?1, last_seen_at = ?2 WHERE id = (SELECT value FROM app_settings WHERE key = 'local_device_id')",
            params![clean, Utc::now().to_rfc3339()],
        )?;
        self.device()
    }

    pub fn insert_usage_events(&self, events: &[UsageEvent]) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut inserted = 0;
        {
            let mut statement = transaction.prepare_cached(
                "INSERT INTO usage_events(
                    id, provider, source, source_type, native_session_id, native_event_id, occurred_at,
                    model, project_name, input_tokens, cached_input_tokens, cache_write_tokens,
                    output_tokens, reasoning_tokens, total_tokens, estimated_api_value_usd_micros,
                    native_cost_usd_ticks, pricing_status, measurement_kind, device_id, created_at,
                    updated_at, sync_status
                 ) VALUES(
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                    ?16, ?17, ?18, ?19, ?20, ?21, ?21, 'pending'
                 ) ON CONFLICT(id) DO NOTHING",
            )?;
            for event in events {
                inserted += statement.execute(params![
                    event.id,
                    event.provider,
                    event.source,
                    event.source_type.as_str(),
                    event.native_session_id,
                    event.native_event_id,
                    event.occurred_at.to_rfc3339(),
                    event.model,
                    event.project_name,
                    event.tokens.input_tokens,
                    event.tokens.cached_input_tokens,
                    event.tokens.cache_write_tokens,
                    event.tokens.output_tokens,
                    event.tokens.reasoning_tokens,
                    event.tokens.total_tokens,
                    event.estimated_api_value_usd_micros,
                    event.native_cost_usd_ticks,
                    event.pricing_status,
                    event.measurement_kind.as_str(),
                    event.device_id,
                    event.created_at.to_rfc3339(),
                ])?;
            }
        }
        transaction.commit()?;
        Ok(inserted)
    }

    /// Marks only uniquely matched legacy Grok rows as superseded. The old row remains
    /// recoverable; analytics ignore it once the authoritative completed-turn event exists.
    pub fn reconcile_grok_events(&self, events: &[UsageEvent]) -> Result<usize> {
        use std::collections::BTreeMap;

        #[derive(Default)]
        struct TurnTotal {
            session_id: String,
            occurred_at: String,
            input: i64,
            cached: i64,
            output: i64,
            reasoning: i64,
            total: i64,
            replacement_id: String,
        }

        let mut turns = BTreeMap::<String, TurnTotal>::new();
        for event in events.iter().filter(|event| event.provider == "grok") {
            let Some((turn_id, _)) = event.native_event_id.split_once(":model:") else {
                continue;
            };
            let key = format!("{}\u{1f}{turn_id}", event.native_session_id);
            let turn = turns.entry(key).or_default();
            turn.session_id.clone_from(&event.native_session_id);
            turn.occurred_at = event.occurred_at.to_rfc3339();
            turn.input = turn.input.saturating_add(event.tokens.input_tokens);
            turn.cached = turn.cached.saturating_add(event.tokens.cached_input_tokens);
            turn.output = turn.output.saturating_add(event.tokens.output_tokens);
            turn.reasoning = turn.reasoning.saturating_add(event.tokens.reasoning_tokens);
            turn.total = turn.total.saturating_add(event.tokens.total_tokens);
            turn.replacement_id.clone_from(&event.id);
        }

        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut reconciled = 0;
        for turn in turns.into_values() {
            let candidates: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM usage_events
                 WHERE provider = 'grok' AND source = 'grok_build'
                   AND superseded_by_event_id IS NULL
                   AND native_event_id NOT LIKE 'turn:%:model:%'
                   AND native_session_id = ?1 AND occurred_at = ?2
                   AND input_tokens = ?3 AND cached_input_tokens = ?4
                   AND output_tokens = ?5 AND reasoning_tokens = ?6 AND total_tokens = ?7",
                params![
                    turn.session_id,
                    turn.occurred_at,
                    turn.input,
                    turn.cached,
                    turn.output,
                    turn.reasoning,
                    turn.total,
                ],
                |row| row.get(0),
            )?;
            if candidates != 1 {
                continue;
            }
            reconciled += transaction.execute(
                "UPDATE usage_events SET superseded_by_event_id = ?8, updated_at = ?9,
                        sync_status = 'pending'
                 WHERE provider = 'grok' AND source = 'grok_build'
                   AND superseded_by_event_id IS NULL
                   AND native_event_id NOT LIKE 'turn:%:model:%'
                   AND native_session_id = ?1 AND occurred_at = ?2
                   AND input_tokens = ?3 AND cached_input_tokens = ?4
                   AND output_tokens = ?5 AND reasoning_tokens = ?6 AND total_tokens = ?7",
                params![
                    turn.session_id,
                    turn.occurred_at,
                    turn.input,
                    turn.cached,
                    turn.output,
                    turn.reasoning,
                    turn.total,
                    turn.replacement_id,
                    Utc::now().to_rfc3339(),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(reconciled)
    }

    #[cfg(test)]
    pub fn event_count_and_tokens(&self) -> Result<(i64, i64)> {
        self.connect()?
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0) FROM usage_events
                 WHERE measurement_kind = 'measured' AND superseded_by_event_id IS NULL",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
    }

    pub fn collector_state(&self, source_key: &str) -> Result<Option<CollectorState>> {
        self.connect()?
            .query_row(
                "SELECT source_key, provider, safe_file_fingerprint, file_size, modified_at_ms, last_processed_offset,
                        parser_version, last_scan_at, last_usage_at, status, diagnostic
                 FROM collector_state WHERE source_key = ?1",
                [source_key],
                |row| {
                    Ok(CollectorState {
                        source_key: row.get(0)?,
                        provider: row.get(1)?,
                        safe_file_fingerprint: row.get(2)?,
                        file_size: row.get(3)?,
                        modified_at_ms: row.get(4)?,
                        last_processed_offset: row.get(5)?,
                        parser_version: row.get(6)?,
                        last_scan_at: parse_datetime(row.get::<_, String>(7)?, 7)?,
                        last_usage_at: parse_optional_datetime(row.get(8)?, 8)?,
                        status: row.get(9)?,
                        diagnostic: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn save_collector_state(&self, state: &CollectorState) -> Result<()> {
        self.connect()?.execute(
            "INSERT INTO collector_state(source_key, provider, safe_file_fingerprint, file_size, modified_at_ms,
                    last_processed_offset, parser_version, last_scan_at, last_usage_at, status, diagnostic)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(source_key) DO UPDATE SET
               provider = excluded.provider,
               safe_file_fingerprint = excluded.safe_file_fingerprint,
               file_size = excluded.file_size,
               modified_at_ms = excluded.modified_at_ms,
               last_processed_offset = excluded.last_processed_offset,
               parser_version = excluded.parser_version,
               last_scan_at = excluded.last_scan_at,
               last_usage_at = excluded.last_usage_at,
               status = excluded.status,
               diagnostic = excluded.diagnostic",
            params![
                state.source_key,
                state.provider,
                state.safe_file_fingerprint,
                state.file_size,
                state.modified_at_ms,
                state.last_processed_offset,
                state.parser_version,
                state.last_scan_at.to_rfc3339(),
                state.last_usage_at.map(|value| value.to_rfc3339()),
                state.status,
                state.diagnostic,
            ],
        )?;
        Ok(())
    }

    pub fn ensure_default_subscriptions(&self) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let defaults = [
            ("openai", "ChatGPT Personal"),
            ("openai_business", "ChatGPT Business"),
            ("anthropic", "Claude"),
            ("xai", "Grok"),
            ("google", "Google AI / Gemini"),
        ];
        let connection = self.connect()?;
        for (provider, name) in defaults {
            let id = format!("subscription-{provider}");
            connection.execute(
                "INSERT OR IGNORE INTO subscriptions(id, provider, plan_name, monthly_price_usd_cents,
                    billing_cadence, active, created_at, updated_at, sync_status)
                 VALUES(?1, ?2, ?3, 0, 'monthly', 0, ?4, ?4, 'pending')",
                params![id, provider, name, now],
            )?;
        }
        Ok(())
    }

    pub fn subscriptions(&self) -> Result<Vec<Subscription>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, provider, plan_name, monthly_price_usd_cents, billing_cadence, active, updated_at
             FROM subscriptions ORDER BY CASE provider
               WHEN 'openai' THEN 1 WHEN 'openai_business' THEN 2 WHEN 'anthropic' THEN 3 WHEN 'xai' THEN 4 ELSE 5 END",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Subscription {
                id: row.get(0)?,
                provider: row.get(1)?,
                plan_name: row.get(2)?,
                monthly_price_usd_cents: row.get(3)?,
                billing_cadence: row.get(4)?,
                active: row.get(5)?,
                updated_at: parse_datetime(row.get::<_, String>(6)?, 6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn save_subscription(&self, subscription: &Subscription) -> Result<()> {
        if subscription.monthly_price_usd_cents < 0 {
            return Err(DatabaseError::Invalid(
                "Subscription price cannot be negative".into(),
            ));
        }
        if !matches!(subscription.billing_cadence.as_str(), "monthly" | "annual") {
            return Err(DatabaseError::Invalid("Unsupported billing cadence".into()));
        }
        self.connect()?.execute(
            "UPDATE subscriptions SET plan_name = ?2, monthly_price_usd_cents = ?3, billing_cadence = ?4,
                    active = ?5, updated_at = ?6, sync_status = 'pending' WHERE id = ?1",
            params![
                subscription.id,
                subscription.plan_name.trim(),
                subscription.monthly_price_usd_cents,
                subscription.billing_cadence,
                subscription.active,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.connect()?
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        const ALLOWED: &[&str] = &[
            "close_to_tray",
            "theme",
            "sync_enabled",
            "activity_claude_desktop_enabled",
            "activity_browser_bridge_enabled",
        ];
        if !ALLOWED.contains(&key) {
            return Err(DatabaseError::Invalid("Unsupported setting".into()));
        }
        self.connect()?.execute(
            "INSERT INTO app_settings(key, value, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn ensure_private_setting(
        &self,
        key: &str,
        generate: impl FnOnce() -> String,
    ) -> Result<String> {
        if let Some(value) = self.setting(key)? {
            return Ok(value);
        }
        let value = generate();
        self.connect()?.execute(
            "INSERT OR IGNORE INTO app_settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![key, value, Utc::now().to_rfc3339()],
        )?;
        self.setting(key)?
            .ok_or_else(|| DatabaseError::Invalid("Private setting was not created".into()))
    }
}

fn map_device(row: &rusqlite::Row<'_>) -> rusqlite::Result<Device> {
    Ok(Device {
        id: row.get(0)?,
        friendly_name: row.get(1)?,
        os: row.get(2)?,
        architecture: row.get(3)?,
        app_version: row.get(4)?,
        created_at: parse_datetime(row.get::<_, String>(5)?, 5)?,
        last_seen_at: parse_datetime(row.get::<_, String>(6)?, 6)?,
        last_sync_at: parse_optional_datetime(row.get(7)?, 7)?,
        sync_status: row.get(8)?,
    })
}

fn parse_datetime(value: String, column: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
}

fn parse_optional_datetime(
    value: Option<String>,
    column: usize,
) -> rusqlite::Result<Option<DateTime<Utc>>> {
    value.map(|item| parse_datetime(item, column)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{TokenCounts, UsageEvent};

    fn database() -> (tempfile::TempDir, Database, Device) {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        let device = database.ensure_device("test").unwrap();
        (directory, database, device)
    }

    #[test]
    fn migrations_and_device_identity_persist() {
        let (_directory, database, first) = database();
        let second = database.ensure_device("test-2").unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.app_version, "test-2");
    }

    #[test]
    fn grok_schema_migration_preserves_v2_usage_rows() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade.db");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(INITIAL_MIGRATION).unwrap();
        connection
            .execute_batch(HISTORICAL_PRICING_MIGRATION)
            .unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        let timestamp = Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO devices(id, friendly_name, os, architecture, app_version, created_at,
                   last_seen_at, sync_status)
                 VALUES('device', 'Test device', 'windows', 'x86_64', 'test', ?1, ?1, 'local_only')",
                [&timestamp],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO usage_events(id, provider, source, source_type, native_session_id,
                   native_event_id, occurred_at, input_tokens, output_tokens, total_tokens,
                   pricing_status, measurement_kind, device_id, created_at, updated_at)
                 VALUES(?1, 'grok', 'grok_build', 'local_cli', 'session', 'legacy', ?2,
                   10, 2, 12, 'unavailable', 'measured', 'device', ?2, ?2)",
                params!["a".repeat(64), timestamp],
            )
            .unwrap();
        drop(connection);

        let database = Database::open(&path).unwrap();
        let connection = Connection::open(database.path()).unwrap();
        let preserved: (i64, i64, Option<i64>, Option<String>, i64) = connection
            .query_row(
                "SELECT COUNT(*), cache_write_tokens, native_cost_usd_ticks,
                   superseded_by_event_id, (SELECT user_version FROM pragma_user_version)
                 FROM usage_events",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(preserved, (1, 0, None, None, 3));
    }

    #[test]
    fn repeated_ingestion_is_idempotent() {
        let (_directory, database, device) = database();
        let event = UsageEvent::measured(
            "codex",
            "codex_cli",
            "session",
            "ordinal:1",
            Utc::now(),
            Some("gpt-test".into()),
            Some("ArcMeter".into()),
            TokenCounts {
                input_tokens: 100,
                output_tokens: 20,
                total_tokens: 120,
                ..Default::default()
            },
            device.id,
        );
        assert_eq!(
            database
                .insert_usage_events(std::slice::from_ref(&event))
                .unwrap(),
            1
        );
        assert_eq!(
            database
                .insert_usage_events(std::slice::from_ref(&event))
                .unwrap(),
            0
        );
        assert_eq!(database.event_count_and_tokens().unwrap(), (1, 120));
    }

    #[test]
    fn exact_legacy_grok_measurement_is_superseded_without_deletion() {
        let (_directory, database, device) = database();
        let occurred_at = Utc::now();
        let tokens = TokenCounts {
            input_tokens: 100,
            cached_input_tokens: 40,
            cache_write_tokens: 10,
            output_tokens: 20,
            reasoning_tokens: 8,
            total_tokens: 120,
        };
        let legacy = UsageEvent::measured(
            "grok",
            "grok_build",
            "session",
            "legacy-turn",
            occurred_at,
            Some("grok-4.5-build".into()),
            Some("ArcMeter".into()),
            tokens.clone(),
            device.id.clone(),
        );
        database.insert_usage_events(&[legacy]).unwrap();
        let completed = UsageEvent::measured(
            "grok",
            "grok_build",
            "session",
            format!("turn:{}:model:{}", "a".repeat(64), "b".repeat(64)),
            occurred_at,
            Some("grok-4.5-build".into()),
            Some("ArcMeter".into()),
            tokens,
            device.id,
        );

        assert_eq!(
            database
                .insert_usage_events(std::slice::from_ref(&completed))
                .unwrap(),
            1
        );
        assert_eq!(database.reconcile_grok_events(&[completed]).unwrap(), 1);
        assert_eq!(database.event_count_and_tokens().unwrap(), (1, 120));

        let connection = Connection::open(database.path()).unwrap();
        let (all_rows, superseded): (i64, i64) = connection
            .query_row(
                "SELECT COUNT(*), COUNT(superseded_by_event_id) FROM usage_events",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((all_rows, superseded), (2, 1));
    }

    #[test]
    #[ignore = "opens the explicitly selected launched-app database for a production smoke check"]
    fn launched_database_integrity_smoke_check() {
        let path = std::env::var("ARCMETER_TEST_DATABASE_PATH")
            .expect("set ARCMETER_TEST_DATABASE_PATH to an ArcMeter SQLite database");
        let connection = Connection::open(path).unwrap();
        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
        let summary = connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM usage_events),
                   (SELECT COUNT(*) FROM usage_events WHERE measurement_kind = 'measured'),
                   (SELECT COALESCE(SUM(total_tokens), 0) FROM usage_events WHERE measurement_kind = 'measured'),
                   (SELECT COUNT(*) FROM devices),
                   (SELECT COUNT(*) FROM collector_state),
                   (SELECT COUNT(*) FROM pricing),
                   (SELECT user_version FROM pragma_user_version)",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .unwrap();
        println!(
            "ARCMETER_LAUNCHED_DB events={} measured={} tokens={} devices={} collectors={} pricing={} schema={} integrity={}",
            summary.0, summary.1, summary.2, summary.3, summary.4, summary.5, summary.6, integrity
        );
        assert!(summary.0 > 0);
        assert!(summary.1 > 0);
        assert!(summary.2 > 0);
        assert_eq!(summary.3, 1);
        assert!(summary.4 >= 4);
        assert!(summary.5 >= 20);
        assert_eq!(summary.6, 3);
    }
}
