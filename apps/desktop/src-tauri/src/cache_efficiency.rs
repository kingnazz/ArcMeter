use crate::analytics::range_start;
use crate::db::{Database, Result};
use crate::domain::TokenCounts;
use crate::pricing::{
    InputTokenSemantics, PricingResult, PricingRule, calculate_cache_impact, load_pricing_rules,
    select_pricing_rule,
};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEfficiencySummary {
    pub semantic_coverage: CoverageState,
    pub fresh_input_tokens: Option<i64>,
    pub cached_input_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_write_5m_tokens: i64,
    pub cache_write_1h_tokens: i64,
    pub cache_write_unspecified_tokens: i64,
    pub normalized_input_context_tokens: Option<i64>,
    pub reuse_share_bps: Option<i64>,
    pub api_equivalent_cache_impact_usd_micros: Option<i64>,
    pub cache_pricing_coverage: CoverageState,
    pub measured_event_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEfficiencyBreakdown {
    pub key: String,
    pub label: String,
    pub provider: Option<String>,
    pub source: Option<String>,
    pub model: Option<String>,
    pub project: Option<String>,
    #[serde(flatten)]
    pub summary: CacheEfficiencySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEfficiencyReport {
    pub range: String,
    pub provider_filter: Option<String>,
    pub available_providers: Vec<String>,
    pub summary: CacheEfficiencySummary,
    pub by_provider: Vec<CacheEfficiencyBreakdown>,
    pub by_model: Vec<CacheEfficiencyBreakdown>,
    pub by_project: Vec<CacheEfficiencyBreakdown>,
}

#[derive(Debug, Clone)]
struct CacheEvent {
    provider: String,
    source: String,
    occurred_at: String,
    model: Option<String>,
    project: Option<String>,
    tokens: TokenCounts,
}

#[derive(Debug, Default, Clone)]
struct Accumulator {
    measured_event_count: i64,
    known_semantics_events: i64,
    fresh_input_tokens: i64,
    known_cached_input_tokens: i64,
    normalized_input_context_tokens: i64,
    cached_input_tokens: i64,
    cache_write_tokens: i64,
    cache_write_5m_tokens: i64,
    cache_write_1h_tokens: i64,
    impact_relevant_events: i64,
    impact_valued_events: i64,
    impact_complete_events: i64,
    impact_usd_micros: i64,
}

impl Accumulator {
    fn add(&mut self, event: &CacheEvent, rules: &[PricingRule]) {
        self.measured_event_count = self.measured_event_count.saturating_add(1);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(event.tokens.cached_input_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(event.tokens.cache_write_tokens);
        self.cache_write_5m_tokens = self
            .cache_write_5m_tokens
            .saturating_add(event.tokens.cache_write_5m_tokens);
        self.cache_write_1h_tokens = self
            .cache_write_1h_tokens
            .saturating_add(event.tokens.cache_write_1h_tokens);

        if let Some(semantics) = source_input_semantics(&event.provider, &event.source) {
            self.known_semantics_events = self.known_semantics_events.saturating_add(1);
            let fresh = fresh_input(&event.tokens, semantics);
            self.fresh_input_tokens = self.fresh_input_tokens.saturating_add(fresh);
            self.known_cached_input_tokens = self
                .known_cached_input_tokens
                .saturating_add(event.tokens.cached_input_tokens);
            self.normalized_input_context_tokens = self
                .normalized_input_context_tokens
                .saturating_add(fresh)
                .saturating_add(event.tokens.cached_input_tokens)
                .saturating_add(event.tokens.cache_write_tokens);
        }

        if event.tokens.cached_input_tokens <= 0 && event.tokens.cache_write_tokens <= 0 {
            return;
        }
        self.impact_relevant_events = self.impact_relevant_events.saturating_add(1);
        let Some(model) = event.model.as_deref() else {
            return;
        };
        let Some(rule) = select_pricing_rule(
            rules,
            &event.provider,
            model,
            &event.occurred_at,
            &event.tokens,
        ) else {
            return;
        };
        match calculate_cache_impact(&event.tokens, rule) {
            PricingResult::Available(value) => {
                self.impact_usd_micros = self.impact_usd_micros.saturating_add(value);
                self.impact_valued_events = self.impact_valued_events.saturating_add(1);
                self.impact_complete_events = self.impact_complete_events.saturating_add(1);
            }
            PricingResult::Partial(value) => {
                self.impact_usd_micros = self.impact_usd_micros.saturating_add(value);
                self.impact_valued_events = self.impact_valued_events.saturating_add(1);
            }
            PricingResult::Unavailable => {}
        }
    }

    fn finish(self) -> CacheEfficiencySummary {
        let semantic_coverage = if self.known_semantics_events == 0 {
            CoverageState::Unavailable
        } else if self.known_semantics_events == self.measured_event_count {
            CoverageState::Complete
        } else {
            CoverageState::Partial
        };
        let known = self.known_semantics_events > 0;
        let reuse_share_bps =
            (known && self.normalized_input_context_tokens > 0).then_some(basis_points(
                self.known_cached_input_tokens,
                self.normalized_input_context_tokens,
            ));
        let cache_pricing_coverage = if self.impact_valued_events == 0 {
            CoverageState::Unavailable
        } else if self.impact_complete_events == self.impact_relevant_events {
            CoverageState::Complete
        } else {
            CoverageState::Partial
        };
        let known_write_detail = self
            .cache_write_5m_tokens
            .saturating_add(self.cache_write_1h_tokens)
            .min(self.cache_write_tokens);
        CacheEfficiencySummary {
            semantic_coverage,
            fresh_input_tokens: known.then_some(self.fresh_input_tokens),
            cached_input_tokens: self.cached_input_tokens,
            cache_write_tokens: self.cache_write_tokens,
            cache_write_5m_tokens: self.cache_write_5m_tokens.min(self.cache_write_tokens),
            cache_write_1h_tokens: self.cache_write_1h_tokens.min(
                self.cache_write_tokens
                    .saturating_sub(self.cache_write_5m_tokens),
            ),
            cache_write_unspecified_tokens: self
                .cache_write_tokens
                .saturating_sub(known_write_detail),
            normalized_input_context_tokens: known.then_some(self.normalized_input_context_tokens),
            reuse_share_bps,
            api_equivalent_cache_impact_usd_micros: (self.impact_valued_events > 0)
                .then_some(self.impact_usd_micros),
            cache_pricing_coverage,
            measured_event_count: self.measured_event_count,
        }
    }
}

pub fn report(
    database: &Database,
    range: &str,
    provider: Option<&str>,
) -> Result<CacheEfficiencyReport> {
    let connection = Connection::open(database.path())?;
    let rules = load_pricing_rules(&connection)?;
    let provider = provider.map(str::trim).filter(|value| !value.is_empty());
    let start = range_start(range, Utc::now()).to_rfc3339();
    let available_providers = period_providers(&connection, &start)?;
    let events = period_events(&connection, &start, provider)?;
    Ok(build_report(
        range,
        provider,
        available_providers,
        &events,
        &rules,
    ))
}

pub fn session_summary(
    database: &Database,
    provider: &str,
    source: &str,
    native_session_id: &str,
) -> Result<CacheEfficiencySummary> {
    let connection = Connection::open(database.path())?;
    let rules = load_pricing_rules(&connection)?;
    let mut statement = connection.prepare(
        "SELECT provider, source, native_session_id, occurred_at, model, project_name,
                input_tokens, cached_input_tokens, cache_write_tokens,
                cache_write_5m_tokens, cache_write_1h_tokens, output_tokens,
                reasoning_tokens, total_tokens
         FROM usage_events
         WHERE measurement_kind = 'measured' AND superseded_by_event_id IS NULL
           AND provider = ?1 AND source = ?2 AND native_session_id = ?3",
    )?;
    let rows = statement.query_map(params![provider, source, native_session_id], map_event)?;
    let events = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(aggregate(&events, &rules))
}

fn period_events(
    connection: &Connection,
    start: &str,
    provider: Option<&str>,
) -> Result<Vec<CacheEvent>> {
    let mut statement = connection.prepare(
        "SELECT provider, source, native_session_id, occurred_at, model, project_name,
                input_tokens, cached_input_tokens, cache_write_tokens,
                cache_write_5m_tokens, cache_write_1h_tokens, output_tokens,
                reasoning_tokens, total_tokens
         FROM usage_events
         WHERE measurement_kind = 'measured' AND superseded_by_event_id IS NULL
           AND occurred_at >= ?1 AND (?2 = '' OR provider = ?2)",
    )?;
    let rows = statement.query_map(params![start, provider.unwrap_or_default()], map_event)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn period_providers(connection: &Connection, start: &str) -> Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT DISTINCT provider
         FROM usage_events
         WHERE measurement_kind = 'measured' AND superseded_by_event_id IS NULL
           AND occurred_at >= ?1
         ORDER BY provider",
    )?;
    let rows = statement.query_map([start], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<CacheEvent> {
    Ok(CacheEvent {
        provider: row.get(0)?,
        source: row.get(1)?,
        occurred_at: row.get(3)?,
        model: row.get(4)?,
        project: row.get(5)?,
        tokens: TokenCounts {
            input_tokens: row.get(6)?,
            cached_input_tokens: row.get(7)?,
            cache_write_tokens: row.get(8)?,
            cache_write_5m_tokens: row.get(9)?,
            cache_write_1h_tokens: row.get(10)?,
            output_tokens: row.get(11)?,
            reasoning_tokens: row.get(12)?,
            total_tokens: row.get(13)?,
        },
    })
}

fn build_report(
    range: &str,
    provider_filter: Option<&str>,
    available_providers: Vec<String>,
    events: &[CacheEvent],
    rules: &[PricingRule],
) -> CacheEfficiencyReport {
    let mut provider_groups: HashMap<String, (String, String, Accumulator)> = HashMap::new();
    let mut model_groups: HashMap<String, (String, String, Accumulator)> = HashMap::new();
    let mut project_groups: HashMap<String, Accumulator> = HashMap::new();
    for event in events {
        provider_groups
            .entry(format!("{}:{}", event.provider, event.source))
            .or_insert_with(|| {
                (
                    event.provider.clone(),
                    event.source.clone(),
                    Accumulator::default(),
                )
            })
            .2
            .add(event, rules);
        let model = event.model.as_deref().unwrap_or("Unknown model");
        model_groups
            .entry(format!("{}:{model}", event.provider))
            .or_insert_with(|| {
                (
                    event.provider.clone(),
                    model.to_owned(),
                    Accumulator::default(),
                )
            })
            .2
            .add(event, rules);
        project_groups
            .entry(event.project.clone().unwrap_or_else(|| "Unassigned".into()))
            .or_default()
            .add(event, rules);
    }

    let mut by_provider = provider_groups
        .into_iter()
        .map(
            |(key, (provider, source, accumulator))| CacheEfficiencyBreakdown {
                key,
                label: source_label(&provider, &source),
                provider: Some(provider),
                source: Some(source),
                model: None,
                project: None,
                summary: accumulator.finish(),
            },
        )
        .collect::<Vec<_>>();
    let mut by_model = model_groups
        .into_iter()
        .map(
            |(key, (provider, model, accumulator))| CacheEfficiencyBreakdown {
                key,
                label: model.clone(),
                provider: Some(provider),
                source: None,
                model: Some(model),
                project: None,
                summary: accumulator.finish(),
            },
        )
        .collect::<Vec<_>>();
    let mut by_project = project_groups
        .into_iter()
        .map(|(project, accumulator)| CacheEfficiencyBreakdown {
            key: project.clone(),
            label: project.clone(),
            provider: None,
            source: None,
            model: None,
            project: Some(project),
            summary: accumulator.finish(),
        })
        .collect::<Vec<_>>();
    sort_breakdowns(&mut by_provider);
    sort_breakdowns(&mut by_model);
    sort_breakdowns(&mut by_project);

    CacheEfficiencyReport {
        range: range.to_owned(),
        provider_filter: provider_filter.map(str::to_owned),
        available_providers,
        summary: aggregate(events, rules),
        by_provider,
        by_model,
        by_project,
    }
}

fn aggregate(events: &[CacheEvent], rules: &[PricingRule]) -> CacheEfficiencySummary {
    let mut accumulator = Accumulator::default();
    for event in events {
        accumulator.add(event, rules);
    }
    accumulator.finish()
}

fn sort_breakdowns(items: &mut [CacheEfficiencyBreakdown]) {
    items.sort_by(|left, right| {
        right
            .summary
            .cached_input_tokens
            .cmp(&left.summary.cached_input_tokens)
            .then_with(|| {
                right
                    .summary
                    .normalized_input_context_tokens
                    .cmp(&left.summary.normalized_input_context_tokens)
            })
            .then_with(|| left.label.cmp(&right.label))
    });
}

fn source_input_semantics(provider: &str, source: &str) -> Option<InputTokenSemantics> {
    match (provider, source) {
        ("codex", "codex_cli") | ("grok", "grok_build") | ("gemini", "gemini_cli") => {
            Some(InputTokenSemantics::CacheIncluded)
        }
        ("claude", "claude_code") => Some(InputTokenSemantics::CacheAdditive),
        _ => None,
    }
}

fn fresh_input(tokens: &TokenCounts, semantics: InputTokenSemantics) -> i64 {
    match semantics {
        InputTokenSemantics::CacheIncluded => tokens
            .input_tokens
            .saturating_sub(tokens.cached_input_tokens)
            .saturating_sub(tokens.cache_write_tokens)
            .max(0),
        InputTokenSemantics::CacheAdditive => tokens.input_tokens.max(0),
    }
}

fn basis_points(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        return 0;
    }
    ((numerator.max(0) as i128).saturating_mul(10_000) / denominator as i128).min(10_000) as i64
}

fn source_label(provider: &str, source: &str) -> String {
    match (provider, source) {
        ("codex", "codex_cli") => "Codex".into(),
        ("claude", "claude_code") => "Claude Code".into(),
        ("grok", "grok_build") => "Grok Build".into(),
        ("gemini", "gemini_cli") => "Gemini CLI".into(),
        _ => source.replace('_', " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{SourceType, UsageEvent};
    use chrono::Duration;
    use rusqlite::Connection;
    use std::time::{Duration as StdDuration, Instant};

    fn database() -> (tempfile::TempDir, Database, String) {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("cache.db")).unwrap();
        let device = database.ensure_device("test").unwrap();
        (directory, database, device.id)
    }

    fn event(
        provider: &str,
        source: &str,
        id: &str,
        model: Option<&str>,
        tokens: TokenCounts,
        device: &str,
    ) -> UsageEvent {
        UsageEvent::measured(
            provider,
            source,
            "session",
            id,
            Utc::now(),
            model.map(str::to_owned),
            Some("ArcMeter".into()),
            tokens,
            device,
        )
    }

    #[test]
    fn normalizes_included_and_additive_input_with_basis_point_precision() {
        let (_directory, database, device) = database();
        database
            .insert_usage_events(&[
                event(
                    "codex",
                    "codex_cli",
                    "included",
                    Some("gpt-5.6-sol"),
                    TokenCounts {
                        input_tokens: 1_000,
                        cached_input_tokens: 700,
                        cache_write_tokens: 100,
                        ..Default::default()
                    },
                    &device,
                ),
                event(
                    "claude",
                    "claude_code",
                    "additive",
                    Some("claude-sonnet-5"),
                    TokenCounts {
                        input_tokens: 200,
                        cached_input_tokens: 700,
                        cache_write_tokens: 100,
                        cache_write_5m_tokens: 100,
                        ..Default::default()
                    },
                    &device,
                ),
            ])
            .unwrap();
        let cache = report(&database, "7d", None).unwrap().summary;
        assert_eq!(cache.fresh_input_tokens, Some(400));
        assert_eq!(cache.normalized_input_context_tokens, Some(2_000));
        assert_eq!(cache.reuse_share_bps, Some(7_000));
        assert_eq!(cache.semantic_coverage, CoverageState::Complete);
        let codex = cache
            .by_provider
            .iter()
            .find(|item| item.provider.as_deref() == Some("codex"))
            .expect("Codex provider breakdown");
        assert_eq!(codex.cache_write_tokens, 100);
        assert_eq!(codex.fresh_input_tokens, Some(200));
        assert_eq!(codex.cache_write_unspecified_tokens, 100);
        assert_eq!(codex.reuse_share_bps, Some(7_000));
        let model = cache
            .by_model
            .iter()
            .find(|item| item.model.as_deref() == Some("gpt-5.6-sol"))
            .expect("Codex model breakdown");
        assert_eq!(model.cache_write_tokens, 100);
        let project = cache
            .by_project
            .iter()
            .find(|item| item.project.as_deref() == Some("ArcMeter"))
            .expect("Codex project breakdown");
        assert_eq!(project.cache_write_tokens, 100);
    }

    #[test]
    fn provider_semantics_zero_denominators_and_safe_subtraction_are_explicit() {
        assert_eq!(
            source_input_semantics("codex", "codex_cli"),
            Some(InputTokenSemantics::CacheIncluded)
        );
        assert_eq!(
            source_input_semantics("claude", "claude_code"),
            Some(InputTokenSemantics::CacheAdditive)
        );
        assert_eq!(
            source_input_semantics("grok", "grok_build"),
            Some(InputTokenSemantics::CacheIncluded)
        );
        assert_eq!(
            source_input_semantics("gemini", "gemini_cli"),
            Some(InputTokenSemantics::CacheIncluded)
        );
        assert_eq!(source_input_semantics("future", "future_cli"), None);
        assert_eq!(
            fresh_input(
                &TokenCounts {
                    input_tokens: 10,
                    cached_input_tokens: 20,
                    cache_write_tokens: 5,
                    ..Default::default()
                },
                InputTokenSemantics::CacheIncluded,
            ),
            0
        );
        assert_eq!(basis_points(0, 0), 0);
        assert_eq!(basis_points(0, 100), 0);
        assert_eq!(basis_points(1, 3), 3_333);
    }

    #[test]
    fn unknown_and_mixed_semantics_never_claim_complete_coverage() {
        let (_directory, database, device) = database();
        database
            .insert_usage_events(&[
                event(
                    "codex",
                    "codex_cli",
                    "known",
                    None,
                    TokenCounts {
                        input_tokens: 100,
                        cached_input_tokens: 50,
                        ..Default::default()
                    },
                    &device,
                ),
                event(
                    "future",
                    "future_cli",
                    "unknown",
                    None,
                    TokenCounts {
                        input_tokens: 100,
                        cached_input_tokens: 90,
                        ..Default::default()
                    },
                    &device,
                ),
            ])
            .unwrap();
        let cache = report(&database, "7d", None).unwrap().summary;
        assert_eq!(cache.semantic_coverage, CoverageState::Partial);
        assert_eq!(cache.reuse_share_bps, Some(5_000));
        let future = report(&database, "7d", Some("future")).unwrap().summary;
        assert_eq!(future.semantic_coverage, CoverageState::Unavailable);
        assert_eq!(future.reuse_share_bps, None);
        assert_eq!(future.cached_input_tokens, 90);
    }

    #[test]
    fn ttl_writes_are_subcategories_and_unspecified_is_safe() {
        let (_directory, database, device) = database();
        database
            .insert_usage_events(&[event(
                "claude",
                "claude_code",
                "ttl",
                None,
                TokenCounts {
                    input_tokens: 10,
                    cache_write_tokens: 100,
                    cache_write_5m_tokens: 60,
                    cache_write_1h_tokens: 30,
                    ..Default::default()
                },
                &device,
            )])
            .unwrap();
        let cache = report(&database, "7d", None).unwrap().summary;
        assert_eq!(cache.cache_write_tokens, 100);
        assert_eq!(cache.cache_write_5m_tokens, 60);
        assert_eq!(cache.cache_write_1h_tokens, 30);
        assert_eq!(cache.cache_write_unspecified_tokens, 10);
    }

    #[test]
    fn cache_pricing_reports_positive_negative_partial_and_unavailable() {
        let (_directory, database, device) = database();
        database
            .insert_usage_events(&[
                event(
                    "codex",
                    "codex_cli",
                    "saving",
                    Some("gpt-5.6-sol"),
                    TokenCounts {
                        input_tokens: 1_000_000,
                        cached_input_tokens: 500_000,
                        ..Default::default()
                    },
                    &device,
                ),
                event(
                    "claude",
                    "claude_code",
                    "overhead",
                    Some("claude-sonnet-5"),
                    TokenCounts {
                        cache_write_tokens: 1_000_000,
                        cache_write_5m_tokens: 1_000_000,
                        ..Default::default()
                    },
                    &device,
                ),
                event(
                    "claude",
                    "claude_code",
                    "partial",
                    Some("claude-sonnet-5"),
                    TokenCounts {
                        cache_write_tokens: 1,
                        ..Default::default()
                    },
                    &device,
                ),
            ])
            .unwrap();
        let cache = report(&database, "7d", None).unwrap().summary;
        assert!(cache.api_equivalent_cache_impact_usd_micros.unwrap() > 0);
        assert_eq!(cache.cache_pricing_coverage, CoverageState::Partial);
        let claude = report(&database, "7d", Some("claude")).unwrap().summary;
        assert!(claude.api_equivalent_cache_impact_usd_micros.unwrap() < 0);
        let unknown = report(&database, "7d", Some("grok")).unwrap().summary;
        assert_eq!(unknown.cache_pricing_coverage, CoverageState::Unavailable);
    }

    #[test]
    fn canonical_range_filter_and_breakdowns_exclude_activity_and_superseded() {
        let (_directory, database, device) = database();
        let valid = event(
            "grok",
            "grok_build",
            "valid",
            None,
            TokenCounts {
                input_tokens: 1_000,
                cached_input_tokens: 600,
                cache_write_tokens: 100,
                ..Default::default()
            },
            &device,
        );
        let mut superseded = event(
            "grok",
            "grok_build",
            "old",
            None,
            TokenCounts {
                input_tokens: 9_000,
                cached_input_tokens: 9_000,
                ..Default::default()
            },
            &device,
        );
        superseded.id = "f".repeat(64);
        let activity = UsageEvent::activity(
            "grok",
            "grok_web",
            SourceType::Browser,
            Utc::now().timestamp().div_euclid(60),
            &device,
        )
        .unwrap();
        database
            .insert_usage_events(&[valid, superseded.clone(), activity])
            .unwrap();
        Connection::open(database.path())
            .unwrap()
            .execute(
                "UPDATE usage_events SET superseded_by_event_id = 'replacement' WHERE id = ?1",
                [&superseded.id],
            )
            .unwrap();
        let cache = report(&database, "7d", Some("grok")).unwrap();
        assert_eq!(cache.summary.measured_event_count, 1);
        assert_eq!(cache.summary.cached_input_tokens, 600);
        assert_eq!(cache.by_provider.len(), 1);
        assert_eq!(cache.by_model.len(), 1);
        assert_eq!(cache.by_project[0].label, "ArcMeter");
    }

    #[test]
    fn provider_discovery_is_range_scoped_canonical_and_filter_independent() {
        let (_directory, database, device) = database();
        let mut claude = event(
            "claude",
            "claude_code",
            "claude-old",
            None,
            TokenCounts {
                input_tokens: 100,
                cached_input_tokens: 20,
                ..Default::default()
            },
            &device,
        );
        claude.occurred_at = Utc::now() - Duration::days(8);
        let codex = event(
            "codex",
            "codex_cli",
            "codex-recent",
            None,
            TokenCounts {
                input_tokens: 100,
                cached_input_tokens: 10,
                ..Default::default()
            },
            &device,
        );
        let grok = event(
            "grok",
            "grok_build",
            "grok-recent",
            None,
            TokenCounts {
                input_tokens: 100,
                cached_input_tokens: 30,
                ..Default::default()
            },
            &device,
        );
        let mut superseded = event(
            "gemini",
            "gemini_cli",
            "superseded",
            None,
            TokenCounts {
                input_tokens: 999,
                cached_input_tokens: 999,
                ..Default::default()
            },
            &device,
        );
        superseded.id = "e".repeat(64);
        let activity = UsageEvent::activity(
            "browser_only",
            "browser",
            SourceType::Browser,
            Utc::now().timestamp().div_euclid(60),
            &device,
        )
        .unwrap();
        database
            .insert_usage_events(&[claude, codex, grok, superseded.clone(), activity])
            .unwrap();
        Connection::open(database.path())
            .unwrap()
            .execute(
                "UPDATE usage_events SET superseded_by_event_id = 'replacement' WHERE id = ?1",
                [&superseded.id],
            )
            .unwrap();

        let all_30d = report(&database, "30d", None).unwrap();
        let claude_30d = report(&database, "30d", Some("claude")).unwrap();
        assert_eq!(
            claude_30d.available_providers,
            vec!["claude", "codex", "grok"]
        );
        assert_eq!(claude_30d.summary.measured_event_count, 1);
        assert_eq!(claude_30d.summary.cached_input_tokens, 20);
        assert_eq!(claude_30d.by_provider.len(), 1);
        assert_eq!(
            claude_30d.by_provider[0].provider.as_deref(),
            Some("claude")
        );
        assert_eq!(all_30d.summary.cached_input_tokens, 60);

        let empty_claude_7d = report(&database, "7d", Some("claude")).unwrap();
        assert_eq!(empty_claude_7d.available_providers, vec!["codex", "grok"]);
        assert_eq!(empty_claude_7d.summary.measured_event_count, 0);
        assert_eq!(empty_claude_7d.summary.cached_input_tokens, 0);
    }

    #[test]
    fn today_seven_thirty_all_and_provider_filters_are_event_period_bounded() {
        let (_directory, database, device) = database();
        let mut old = event(
            "codex",
            "codex_cli",
            "old",
            None,
            TokenCounts {
                input_tokens: 100,
                cached_input_tokens: 10,
                ..Default::default()
            },
            &device,
        );
        old.occurred_at = Utc::now() - Duration::days(8);
        let recent = event(
            "claude",
            "claude_code",
            "recent",
            None,
            TokenCounts {
                input_tokens: 100,
                cached_input_tokens: 20,
                ..Default::default()
            },
            &device,
        );
        database.insert_usage_events(&[old, recent]).unwrap();
        assert_eq!(
            report(&database, "today", None)
                .unwrap()
                .summary
                .measured_event_count,
            1
        );
        assert_eq!(
            report(&database, "7d", None)
                .unwrap()
                .summary
                .measured_event_count,
            1
        );
        assert_eq!(
            report(&database, "30d", None)
                .unwrap()
                .summary
                .measured_event_count,
            2
        );
        assert_eq!(
            report(&database, "all", Some("codex"))
                .unwrap()
                .summary
                .measured_event_count,
            1
        );
    }

    #[test]
    fn session_summary_reconciles_exact_canonical_counters() {
        let (_directory, database, device) = database();
        database
            .insert_usage_events(&[event(
                "codex",
                "codex_cli",
                "one",
                None,
                TokenCounts {
                    input_tokens: 100,
                    cached_input_tokens: 40,
                    cache_write_tokens: 20,
                    ..Default::default()
                },
                &device,
            )])
            .unwrap();
        let summary = session_summary(&database, "codex", "codex_cli", "session").unwrap();
        assert_eq!(summary.cached_input_tokens, 40);
        assert_eq!(summary.cache_write_tokens, 20);
        assert_eq!(summary.cache_write_5m_tokens, 0);
        assert_eq!(summary.cache_write_1h_tokens, 0);
        assert_eq!(summary.cache_write_unspecified_tokens, 20);
        assert_eq!(summary.fresh_input_tokens, Some(40));
        assert_eq!(summary.reuse_share_bps, Some(4_000));
    }

    #[test]
    fn ten_thousand_event_seven_day_query_is_comfortably_bounded() {
        let (_directory, database, device) = database();
        let mut events = Vec::with_capacity(10_075);
        for index in 0..10_050 {
            let provider = if index % 3 == 0 {
                "claude"
            } else if index % 3 == 1 {
                "codex"
            } else {
                "grok"
            };
            let source = if provider == "claude" {
                "claude_code"
            } else if provider == "codex" {
                "codex_cli"
            } else {
                "grok_build"
            };
            events.push(UsageEvent::measured(
                provider,
                source,
                format!("session-{}", index % 100),
                format!("event-{index}"),
                Utc::now(),
                Some(if provider == "claude" {
                    if index % 2 == 0 {
                        "claude-sonnet-5".into()
                    } else {
                        "claude-opus-5".into()
                    }
                } else {
                    if index % 2 == 0 {
                        "gpt-5.6-sol".into()
                    } else {
                        "gpt-5.6-terra".into()
                    }
                }),
                Some(format!("Project-{}", index % 20)),
                TokenCounts {
                    input_tokens: 1_000,
                    cached_input_tokens: 600,
                    cache_write_tokens: 100,
                    cache_write_5m_tokens: if provider == "claude" { 100 } else { 0 },
                    ..Default::default()
                },
                &device,
            ));
        }
        for index in 0..25 {
            events.push(event(
                "claude",
                "claude_code",
                &format!("superseded-{index}"),
                Some("claude-sonnet-5"),
                TokenCounts {
                    input_tokens: 9_000,
                    cached_input_tokens: 9_000,
                    ..Default::default()
                },
                &device,
            ));
        }
        database.insert_usage_events(&events).unwrap();
        Connection::open(database.path())
            .unwrap()
            .execute(
                "UPDATE usage_events SET superseded_by_event_id = 'replacement'
                 WHERE native_event_id LIKE 'superseded-%'",
                [],
            )
            .unwrap();
        let activity = (0..25)
            .map(|offset| {
                UsageEvent::activity(
                    "grok",
                    "grok_web",
                    SourceType::Browser,
                    Utc::now().timestamp().div_euclid(60) - offset,
                    &device,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        database.insert_usage_events(&activity).unwrap();
        let started = Instant::now();
        let cache = report(&database, "7d", None).unwrap();
        assert_eq!(cache.summary.measured_event_count, 10_050);
        assert!(started.elapsed() < StdDuration::from_secs(5));
    }
}
