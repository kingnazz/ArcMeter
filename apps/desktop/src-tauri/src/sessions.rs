use crate::db::{Database, DatabaseError, Result};
use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SESSION_PAGE_SIZE: i64 = 50;
const EVENT_PAGE_SIZE: i64 = 100;

/// The filter identifies sessions, rather than the individual events to total.
/// Parameter order: range start, provider, normalized search text, search pattern.
const SESSION_FILTER_CTES: &str = "WITH eligible_events AS (
    SELECT u.provider, u.source, u.native_session_id, u.occurred_at, u.model,
           u.project_name, u.input_tokens, u.cached_input_tokens, u.cache_write_tokens,
           u.cache_write_5m_tokens, u.cache_write_1h_tokens, u.output_tokens,
           u.reasoning_tokens, u.total_tokens, u.estimated_api_value_usd_micros,
           u.native_cost_usd_ticks, u.pricing_status, u.device_id, d.friendly_name
    FROM usage_events u JOIN devices d ON d.id = u.device_id
    WHERE u.measurement_kind = 'measured' AND u.superseded_by_event_id IS NULL
), session_bounds AS (
    SELECT provider, source, native_session_id, MAX(occurred_at) AS last_activity_at
    FROM eligible_events GROUP BY provider, source, native_session_id
), candidate_sessions AS (
    SELECT b.provider, b.source, b.native_session_id
    FROM session_bounds b
    WHERE b.last_activity_at >= ?1
      AND (?2 = '' OR b.provider = ?2)
      AND (?3 = '' OR EXISTS (
        SELECT 1 FROM eligible_events matching
        WHERE matching.provider = b.provider AND matching.source = b.source
          AND matching.native_session_id = b.native_session_id
          AND lower(COALESCE(matching.project_name, '') || ' ' || matching.provider || ' ' ||
              matching.source || ' ' || COALESCE(matching.model, '') || ' ' ||
              matching.friendly_name) LIKE ?4
      ))
)";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuery {
    pub range: String,
    pub provider: Option<String>,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    pub sessions: Vec<SessionSummary>,
    pub total_count: i64,
    pub stats: SessionStats,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub session_count: i64,
    pub total_tokens: i64,
    pub estimated_api_value_usd_micros: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_key: String,
    pub provider: String,
    pub source: String,
    /// Retained only as an opaque native lookup key for the detail command.
    /// The renderer deliberately never displays it as a session title.
    pub native_session_id: String,
    pub project_name: String,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub duration_seconds: i64,
    pub event_count: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_write_5m_tokens: i64,
    pub cache_write_1h_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
    pub estimated_api_value_usd_micros: Option<i64>,
    pub native_cost_usd_ticks: Option<i64>,
    pub pricing_coverage: String,
    pub primary_model: String,
    pub model_count: i64,
    pub device_count: i64,
    pub primary_device_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: SessionSummary,
    pub models: Vec<SessionModel>,
    pub devices: Vec<String>,
    pub events: Vec<SessionTimelineItem>,
    pub events_has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModel {
    pub model: String,
    pub tokens: i64,
    pub event_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTimelineItem {
    pub occurred_at: DateTime<Utc>,
    pub model: String,
    pub total_tokens: i64,
    pub estimated_api_value_usd_micros: Option<i64>,
}

pub fn session_page(database: &Database, query: SessionQuery) -> Result<SessionPage> {
    let connection = Connection::open(database.path())?;
    let start = session_range_start(&query.range, Utc::now());
    let provider = query.provider.unwrap_or_default().trim().to_owned();
    let search = query.search.unwrap_or_default().trim().to_lowercase();
    let search_like = format!("%{search}%");
    let limit = query
        .limit
        .unwrap_or(SESSION_PAGE_SIZE)
        .clamp(1, SESSION_PAGE_SIZE);
    let offset = query.offset.unwrap_or_default().max(0);
    let order = session_sort(query.sort.as_deref());
    let statistics = session_stats(&connection, start, &provider, &search, &search_like)?;
    let sql = format!(
        "{SESSION_FILTER_CTES}, candidate_events AS (
           SELECT e.* FROM eligible_events e JOIN candidate_sessions c
             ON c.provider = e.provider AND c.source = e.source
            AND c.native_session_id = e.native_session_id
         ), session_aggregates AS (
           SELECT provider, source, native_session_id, MIN(occurred_at) AS started_at,
                  MAX(occurred_at) AS last_activity_at, COUNT(*) AS event_count,
                  COALESCE(SUM(input_tokens), 0) AS input_tokens,
                  COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
                  COALESCE(SUM(cache_write_tokens), 0) AS cache_write_tokens,
                  COALESCE(SUM(cache_write_5m_tokens), 0) AS cache_write_5m_tokens,
                  COALESCE(SUM(cache_write_1h_tokens), 0) AS cache_write_1h_tokens,
                  COALESCE(SUM(output_tokens), 0) AS output_tokens,
                  COALESCE(SUM(reasoning_tokens), 0) AS reasoning_tokens,
                  COALESCE(SUM(total_tokens), 0) AS total_tokens,
                  SUM(CASE WHEN total_tokens > 0 AND pricing_status IN ('available', 'partial')
                            AND estimated_api_value_usd_micros IS NOT NULL
                           THEN estimated_api_value_usd_micros END) AS api_value,
                  SUM(native_cost_usd_ticks) AS native_cost,
                  SUM(CASE WHEN total_tokens > 0 THEN 1 ELSE 0 END) AS relevant_events,
                  SUM(CASE WHEN total_tokens > 0 AND pricing_status = 'available'
                            AND estimated_api_value_usd_micros IS NOT NULL THEN 1 ELSE 0 END) AS fully_priced_events,
                  SUM(CASE WHEN total_tokens > 0 AND pricing_status IN ('available', 'partial')
                            AND estimated_api_value_usd_micros IS NOT NULL THEN 1 ELSE 0 END) AS defensible_value_events,
                  COUNT(DISTINCT device_id) AS device_count, MIN(friendly_name) AS primary_device_name
           FROM candidate_events GROUP BY provider, source, native_session_id
         ), project_ranked AS (
           SELECT provider, source, native_session_id, project_name,
                  ROW_NUMBER() OVER (
                    PARTITION BY provider, source, native_session_id
                    ORDER BY COUNT(*) DESC, MAX(occurred_at) DESC, project_name ASC
                  ) AS rank
           FROM candidate_events WHERE project_name IS NOT NULL AND project_name <> ''
           GROUP BY provider, source, native_session_id, project_name
         ), model_ranked AS (
           SELECT provider, source, native_session_id, COALESCE(model, 'Unknown model') AS model,
                  COUNT(*) AS event_count,
                  ROW_NUMBER() OVER (
                    PARTITION BY provider, source, native_session_id
                    ORDER BY SUM(total_tokens) DESC, COUNT(*) DESC,
                      COALESCE(model, 'Unknown model') ASC
                  ) AS rank
           FROM candidate_events
           GROUP BY provider, source, native_session_id, COALESCE(model, 'Unknown model')
         )
         SELECT a.provider, a.source, a.native_session_id, a.started_at, a.last_activity_at,
                a.event_count, a.input_tokens, a.cached_input_tokens, a.cache_write_tokens,
                a.cache_write_5m_tokens, a.cache_write_1h_tokens, a.output_tokens,
                a.reasoning_tokens, a.total_tokens, a.api_value, a.native_cost,
                a.relevant_events, a.fully_priced_events, a.defensible_value_events,
                COALESCE(p.project_name, 'Unassigned'), COALESCE(m.model, 'Unknown model'),
                (SELECT COUNT(*) FROM model_ranked all_models
                 WHERE all_models.provider = a.provider AND all_models.source = a.source
                   AND all_models.native_session_id = a.native_session_id),
                a.device_count, a.primary_device_name
         FROM session_aggregates a
         LEFT JOIN project_ranked p ON p.provider = a.provider AND p.source = a.source
           AND p.native_session_id = a.native_session_id AND p.rank = 1
         LEFT JOIN model_ranked m ON m.provider = a.provider AND m.source = a.source
           AND m.native_session_id = a.native_session_id AND m.rank = 1
         ORDER BY {order}
         LIMIT ?5 OFFSET ?6"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            start.to_rfc3339(),
            provider,
            search,
            search_like,
            limit + 1,
            offset
        ],
        map_session_summary,
    )?;
    let mut sessions = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    let has_more = sessions.len() as i64 > limit;
    sessions.truncate(limit as usize);
    Ok(SessionPage {
        total_count: statistics.session_count,
        stats: statistics,
        sessions,
        has_more,
    })
}

pub fn session_detail(
    database: &Database,
    provider: String,
    source: String,
    native_session_id: String,
    limit: i64,
    offset: i64,
) -> Result<SessionDetail> {
    let connection = Connection::open(database.path())?;
    let summary = detail_summary(&connection, &provider, &source, &native_session_id)?
        .ok_or_else(|| DatabaseError::Invalid("Session is unavailable".into()))?;
    let models = detail_models(&connection, &provider, &source, &native_session_id)?;
    let devices = detail_devices(&connection, &provider, &source, &native_session_id)?;
    let (events, events_has_more) = detail_events(
        &connection,
        &provider,
        &source,
        &native_session_id,
        limit.clamp(1, EVENT_PAGE_SIZE),
        offset.max(0),
    )?;
    Ok(SessionDetail {
        session: SessionSummary {
            primary_model: models
                .first()
                .map(|model| model.model.clone())
                .unwrap_or_else(|| "Unknown model".into()),
            model_count: models.len() as i64,
            device_count: devices.len() as i64,
            primary_device_name: devices
                .first()
                .cloned()
                .unwrap_or_else(|| "Unknown device".into()),
            ..summary
        },
        models,
        devices,
        events,
        events_has_more,
    })
}

fn session_stats(
    connection: &Connection,
    start: DateTime<Utc>,
    provider: &str,
    search: &str,
    search_like: &str,
) -> Result<SessionStats> {
    let sql = format!(
        "{SESSION_FILTER_CTES}, candidate_events AS (
           SELECT e.* FROM eligible_events e JOIN candidate_sessions c
             ON c.provider = e.provider AND c.source = e.source
            AND c.native_session_id = e.native_session_id
         ), sessions AS (
           SELECT provider, source, native_session_id, COALESCE(SUM(total_tokens), 0) AS tokens,
                  SUM(CASE WHEN total_tokens > 0 AND pricing_status IN ('available', 'partial')
                            AND estimated_api_value_usd_micros IS NOT NULL
                           THEN estimated_api_value_usd_micros END) AS api_value
           FROM candidate_events GROUP BY provider, source, native_session_id
         )
         SELECT COUNT(*), COALESCE(SUM(tokens), 0), SUM(api_value) FROM sessions"
    );
    connection
        .query_row(
            &sql,
            params![start.to_rfc3339(), provider, search, search_like],
            |row| {
                Ok(SessionStats {
                    session_count: row.get(0)?,
                    total_tokens: row.get(1)?,
                    estimated_api_value_usd_micros: row.get(2)?,
                })
            },
        )
        .map_err(Into::into)
}

fn detail_summary(
    connection: &Connection,
    provider: &str,
    source: &str,
    native_session_id: &str,
) -> Result<Option<SessionSummary>> {
    let aggregate = connection
        .query_row(
            "SELECT MIN(u.occurred_at), MAX(u.occurred_at), COUNT(*),
                    COALESCE(SUM(u.input_tokens), 0), COALESCE(SUM(u.cached_input_tokens), 0),
                    COALESCE(SUM(u.cache_write_tokens), 0), COALESCE(SUM(u.cache_write_5m_tokens), 0),
                    COALESCE(SUM(u.cache_write_1h_tokens), 0), COALESCE(SUM(u.output_tokens), 0),
                    COALESCE(SUM(u.reasoning_tokens), 0), COALESCE(SUM(u.total_tokens), 0),
                    SUM(CASE WHEN u.total_tokens > 0 AND u.pricing_status IN ('available', 'partial')
                              AND u.estimated_api_value_usd_micros IS NOT NULL
                             THEN u.estimated_api_value_usd_micros END),
                    SUM(u.native_cost_usd_ticks),
                    SUM(CASE WHEN u.total_tokens > 0 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN u.total_tokens > 0 AND u.pricing_status = 'available'
                              AND u.estimated_api_value_usd_micros IS NOT NULL THEN 1 ELSE 0 END),
                    SUM(CASE WHEN u.total_tokens > 0
                              AND u.pricing_status IN ('available', 'partial')
                              AND u.estimated_api_value_usd_micros IS NOT NULL THEN 1 ELSE 0 END),
                    MIN(d.friendly_name)
             FROM usage_events u JOIN devices d ON d.id = u.device_id
             WHERE u.measurement_kind = 'measured' AND u.superseded_by_event_id IS NULL
               AND u.provider = ?1 AND u.source = ?2 AND u.native_session_id = ?3
             HAVING COUNT(*) > 0",
            params![provider, source, native_session_id],
            |row| {
                let started_at = parse_datetime(row.get(0)?, 0)?;
                let last_activity_at = parse_datetime(row.get(1)?, 1)?;
                Ok(SessionSummary {
                    session_key: session_key(provider, source, native_session_id),
                    provider: provider.to_owned(),
                    source: source.to_owned(),
                    native_session_id: native_session_id.to_owned(),
                    project_name: "Unassigned".into(),
                    started_at,
                    last_activity_at,
                    duration_seconds: (last_activity_at - started_at).num_seconds().max(0),
                    event_count: row.get(2)?,
                    input_tokens: row.get(3)?,
                    cached_input_tokens: row.get(4)?,
                    cache_write_tokens: row.get(5)?,
                    cache_write_5m_tokens: row.get(6)?,
                    cache_write_1h_tokens: row.get(7)?,
                    output_tokens: row.get(8)?,
                    reasoning_tokens: row.get(9)?,
                    total_tokens: row.get(10)?,
                    estimated_api_value_usd_micros: row.get(11)?,
                    native_cost_usd_ticks: row.get(12)?,
                    pricing_coverage: pricing_coverage(row.get(13)?, row.get(14)?, row.get(15)?),
                    primary_model: "Unknown model".into(),
                    model_count: 0,
                    device_count: 0,
                    primary_device_name: row.get(16)?,
                })
            },
        )
        .optional()?;
    let Some(mut summary) = aggregate else {
        return Ok(None);
    };
    summary.project_name = connection
        .query_row(
            "SELECT project_name FROM usage_events
             WHERE measurement_kind = 'measured' AND superseded_by_event_id IS NULL
               AND provider = ?1 AND source = ?2 AND native_session_id = ?3
               AND project_name IS NOT NULL AND project_name <> ''
             GROUP BY project_name ORDER BY COUNT(*) DESC, MAX(occurred_at) DESC, project_name ASC LIMIT 1",
            params![provider, source, native_session_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "Unassigned".into());
    Ok(Some(summary))
}

fn detail_models(
    connection: &Connection,
    provider: &str,
    source: &str,
    native_session_id: &str,
) -> Result<Vec<SessionModel>> {
    let mut statement = connection.prepare(
        "SELECT COALESCE(model, 'Unknown model'), COALESCE(SUM(total_tokens), 0), COUNT(*)
         FROM usage_events WHERE measurement_kind = 'measured' AND superseded_by_event_id IS NULL
           AND provider = ?1 AND source = ?2 AND native_session_id = ?3
         GROUP BY COALESCE(model, 'Unknown model')
         ORDER BY SUM(total_tokens) DESC, COUNT(*) DESC, COALESCE(model, 'Unknown model') ASC",
    )?;
    let rows = statement.query_map(params![provider, source, native_session_id], |row| {
        Ok(SessionModel {
            model: row.get(0)?,
            tokens: row.get(1)?,
            event_count: row.get(2)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn detail_devices(
    connection: &Connection,
    provider: &str,
    source: &str,
    native_session_id: &str,
) -> Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT DISTINCT d.friendly_name FROM usage_events u JOIN devices d ON d.id = u.device_id
         WHERE u.measurement_kind = 'measured' AND u.superseded_by_event_id IS NULL
           AND u.provider = ?1 AND u.source = ?2 AND u.native_session_id = ?3
         ORDER BY d.friendly_name ASC",
    )?;
    let rows = statement.query_map(params![provider, source, native_session_id], |row| {
        row.get(0)
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn detail_events(
    connection: &Connection,
    provider: &str,
    source: &str,
    native_session_id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SessionTimelineItem>, bool)> {
    let mut statement = connection.prepare(
        "SELECT occurred_at, COALESCE(model, 'Unknown model'), total_tokens,
                estimated_api_value_usd_micros
         FROM usage_events WHERE measurement_kind = 'measured' AND superseded_by_event_id IS NULL
           AND provider = ?1 AND source = ?2 AND native_session_id = ?3
         ORDER BY occurred_at ASC, id ASC LIMIT ?4 OFFSET ?5",
    )?;
    let rows = statement.query_map(
        params![provider, source, native_session_id, limit + 1, offset],
        |row| {
            Ok(SessionTimelineItem {
                occurred_at: parse_datetime(row.get(0)?, 0)?,
                model: row.get(1)?,
                total_tokens: row.get(2)?,
                estimated_api_value_usd_micros: row.get(3)?,
            })
        },
    )?;
    let mut events = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    let has_more = events.len() as i64 > limit;
    events.truncate(limit as usize);
    Ok((events, has_more))
}

fn map_session_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSummary> {
    let provider: String = row.get(0)?;
    let source: String = row.get(1)?;
    let native_session_id: String = row.get(2)?;
    let started_at = parse_datetime(row.get(3)?, 3)?;
    let last_activity_at = parse_datetime(row.get(4)?, 4)?;
    Ok(SessionSummary {
        session_key: session_key(&provider, &source, &native_session_id),
        provider,
        source,
        native_session_id,
        project_name: row.get(19)?,
        started_at,
        last_activity_at,
        duration_seconds: (last_activity_at - started_at).num_seconds().max(0),
        event_count: row.get(5)?,
        input_tokens: row.get(6)?,
        cached_input_tokens: row.get(7)?,
        cache_write_tokens: row.get(8)?,
        cache_write_5m_tokens: row.get(9)?,
        cache_write_1h_tokens: row.get(10)?,
        output_tokens: row.get(11)?,
        reasoning_tokens: row.get(12)?,
        total_tokens: row.get(13)?,
        estimated_api_value_usd_micros: row.get(14)?,
        native_cost_usd_ticks: row.get(15)?,
        pricing_coverage: pricing_coverage(row.get(16)?, row.get(17)?, row.get(18)?),
        primary_model: row.get(20)?,
        model_count: row.get(21)?,
        device_count: row.get(22)?,
        primary_device_name: row.get(23)?,
    })
}

fn parse_datetime(value: String, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
}

fn pricing_coverage(
    relevant_events: i64,
    fully_priced_events: i64,
    defensible_value_events: i64,
) -> String {
    if relevant_events > 0 && fully_priced_events == relevant_events {
        "complete".into()
    } else if defensible_value_events > 0 {
        "partial".into()
    } else {
        "unavailable".into()
    }
}

fn session_key(provider: &str, source: &str, native_session_id: &str) -> String {
    let material = format!("{provider}\u{1f}{source}\u{1f}{native_session_id}");
    hex::encode(Sha256::digest(material.as_bytes()))
}

fn session_sort(value: Option<&str>) -> &'static str {
    match value {
        Some("tokens") => "a.total_tokens DESC, a.last_activity_at DESC, a.native_session_id ASC",
        Some("value") => "a.api_value DESC, a.last_activity_at DESC, a.native_session_id ASC",
        Some("duration") => {
            "(julianday(a.last_activity_at) - julianday(a.started_at)) DESC, a.last_activity_at DESC, a.native_session_id ASC"
        }
        _ => "a.last_activity_at DESC, a.native_session_id ASC",
    }
}

fn session_range_start(range: &str, now: DateTime<Utc>) -> DateTime<Utc> {
    match range {
        "today" => local_day_start(now),
        "7d" => local_day_start(now) - Duration::days(6),
        "30d" => local_day_start(now) - Duration::days(29),
        "all" => Utc
            .timestamp_opt(0, 0)
            .single()
            .unwrap_or(now - Duration::days(3650)),
        _ => local_day_start(now) - Duration::days(29),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MeasurementKind, SourceType, TokenCounts, UsageEvent};
    use std::time::{Duration as StdDuration, Instant};

    fn database() -> (tempfile::TempDir, Database, String) {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("sessions.db")).unwrap();
        let device = database.ensure_device("test").unwrap();
        (directory, database, device.id)
    }

    #[allow(clippy::too_many_arguments)]
    fn event(
        provider: &str,
        source: &str,
        session: &str,
        event_id: &str,
        minute: i64,
        model: Option<&str>,
        project: Option<&str>,
        device_id: &str,
    ) -> UsageEvent {
        UsageEvent::measured(
            provider,
            source,
            session,
            event_id,
            "2026-09-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap() + Duration::minutes(minute),
            model.map(str::to_owned),
            project.map(str::to_owned),
            TokenCounts {
                input_tokens: 100,
                cached_input_tokens: 200,
                cache_write_tokens: 30,
                cache_write_5m_tokens: 20,
                cache_write_1h_tokens: 10,
                output_tokens: 50,
                reasoning_tokens: 15,
                total_tokens: 350,
            },
            device_id,
        )
    }

    fn all_query() -> SessionQuery {
        SessionQuery {
            range: "all".into(),
            provider: None,
            search: None,
            sort: None,
            limit: None,
            offset: None,
        }
    }

    #[test]
    fn groups_canonical_measured_events_by_provider_source_and_native_session() {
        let (_directory, database, device) = database();
        let mut first = event(
            "claude",
            "claude_code",
            "alpha",
            "one",
            0,
            Some("opus"),
            Some("ArcMeter"),
            &device,
        );
        first.estimated_api_value_usd_micros = Some(1_000);
        let mut second = event(
            "claude",
            "claude_code",
            "alpha",
            "two",
            20,
            Some("sonnet"),
            Some("ArcMeter"),
            &device,
        );
        second.estimated_api_value_usd_micros = Some(2_000);
        first.pricing_status = "available".into();
        second.pricing_status = "partial".into();
        database.insert_usage_events(&[first, second]).unwrap();
        let page = session_page(&database, all_query()).unwrap();
        assert_eq!(page.total_count, 1);
        let session = &page.sessions[0];
        assert_eq!(session.event_count, 2);
        assert_eq!(session.total_tokens, 700);
        assert_eq!(session.duration_seconds, 20 * 60);
        assert_eq!(session.project_name, "ArcMeter");
        assert_eq!(session.primary_model, "opus");
        assert_eq!(session.pricing_coverage, "partial");
        assert_eq!(session.estimated_api_value_usd_micros, Some(3_000));
    }

    #[test]
    fn separates_provider_and_source_collisions_and_excludes_activity_and_superseded_rows() {
        let (_directory, database, device) = database();
        let first = event(
            "claude",
            "claude_code",
            "shared",
            "one",
            0,
            None,
            None,
            &device,
        );
        let second = event(
            "grok",
            "grok_build",
            "shared",
            "two",
            1,
            None,
            None,
            &device,
        );
        let third = event(
            "claude",
            "other_cli",
            "shared",
            "three",
            2,
            None,
            None,
            &device,
        );
        let mut superseded = event(
            "claude",
            "claude_code",
            "ignored",
            "four",
            3,
            None,
            None,
            &device,
        );
        superseded.id = "f".repeat(64);
        let activity =
            UsageEvent::activity("claude", "claude_desktop", SourceType::Manual, 1, &device)
                .unwrap();
        database
            .insert_usage_events(&[first, second, third, superseded.clone(), activity])
            .unwrap();
        Connection::open(database.path())
            .unwrap()
            .execute(
                "UPDATE usage_events SET superseded_by_event_id = 'replacement' WHERE id = ?1",
                [&superseded.id],
            )
            .unwrap();
        let page = session_page(&database, all_query()).unwrap();
        assert_eq!(page.total_count, 3);
        assert!(
            page.sessions
                .iter()
                .all(|session| session.total_tokens == 350)
        );
    }

    #[test]
    fn detail_preserves_token_semantics_models_devices_and_native_cost_separately() {
        let (_directory, database, device) = database();
        let timestamp = Utc::now().to_rfc3339();
        Connection::open(database.path()).unwrap().execute(
            "INSERT INTO devices(id,friendly_name,os,architecture,app_version,created_at,last_seen_at,sync_status)
             VALUES('mac','MacBook','macos','aarch64','test',?1,?1,'synced')",
            [&timestamp],
        ).unwrap();
        let mut first = event(
            "grok",
            "grok_build",
            "costly",
            "one",
            0,
            Some("grok-fast"),
            Some("ArcMeter"),
            &device,
        )
        .with_native_cost_usd_ticks(Some(120_000_000));
        first.estimated_api_value_usd_micros = Some(1_000);
        let second = event(
            "grok",
            "grok_build",
            "costly",
            "two",
            5,
            Some("grok-reasoning"),
            Some("TDVR"),
            "mac",
        );
        database.insert_usage_events(&[first, second]).unwrap();
        let detail = session_detail(
            &database,
            "grok".into(),
            "grok_build".into(),
            "costly".into(),
            100,
            0,
        )
        .unwrap();
        assert_eq!(detail.session.total_tokens, 700);
        assert_eq!(detail.session.reasoning_tokens, 30);
        assert_eq!(detail.session.native_cost_usd_ticks, Some(120_000_000));
        assert_eq!(detail.models.len(), 2);
        assert_eq!(detail.devices.len(), 2);
        assert!(detail.devices.contains(&"MacBook".into()));
        assert_eq!(detail.events.len(), 2);
    }

    #[test]
    fn filters_and_paginates_stably_without_duplicate_sessions() {
        let (_directory, database, device) = database();
        let events = (0..60)
            .map(|index| {
                event(
                    "codex",
                    "codex_cli",
                    &format!("session-{index}"),
                    "turn",
                    index,
                    Some("gpt"),
                    Some(if index % 2 == 0 { "ArcMeter" } else { "TDVR" }),
                    &device,
                )
            })
            .collect::<Vec<_>>();
        database.insert_usage_events(&events).unwrap();
        let mut query = all_query();
        query.provider = Some("codex".into());
        query.search = Some("ArcMeter".into());
        query.limit = Some(10);
        let first = session_page(&database, query.clone()).unwrap();
        assert_eq!(first.total_count, 30);
        assert_eq!(first.sessions.len(), 10);
        assert!(first.has_more);
        query.offset = Some(10);
        let second = session_page(&database, query).unwrap();
        let first_keys = first
            .sessions
            .iter()
            .map(|item| &item.session_key)
            .collect::<Vec<_>>();
        assert!(
            second
                .sessions
                .iter()
                .all(|item| !first_keys.contains(&&item.session_key))
        );
        let mut changed_filter = all_query();
        changed_filter.provider = Some("codex".into());
        changed_filter.search = Some("TDVR".into());
        changed_filter.limit = Some(10);
        let changed_first_page = session_page(&database, changed_filter).unwrap();
        assert_eq!(changed_first_page.sessions.len(), 10);
        assert!(
            changed_first_page
                .sessions
                .iter()
                .all(|item| item.project_name == "TDVR")
        );
    }

    #[test]
    fn canonical_cumulative_revision_counts_once_and_event_timeline_is_bounded() {
        let (_directory, database, device) = database();
        let mut original = event(
            "claude",
            "claude_code",
            "revision",
            "request:revision",
            0,
            Some("opus"),
            None,
            &device,
        );
        original.tokens.total_tokens = 100;
        let mut richer = original.clone();
        richer.tokens.total_tokens = 900;
        richer.tokens.output_tokens = 150;
        richer.occurred_at += Duration::minutes(2);
        database.upsert_claude_request_events(&[original]).unwrap();
        database.upsert_claude_request_events(&[richer]).unwrap();
        let page = session_page(&database, all_query()).unwrap();
        assert_eq!(page.sessions[0].event_count, 1);
        assert_eq!(page.sessions[0].total_tokens, 900);
        let detail = session_detail(
            &database,
            "claude".into(),
            "claude_code".into(),
            "revision".into(),
            1,
            0,
        )
        .unwrap();
        assert_eq!(detail.events.len(), 1);
        assert!(!detail.events_has_more);
    }

    #[test]
    fn aggregates_ten_thousand_events_with_one_bounded_session_query() {
        let (_directory, database, device) = database();
        let mut events = Vec::with_capacity(10_000);
        for session in 0..1_000 {
            for turn in 0..10 {
                events.push(event(
                    "codex",
                    "codex_cli",
                    &format!("bulk-{session}"),
                    &format!("turn-{turn}"),
                    (session * 10 + turn) as i64,
                    Some("gpt"),
                    Some("Bulk"),
                    &device,
                ));
            }
        }
        database.insert_usage_events(&events).unwrap();
        let started = Instant::now();
        let page = session_page(&database, all_query()).unwrap();
        assert_eq!(page.total_count, 1_000);
        assert_eq!(page.sessions.len(), 50);
        assert!(started.elapsed() < StdDuration::from_secs(5));
    }

    #[test]
    fn search_qualifies_a_session_without_truncating_its_models_devices_totals_or_stats() {
        let (_directory, database, device) = database();
        let timestamp = Utc::now().to_rfc3339();
        Connection::open(database.path())
            .unwrap()
            .execute(
                "INSERT INTO devices(id,friendly_name,os,architecture,app_version,created_at,last_seen_at,sync_status)
                 VALUES('mac','MacBook','macos','aarch64','test',?1,?1,'synced')",
                [&timestamp],
            )
            .unwrap();

        let mut opus = event(
            "claude",
            "claude_code",
            "complete",
            "opus",
            0,
            Some("Claude Opus"),
            Some("ArcMeter"),
            &device,
        );
        opus.tokens.total_tokens = 800_000;
        let mut sonnet = event(
            "claude",
            "claude_code",
            "complete",
            "sonnet",
            10,
            Some("Claude Sonnet"),
            Some("ArcMeter"),
            "mac",
        );
        sonnet.tokens.total_tokens = 200_000;
        let mut smaller = event(
            "claude",
            "claude_code",
            "smaller",
            "turn",
            20,
            Some("Claude Haiku"),
            Some("Other"),
            &device,
        );
        smaller.tokens.total_tokens = 400_000;
        let mut other_provider = event(
            "grok",
            "grok_build",
            "other-provider",
            "turn",
            30,
            Some("Grok Opus Adapter"),
            Some("Elsewhere"),
            &device,
        );
        other_provider.tokens.total_tokens = 50_000;
        database
            .insert_usage_events(&[opus, sonnet, smaller, other_provider])
            .unwrap();

        let mut query = all_query();
        query.search = Some("Opus".into());
        let searched = session_page(&database, query.clone()).unwrap();
        assert_eq!(searched.total_count, 2);
        let complete = searched
            .sessions
            .iter()
            .find(|item| item.native_session_id == "complete")
            .unwrap();
        assert_eq!(complete.total_tokens, 1_000_000);
        assert_eq!(complete.model_count, 2);
        assert_eq!(complete.device_count, 2);
        assert_eq!(searched.stats.total_tokens, 1_050_000);

        query.provider = Some("claude".into());
        let provider_filtered = session_page(&database, query).unwrap();
        assert_eq!(provider_filtered.total_count, 1);
        assert_eq!(provider_filtered.sessions[0].total_tokens, 1_000_000);

        let mut project_query = all_query();
        project_query.search = Some("ArcMeter".into());
        assert_eq!(
            session_page(&database, project_query).unwrap().sessions[0].total_tokens,
            1_000_000
        );
        let mut device_query = all_query();
        device_query.search = Some("MacBook".into());
        assert_eq!(
            session_page(&database, device_query).unwrap().sessions[0].total_tokens,
            1_000_000
        );

        let mut sort_query = all_query();
        sort_query.sort = Some("tokens".into());
        assert_eq!(
            session_page(&database, sort_query).unwrap().sessions[0].native_session_id,
            "complete"
        );
        let detail = session_detail(
            &database,
            "claude".into(),
            "claude_code".into(),
            "complete".into(),
            100,
            0,
        )
        .unwrap();
        assert_eq!(detail.session.total_tokens, complete.total_tokens);
        assert_eq!(detail.models.len(), 2);
    }

    #[test]
    fn range_qualifies_by_last_activity_without_truncating_a_cross_midnight_session() {
        let (_directory, database, device) = database();
        let day_start = local_day_start(Utc::now());
        let mut before_midnight = event(
            "codex",
            "codex_cli",
            "cross-midnight",
            "before",
            0,
            Some("gpt"),
            Some("ArcMeter"),
            &device,
        );
        before_midnight.occurred_at = day_start - Duration::minutes(10);
        before_midnight.tokens.total_tokens = 1_000_000;
        let mut after_midnight = event(
            "codex",
            "codex_cli",
            "cross-midnight",
            "after",
            0,
            Some("gpt"),
            Some("ArcMeter"),
            &device,
        );
        after_midnight.occurred_at = day_start + Duration::minutes(20);
        after_midnight.tokens.total_tokens = 400_000;
        database
            .insert_usage_events(&[before_midnight, after_midnight])
            .unwrap();
        let mut query = all_query();
        query.range = "today".into();
        let page = session_page(&database, query).unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.sessions[0].total_tokens, 1_400_000);
        assert_eq!(page.stats.total_tokens, 1_400_000);
        assert_eq!(
            page.sessions[0].started_at,
            day_start - Duration::minutes(10)
        );
    }

    #[test]
    fn pricing_coverage_keeps_partial_subtotals_and_matches_detail() {
        let (_directory, database, device) = database();
        let cases = [
            (
                "all-available",
                [
                    ("available", Some(2_000_000)),
                    ("available", Some(3_000_000)),
                ],
                "complete",
                Some(5_000_000),
            ),
            (
                "available-partial",
                [("available", Some(2_000_000)), ("partial", Some(1_000_000))],
                "partial",
                Some(3_000_000),
            ),
            (
                "partial-only",
                [("partial", Some(4_000_000)), ("partial", Some(2_000_000))],
                "partial",
                Some(6_000_000),
            ),
            (
                "available-unavailable",
                [("available", Some(3_000_000)), ("unavailable", None)],
                "partial",
                Some(3_000_000),
            ),
            (
                "partial-unavailable",
                [("partial", Some(2_000_000)), ("unavailable", None)],
                "partial",
                Some(2_000_000),
            ),
            (
                "unavailable-only",
                [("unavailable", None), ("unavailable", None)],
                "unavailable",
                None,
            ),
        ];
        let mut events = Vec::new();
        for (session, prices, _, _) in cases {
            for (index, (status, value)) in prices.into_iter().enumerate() {
                let mut item = event(
                    "claude",
                    "claude_code",
                    session,
                    &format!("turn-{index}"),
                    index as i64,
                    Some("Claude"),
                    Some("ArcMeter"),
                    &device,
                );
                item.pricing_status = status.into();
                item.estimated_api_value_usd_micros = value;
                if session == "partial-only" && index == 0 {
                    item.native_cost_usd_ticks = Some(120_000_000);
                }
                events.push(item);
            }
        }
        database.insert_usage_events(&events).unwrap();
        let page = session_page(&database, all_query()).unwrap();
        assert_eq!(page.stats.estimated_api_value_usd_micros, Some(19_000_000));
        for (session_id, _, expected_coverage, expected_value) in cases {
            let listed = page
                .sessions
                .iter()
                .find(|item| item.native_session_id == session_id)
                .unwrap();
            assert_eq!(listed.pricing_coverage, expected_coverage);
            assert_eq!(listed.estimated_api_value_usd_micros, expected_value);
            let detail = session_detail(
                &database,
                "claude".into(),
                "claude_code".into(),
                session_id.into(),
                100,
                0,
            )
            .unwrap();
            assert_eq!(detail.session.pricing_coverage, listed.pricing_coverage);
            assert_eq!(
                detail.session.estimated_api_value_usd_micros,
                listed.estimated_api_value_usd_micros
            );
        }
        let partial_only = page
            .sessions
            .iter()
            .find(|item| item.native_session_id == "partial-only")
            .unwrap();
        assert_eq!(partial_only.native_cost_usd_ticks, Some(120_000_000));
        assert_eq!(partial_only.estimated_api_value_usd_micros, Some(6_000_000));
    }

    #[test]
    fn measured_kind_is_the_only_session_eligible_kind() {
        let (_directory, database, device) = database();
        let measured = event(
            "codex",
            "codex_cli",
            "measured",
            "turn",
            0,
            None,
            None,
            &device,
        );
        let mut estimated = event(
            "codex",
            "codex_cli",
            "estimated",
            "turn",
            1,
            None,
            None,
            &device,
        );
        estimated.measurement_kind = MeasurementKind::Estimated;
        let activity =
            UsageEvent::activity("grok", "grok_web", SourceType::Browser, 2, &device).unwrap();
        database
            .insert_usage_events(&[measured, estimated, activity])
            .unwrap();
        assert_eq!(session_page(&database, all_query()).unwrap().total_count, 1);
    }
}
