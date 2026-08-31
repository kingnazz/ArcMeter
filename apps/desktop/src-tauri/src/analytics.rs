use crate::collectors::{ScanReport, SourceScanResult};
use crate::db::{Database, DatabaseError, Result, Subscription};
use crate::device::Device;
use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub generated_at: DateTime<Utc>,
    pub range: String,
    pub metrics: HeadlineMetrics,
    pub trend: Vec<TrendPoint>,
    pub by_provider: Vec<BreakdownItem>,
    pub by_model: Vec<BreakdownItem>,
    pub by_project: Vec<BreakdownItem>,
    pub by_device: Vec<BreakdownItem>,
    pub activity: Vec<ActivityItem>,
    pub insights: Vec<Insight>,
    pub sources: Vec<SourceScanResult>,
    pub subscriptions: Vec<Subscription>,
    pub device: Device,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadlineMetrics {
    pub measured_tokens_today: i64,
    pub measured_tokens_month: i64,
    pub measured_tokens_range: i64,
    pub measured_events_range: i64,
    pub priced_tokens_range: i64,
    pub priced_events_range: i64,
    pub activity_minutes_range: i64,
    pub monthly_subscription_usd_cents: i64,
    pub estimated_api_value_usd_micros: Option<i64>,
    pub pricing_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub date: String,
    pub label: String,
    pub tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakdownItem {
    pub key: String,
    pub label: String,
    pub tokens: i64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: String,
    pub provider: String,
    pub source: String,
    pub occurred_at: DateTime<Utc>,
    pub model: Option<String>,
    pub project_name: Option<String>,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub native_cost_usd_ticks: Option<i64>,
    pub estimated_api_value_usd_micros: Option<i64>,
    pub measurement_kind: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub tone: String,
}

pub fn dashboard(database: &Database, range: &str, scan: &ScanReport) -> Result<DashboardSnapshot> {
    let connection = Connection::open(database.path())?;
    let now = Utc::now();
    let start = range_start(range, now);
    let today_start = local_day_start(now);
    let month_start = local_month_start(now);
    let measured_tokens_today = sum_tokens(&connection, today_start)?;
    let measured_tokens_month = sum_tokens(&connection, month_start)?;
    let (measured_events_range, measured_tokens_range, value_micros, priced_events_range, priced_tokens_range) = connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0), COALESCE(SUM(estimated_api_value_usd_micros), 0),
                COALESCE(SUM(CASE WHEN pricing_status = 'available' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN pricing_status = 'available' THEN total_tokens ELSE 0 END), 0)
         FROM usage_events WHERE measurement_kind = 'measured' AND occurred_at >= ?1
           AND superseded_by_event_id IS NULL",
        [start.to_rfc3339()],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?)),
    )?;
    let activity_minutes_range: i64 = connection.query_row(
        "SELECT COUNT(*) FROM usage_events WHERE measurement_kind = 'activity_only'
           AND occurred_at >= ?1 AND superseded_by_event_id IS NULL",
        [start.to_rfc3339()],
        |row| row.get(0),
    )?;
    let subscriptions = database.subscriptions()?;
    let monthly_subscription_usd_cents = subscriptions
        .iter()
        .filter(|subscription| subscription.active)
        .map(|subscription| subscription.monthly_price_usd_cents)
        .sum::<i64>();
    let pricing_complete =
        measured_events_range > 0 && priced_events_range == measured_events_range;
    let estimated_api_value_usd_micros = (priced_events_range > 0).then_some(value_micros);
    let total = measured_tokens_range.max(1);

    Ok(DashboardSnapshot {
        generated_at: now,
        range: range.into(),
        metrics: HeadlineMetrics {
            measured_tokens_today,
            measured_tokens_month,
            measured_tokens_range,
            measured_events_range,
            priced_tokens_range,
            priced_events_range,
            activity_minutes_range,
            monthly_subscription_usd_cents,
            estimated_api_value_usd_micros,
            pricing_complete,
        },
        trend: trend(&connection, start)?,
        by_provider: breakdown(&connection, "provider", start, total)?,
        by_model: breakdown(
            &connection,
            "COALESCE(model, 'Unknown model')",
            start,
            total,
        )?,
        by_project: breakdown(
            &connection,
            "COALESCE(project_name, 'Unknown project')",
            start,
            total,
        )?,
        by_device: breakdown_devices(&connection, start, total)?,
        activity: activity(&connection, start, 200, 0)?,
        insights: insights(&connection, start, now, measured_tokens_range)?,
        sources: normalized_sources(scan),
        subscriptions,
        device: database.device()?,
    })
}

pub fn activity_page(
    database: &Database,
    range: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ActivityItem>> {
    let connection = Connection::open(database.path())?;
    activity(
        &connection,
        range_start(range, Utc::now()),
        limit.clamp(1, 500),
        offset.max(0),
    )
}

fn normalized_sources(scan: &ScanReport) -> Vec<SourceScanResult> {
    if scan.sources.len() == 4 {
        return scan.sources.clone();
    }
    [
        ("codex", "Codex"),
        ("claude", "Claude Code CLI"),
        ("grok", "Grok Build"),
        ("gemini", "Gemini CLI"),
    ]
    .into_iter()
    .map(|(provider, label)| SourceScanResult {
        provider: provider.into(),
        label: label.into(),
        detected: false,
        files_seen: 0,
        records_seen: 0,
        records_inserted: 0,
        measured_records: 0,
        measured_sessions: 0,
        measured_turns: 0,
        measured_tokens: 0,
        native_cost_usd_ticks: None,
        last_scan_at: Utc::now(),
        last_usage_at: None,
        status: "healthy".into(),
        diagnostics: Vec::new(),
    })
    .collect()
}

fn sum_tokens(connection: &Connection, start: DateTime<Utc>) -> Result<i64> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM usage_events
             WHERE measurement_kind = 'measured' AND occurred_at >= ?1
               AND superseded_by_event_id IS NULL",
            [start.to_rfc3339()],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn trend(connection: &Connection, start: DateTime<Utc>) -> Result<Vec<TrendPoint>> {
    let mut statement = connection.prepare(
        "SELECT substr(occurred_at, 1, 10) AS day, COALESCE(SUM(total_tokens), 0)
         FROM usage_events WHERE measurement_kind = 'measured' AND occurred_at >= ?1
           AND superseded_by_event_id IS NULL
         GROUP BY day ORDER BY day ASC",
    )?;
    let rows = statement.query_map([start.to_rfc3339()], |row| {
        let date: String = row.get(0)?;
        let label = date.get(5..).unwrap_or(&date).replace('-', "/");
        Ok(TrendPoint {
            date,
            label,
            tokens: row.get(1)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn breakdown(
    connection: &Connection,
    column: &str,
    start: DateTime<Utc>,
    total: i64,
) -> Result<Vec<BreakdownItem>> {
    const ALLOWED: &[&str] = &[
        "provider",
        "COALESCE(model, 'Unknown model')",
        "COALESCE(project_name, 'Unknown project')",
    ];
    if !ALLOWED.contains(&column) {
        return Err(DatabaseError::Invalid("Unsupported breakdown".into()));
    }
    let query = format!(
        "SELECT {column}, COALESCE(SUM(total_tokens), 0) AS tokens
         FROM usage_events WHERE measurement_kind = 'measured' AND occurred_at >= ?1
           AND superseded_by_event_id IS NULL
         GROUP BY {column} ORDER BY tokens DESC LIMIT 8"
    );
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([start.to_rfc3339()], |row| {
        let key: String = row.get(0)?;
        let tokens: i64 = row.get(1)?;
        Ok(BreakdownItem {
            label: provider_label(&key),
            key,
            tokens,
            percentage: tokens as f64 * 100.0 / total as f64,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn breakdown_devices(
    connection: &Connection,
    start: DateTime<Utc>,
    total: i64,
) -> Result<Vec<BreakdownItem>> {
    let mut statement = connection.prepare(
        "SELECT d.id, d.friendly_name, COALESCE(SUM(u.total_tokens), 0) AS tokens
         FROM usage_events u JOIN devices d ON d.id = u.device_id
         WHERE u.measurement_kind = 'measured' AND u.occurred_at >= ?1
           AND u.superseded_by_event_id IS NULL
         GROUP BY d.id, d.friendly_name ORDER BY tokens DESC LIMIT 8",
    )?;
    let rows = statement.query_map([start.to_rfc3339()], |row| {
        let tokens: i64 = row.get(2)?;
        Ok(BreakdownItem {
            key: row.get(0)?,
            label: row.get(1)?,
            tokens,
            percentage: tokens as f64 * 100.0 / total as f64,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn activity(
    connection: &Connection,
    start: DateTime<Utc>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ActivityItem>> {
    let mut statement = connection.prepare(
        "SELECT u.id, u.provider, u.source, u.occurred_at, u.model, u.project_name, u.total_tokens,
                u.input_tokens, u.cached_input_tokens, u.cache_write_tokens, u.output_tokens,
                u.reasoning_tokens, u.native_cost_usd_ticks, u.estimated_api_value_usd_micros,
                u.measurement_kind, u.device_id, d.friendly_name
         FROM usage_events u JOIN devices d ON d.id = u.device_id
         WHERE u.occurred_at >= ?1 AND u.superseded_by_event_id IS NULL
         ORDER BY u.occurred_at DESC, u.id DESC LIMIT ?2 OFFSET ?3",
    )?;
    let rows = statement.query_map(params![start.to_rfc3339(), limit, offset], |row| {
        let date: String = row.get(3)?;
        let occurred_at = DateTime::parse_from_rfc3339(&date)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        Ok(ActivityItem {
            id: row.get(0)?,
            provider: row.get(1)?,
            source: row.get(2)?,
            occurred_at,
            model: row.get(4)?,
            project_name: row.get(5)?,
            total_tokens: row.get(6)?,
            input_tokens: row.get(7)?,
            cached_input_tokens: row.get(8)?,
            cache_write_tokens: row.get(9)?,
            output_tokens: row.get(10)?,
            reasoning_tokens: row.get(11)?,
            native_cost_usd_ticks: row.get(12)?,
            estimated_api_value_usd_micros: row.get(13)?,
            measurement_kind: row.get(14)?,
            device_id: row.get(15)?,
            device_name: row.get(16)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn insights(
    connection: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    total: i64,
) -> Result<Vec<Insight>> {
    if total <= 0 {
        return Ok(Vec::new());
    }
    let mut output = Vec::new();
    if let Some(top) = breakdown(connection, "provider", start, total)?
        .into_iter()
        .next()
    {
        output.push(Insight {
            id: "provider-share".into(),
            title: format!("{} leads measured usage", top.label),
            detail: format!(
                "{} represented {:.0}% of measured tokens in this period.",
                top.label, top.percentage
            ),
            tone: "provider".into(),
        });
    }
    if let Some(top) = breakdown(
        connection,
        "COALESCE(project_name, 'Unknown project')",
        start,
        total,
    )?
    .into_iter()
    .find(|item| item.key != "Unknown project")
    {
        output.push(Insight {
            id: "top-project".into(),
            title: format!("{} is the highest-usage project", top.label),
            detail: format!(
                "It accounted for {:.0}% of measured usage in this period.",
                top.percentage
            ),
            tone: "project".into(),
        });
    }
    let period = end.signed_duration_since(start);
    if period <= Duration::days(120) {
        let previous_start = start - period;
        let previous: i64 = connection.query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM usage_events
             WHERE measurement_kind = 'measured' AND occurred_at >= ?1 AND occurred_at < ?2
               AND superseded_by_event_id IS NULL",
            params![previous_start.to_rfc3339(), start.to_rfc3339()],
            |row| row.get(0),
        )?;
        if previous > 0 {
            let change = (total - previous) as f64 * 100.0 / previous as f64;
            output.push(Insight {
                id: "period-change".into(),
                title: if change >= 0.0 {
                    "Usage increased".into()
                } else {
                    "Usage eased".into()
                },
                detail: format!(
                    "Measured tokens changed by {:+.0}% versus the previous comparable period.",
                    change
                ),
                tone: "trend".into(),
            });
        }
    }
    Ok(output)
}

fn range_start(range: &str, now: DateTime<Utc>) -> DateTime<Utc> {
    match range {
        "today" => local_day_start(now),
        "7d" => local_day_start(now) - Duration::days(6),
        "30d" => local_day_start(now) - Duration::days(29),
        "all" => Utc
            .timestamp_opt(0, 0)
            .single()
            .unwrap_or(now - Duration::days(3650)),
        _ => local_month_start(now),
    }
}

fn local_day_start(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&Local);
    Local
        .with_ymd_and_hms(local.year(), local.month(), local.day(), 0, 0, 0)
        .single()
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or(now - Duration::hours(24))
}

fn local_month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&Local);
    Local
        .with_ymd_and_hms(local.year(), local.month(), 1, 0, 0, 0)
        .single()
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or(now - Duration::days(31))
}

fn provider_label(provider: &str) -> String {
    match provider {
        "codex" => "Codex".into(),
        "claude" => "Claude Code CLI".into(),
        "grok" => "Grok Build".into(),
        "gemini" => "Gemini CLI".into(),
        value => value.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{SourceType, TokenCounts, UsageEvent};

    #[test]
    fn dashboard_aggregates_measured_value_subscription_and_device() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("analytics.db")).unwrap();
        let device = database.ensure_device("test").unwrap();
        database.ensure_default_subscriptions().unwrap();
        let mut subscription = database.subscriptions().unwrap().remove(0);
        subscription.active = true;
        subscription.monthly_price_usd_cents = 2_000;
        subscription.updated_at = Utc::now();
        database.save_subscription(&subscription).unwrap();
        let event = UsageEvent::measured(
            "codex",
            "codex_cli",
            "session",
            "turn",
            Utc::now(),
            Some("gpt-5.6-sol".into()),
            Some("ArcMeter".into()),
            TokenCounts {
                input_tokens: 1_000_000,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 100_000,
                reasoning_tokens: 20_000,
                total_tokens: 1_100_000,
            },
            device.id.clone(),
        );
        database.insert_usage_events(&[event]).unwrap();
        assert_eq!(crate::pricing::reprice_events(&database).unwrap(), 1);

        let snapshot = dashboard(&database, "month", &ScanReport::default()).unwrap();
        assert_eq!(snapshot.metrics.measured_tokens_range, 1_100_000);
        assert_eq!(snapshot.metrics.priced_tokens_range, 1_100_000);
        assert_eq!(snapshot.metrics.priced_events_range, 1);
        assert_eq!(snapshot.metrics.monthly_subscription_usd_cents, 2_000);
        assert_eq!(
            snapshot.metrics.estimated_api_value_usd_micros,
            Some(11_000_000)
        );
        assert!(snapshot.metrics.pricing_complete);
        assert_eq!(snapshot.by_provider[0].key, "codex");
        assert_eq!(snapshot.by_project[0].label, "ArcMeter");
        assert_eq!(snapshot.by_device[0].key, device.id);
        assert!(
            snapshot
                .insights
                .iter()
                .any(|item| item.id == "provider-share")
        );
    }

    #[test]
    fn dashboard_keeps_safe_priced_subtotal_when_other_events_are_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("partial-pricing.db")).unwrap();
        let device = database.ensure_device("test").unwrap();
        database.ensure_default_subscriptions().unwrap();
        let mut subscription = database.subscriptions().unwrap().remove(0);
        subscription.active = true;
        subscription.monthly_price_usd_cents = 2_000;
        subscription.updated_at = Utc::now();
        database.save_subscription(&subscription).unwrap();

        let priced = UsageEvent::measured(
            "codex",
            "codex_cli",
            "priced-session",
            "priced-turn",
            Utc::now(),
            Some("gpt-5.6-sol".into()),
            Some("ArcMeter".into()),
            TokenCounts {
                input_tokens: 1_000_000,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 100_000,
                reasoning_tokens: 20_000,
                total_tokens: 1_100_000,
            },
            device.id.clone(),
        );
        let unavailable = UsageEvent::measured(
            "codex",
            "codex_cli",
            "review-session",
            "review-turn",
            Utc::now(),
            Some("codex-auto-review".into()),
            Some("ArcMeter".into()),
            TokenCounts {
                input_tokens: 100_000,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 10_000,
                reasoning_tokens: 2_000,
                total_tokens: 110_000,
            },
            device.id,
        );
        database
            .insert_usage_events(&[priced, unavailable])
            .unwrap();
        assert_eq!(crate::pricing::reprice_events(&database).unwrap(), 1);

        let snapshot = dashboard(&database, "month", &ScanReport::default()).unwrap();
        assert_eq!(snapshot.metrics.measured_events_range, 2);
        assert_eq!(snapshot.metrics.priced_events_range, 1);
        assert_eq!(snapshot.metrics.measured_tokens_range, 1_210_000);
        assert_eq!(snapshot.metrics.priced_tokens_range, 1_100_000);
        assert_eq!(
            snapshot.metrics.estimated_api_value_usd_micros,
            Some(11_000_000)
        );
        assert!(!snapshot.metrics.pricing_complete);
    }

    #[test]
    fn seven_day_range_starts_six_local_midnights_before_today() {
        let now = Utc
            .with_ymd_and_hms(2026, 8, 28, 19, 30, 0)
            .single()
            .unwrap();
        let start = range_start("7d", now);
        let elapsed = now.signed_duration_since(start);
        assert!(elapsed >= Duration::days(6));
        assert!(elapsed < Duration::days(7));
    }

    #[test]
    fn activity_minutes_never_inflate_measured_tokens_or_value() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("activity-analytics.db")).unwrap();
        let device = database.ensure_device("test").unwrap();
        database.ensure_default_subscriptions().unwrap();
        let minute = Utc::now().timestamp().div_euclid(60);
        let event = UsageEvent::activity(
            "claude",
            "claude_desktop",
            SourceType::Manual,
            minute,
            device.id,
        )
        .unwrap();
        database.insert_usage_events(&[event]).unwrap();

        let snapshot = dashboard(&database, "today", &ScanReport::default()).unwrap();
        assert_eq!(snapshot.metrics.activity_minutes_range, 1);
        assert_eq!(snapshot.metrics.measured_events_range, 0);
        assert_eq!(snapshot.metrics.priced_events_range, 0);
        assert_eq!(snapshot.metrics.measured_tokens_range, 0);
        assert_eq!(snapshot.metrics.priced_tokens_range, 0);
        assert_eq!(snapshot.metrics.estimated_api_value_usd_micros, None);
    }
}
