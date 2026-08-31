use crate::db::{Database, DatabaseError, Result as DatabaseResult};
use crate::domain::{TokenCounts, UsageEvent};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingRule {
    pub provider: String,
    pub model_pattern: String,
    pub effective_from: DateTime<Utc>,
    pub min_input_tokens: i64,
    pub max_input_tokens: Option<i64>,
    pub input_usd_micros_per_million: i64,
    pub cached_input_usd_micros_per_million: Option<i64>,
    pub cache_write_5m_usd_micros_per_million: Option<i64>,
    pub cache_write_1h_usd_micros_per_million: Option<i64>,
    pub input_token_semantics: InputTokenSemantics,
    pub output_usd_micros_per_million: i64,
    pub reasoning_pricing_behavior: ReasoningPricingBehavior,
    pub reasoning_usd_micros_per_million: Option<i64>,
    pub version: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputTokenSemantics {
    CacheIncluded,
    CacheAdditive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningPricingBehavior {
    IncludedInOutput,
    Separate,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricingResult {
    Available(i64),
    Partial(i64),
    Unavailable,
}

pub fn calculate_api_value(tokens: &TokenCounts, rule: &PricingRule) -> PricingResult {
    let pricing_input_tokens = match rule.input_token_semantics {
        InputTokenSemantics::CacheIncluded => tokens.input_tokens,
        InputTokenSemantics::CacheAdditive => tokens
            .input_tokens
            .saturating_add(tokens.cached_input_tokens)
            .saturating_add(tokens.cache_write_tokens),
    };
    if pricing_input_tokens < rule.min_input_tokens
        || rule
            .max_input_tokens
            .is_some_and(|maximum| pricing_input_tokens > maximum)
    {
        return PricingResult::Unavailable;
    }
    let fresh_input = match rule.input_token_semantics {
        InputTokenSemantics::CacheIncluded => tokens
            .input_tokens
            .saturating_sub(tokens.cached_input_tokens)
            .saturating_sub(tokens.cache_write_tokens)
            .max(0),
        InputTokenSemantics::CacheAdditive => tokens.input_tokens,
    };
    let mut total = priced(fresh_input, rule.input_usd_micros_per_million).saturating_add(priced(
        tokens.output_tokens,
        rule.output_usd_micros_per_million,
    ));
    let mut complete = true;
    add_optional_component(
        &mut total,
        &mut complete,
        tokens.cached_input_tokens,
        rule.cached_input_usd_micros_per_million,
    );

    let cache_write_5m = tokens.cache_write_5m_tokens.min(tokens.cache_write_tokens);
    let cache_write_1h = tokens
        .cache_write_1h_tokens
        .min(tokens.cache_write_tokens.saturating_sub(cache_write_5m));
    add_optional_component(
        &mut total,
        &mut complete,
        cache_write_5m,
        rule.cache_write_5m_usd_micros_per_million,
    );
    add_optional_component(
        &mut total,
        &mut complete,
        cache_write_1h,
        rule.cache_write_1h_usd_micros_per_million,
    );
    if tokens
        .cache_write_tokens
        .saturating_sub(cache_write_5m)
        .saturating_sub(cache_write_1h)
        > 0
    {
        // An aggregate-only cache write has no defensible TTL-specific rate.
        complete = false;
    }

    if rule.reasoning_pricing_behavior == ReasoningPricingBehavior::Separate {
        add_optional_component(
            &mut total,
            &mut complete,
            tokens.reasoning_tokens,
            rule.reasoning_usd_micros_per_million,
        );
    } else if rule.reasoning_pricing_behavior == ReasoningPricingBehavior::Unavailable
        && tokens.reasoning_tokens > 0
    {
        complete = false;
    }
    if complete {
        PricingResult::Available(total)
    } else {
        PricingResult::Partial(total)
    }
}

fn add_optional_component(total: &mut i64, complete: &mut bool, tokens: i64, rate: Option<i64>) {
    if tokens <= 0 {
        return;
    }
    if let Some(rate) = rate {
        *total = total.saturating_add(priced(tokens, rate));
    } else {
        *complete = false;
    }
}

/// Revalues measured events from versioned, local pricing metadata. Unknown models and
/// unsupported context tiers intentionally remain unavailable.
pub fn reprice_events(database: &Database) -> DatabaseResult<usize> {
    let mut connection = Connection::open(database.path())?;
    let events = {
        let mut statement = connection.prepare(
            "SELECT id, provider, model, occurred_at, input_tokens, cached_input_tokens,
                    cache_write_tokens, cache_write_5m_tokens, cache_write_1h_tokens,
                    output_tokens, reasoning_tokens, total_tokens,
                    estimated_api_value_usd_micros, pricing_status
             FROM usage_events
             WHERE measurement_kind = 'measured' AND model IS NOT NULL
               AND superseded_by_event_id IS NULL",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(PricingInput {
                id: row.get(0)?,
                provider: row.get(1)?,
                model: row.get(2)?,
                occurred_at: row.get(3)?,
                tokens: TokenCounts {
                    input_tokens: row.get(4)?,
                    cached_input_tokens: row.get(5)?,
                    cache_write_tokens: row.get(6)?,
                    cache_write_5m_tokens: row.get(7)?,
                    cache_write_1h_tokens: row.get(8)?,
                    output_tokens: row.get(9)?,
                    reasoning_tokens: row.get(10)?,
                    total_tokens: row.get(11)?,
                },
                previous_value: row.get(12)?,
                previous_status: row.get(13)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };

    let transaction = connection.transaction()?;
    let mut changed = 0;
    for event in events {
        let (value, status) = price_event_fields(
            &transaction,
            &event.provider,
            &event.model,
            &event.occurred_at,
            &event.tokens,
        )?;
        if event.previous_value == value && event.previous_status == status {
            continue;
        }
        changed += transaction.execute(
            "UPDATE usage_events
             SET estimated_api_value_usd_micros = ?2, pricing_status = ?3,
                 updated_at = ?4, sync_status = 'pending'
             WHERE id = ?1",
            params![event.id, value, status, Utc::now().to_rfc3339()],
        )?;
    }
    transaction.commit()?;
    Ok(changed)
}

/// Prices parsed events before they enter the ledger. This lets a cumulative
/// Claude revision update its counters and derived value in one local write.
pub fn price_usage_events(database: &Database, events: &mut [UsageEvent]) -> DatabaseResult<()> {
    let connection = Connection::open(database.path())?;
    for event in events {
        if event.model.is_none() {
            event.model = connection
                .query_row(
                    "SELECT model FROM usage_events
                     WHERE id = ?1 AND provider = ?2 AND source = ?3 AND source_type = ?4
                       AND native_session_id = ?5 AND native_event_id = ?6 AND device_id = ?7
                       AND measurement_kind = ?8 AND superseded_by_event_id IS NULL",
                    params![
                        event.id,
                        event.provider,
                        event.source,
                        event.source_type.as_str(),
                        event.native_session_id,
                        event.native_event_id,
                        event.device_id,
                        event.measurement_kind.as_str(),
                    ],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
        }
        let Some(model) = event.model.as_deref() else {
            event.estimated_api_value_usd_micros = None;
            event.pricing_status = "unavailable".into();
            continue;
        };
        let (value, status) = price_event_fields(
            &connection,
            &event.provider,
            model,
            &event.occurred_at.to_rfc3339(),
            &event.tokens,
        )?;
        event.estimated_api_value_usd_micros = value;
        event.pricing_status = status.into();
    }
    Ok(())
}

fn price_event_fields(
    connection: &Connection,
    provider: &str,
    model: &str,
    occurred_at: &str,
    tokens: &TokenCounts,
) -> DatabaseResult<(Option<i64>, &'static str)> {
    Ok(
        match find_rule(connection, provider, model, occurred_at, tokens)?
            .as_ref()
            .map(|rule| calculate_api_value(tokens, rule))
        {
            Some(PricingResult::Available(value)) => (Some(value), "available"),
            Some(PricingResult::Partial(value)) => (Some(value), "partial"),
            _ => (None, "unavailable"),
        },
    )
}

#[derive(Debug)]
struct PricingInput {
    id: String,
    provider: String,
    model: String,
    occurred_at: String,
    tokens: TokenCounts,
    previous_value: Option<i64>,
    previous_status: String,
}

fn find_rule(
    connection: &Connection,
    provider: &str,
    model: &str,
    occurred_at: &str,
    tokens: &TokenCounts,
) -> DatabaseResult<Option<PricingRule>> {
    let mut statement = connection.prepare(
        "SELECT provider, model_pattern, effective_from, min_input_tokens, max_input_tokens,
                input_usd_micros_per_million, cached_input_usd_micros_per_million,
                cache_write_5m_usd_micros_per_million,
                cache_write_1h_usd_micros_per_million, input_token_semantics,
                output_usd_micros_per_million, reasoning_pricing_behavior,
                reasoning_usd_micros_per_million, version
         FROM pricing
         WHERE provider = ?1 AND effective_from <= ?3
         ORDER BY (model_pattern = ?2) DESC, length(model_pattern) DESC,
                  effective_from DESC, min_input_tokens DESC, version DESC",
    )?;
    let mut rows = statement.query(params![provider, model, occurred_at])?;
    while let Some(row) = rows.next()? {
        let model_pattern: String = row.get(1)?;
        let model_matches = model_pattern
            .strip_suffix('*')
            .map_or(model_pattern == model, |prefix| model.starts_with(prefix));
        if !model_matches {
            continue;
        }
        let effective_from: String = row.get(2)?;
        let effective_from = DateTime::parse_from_rfc3339(&effective_from)
            .map(|date| date.with_timezone(&Utc))
            .map_err(|error| DatabaseError::Invalid(format!("invalid pricing date: {error}")))?;
        let input_semantics: String = row.get(9)?;
        let input_token_semantics = match input_semantics.as_str() {
            "cache_included" => InputTokenSemantics::CacheIncluded,
            "cache_additive" => InputTokenSemantics::CacheAdditive,
            value => {
                return Err(DatabaseError::Invalid(format!(
                    "invalid input token semantics: {value}"
                )));
            }
        };
        let pricing_input_tokens = match input_token_semantics {
            InputTokenSemantics::CacheIncluded => tokens.input_tokens,
            InputTokenSemantics::CacheAdditive => tokens
                .input_tokens
                .saturating_add(tokens.cached_input_tokens)
                .saturating_add(tokens.cache_write_tokens),
        };
        let min_input_tokens: i64 = row.get(3)?;
        let max_input_tokens: Option<i64> = row.get(4)?;
        if pricing_input_tokens < min_input_tokens
            || max_input_tokens.is_some_and(|maximum| pricing_input_tokens > maximum)
        {
            continue;
        }
        let reasoning_behavior: String = row.get(11)?;
        let reasoning_pricing_behavior = match reasoning_behavior.as_str() {
            "included_in_output" => ReasoningPricingBehavior::IncludedInOutput,
            "separate" => ReasoningPricingBehavior::Separate,
            "unavailable" => ReasoningPricingBehavior::Unavailable,
            value => {
                return Err(DatabaseError::Invalid(format!(
                    "invalid reasoning pricing behavior: {value}"
                )));
            }
        };
        return Ok(Some(PricingRule {
            provider: row.get(0)?,
            model_pattern,
            effective_from,
            min_input_tokens,
            max_input_tokens,
            input_usd_micros_per_million: row.get(5)?,
            cached_input_usd_micros_per_million: row.get(6)?,
            cache_write_5m_usd_micros_per_million: row.get(7)?,
            cache_write_1h_usd_micros_per_million: row.get(8)?,
            input_token_semantics,
            output_usd_micros_per_million: row.get(10)?,
            reasoning_pricing_behavior,
            reasoning_usd_micros_per_million: row.get(12)?,
            version: row.get(13)?,
        }));
    }
    Ok(None)
}

fn priced(tokens: i64, usd_micros_per_million: i64) -> i64 {
    let value =
        (tokens.max(0) as i128).saturating_mul(usd_micros_per_million.max(0) as i128) / 1_000_000;
    value.min(i64::MAX as i128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> PricingRule {
        PricingRule {
            provider: "example".into(),
            model_pattern: "model".into(),
            effective_from: Utc::now(),
            min_input_tokens: 0,
            max_input_tokens: None,
            input_usd_micros_per_million: 1_000_000,
            cached_input_usd_micros_per_million: Some(250_000),
            cache_write_5m_usd_micros_per_million: Some(1_250_000),
            cache_write_1h_usd_micros_per_million: Some(2_000_000),
            input_token_semantics: InputTokenSemantics::CacheIncluded,
            output_usd_micros_per_million: 4_000_000,
            reasoning_pricing_behavior: ReasoningPricingBehavior::IncludedInOutput,
            reasoning_usd_micros_per_million: None,
            version: 1,
        }
    }

    #[test]
    fn cached_input_is_not_double_priced() {
        let tokens = TokenCounts {
            input_tokens: 1_000_000,
            cached_input_tokens: 400_000,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: 100_000,
            reasoning_tokens: 20_000,
            total_tokens: 1_100_000,
        };
        assert_eq!(
            calculate_api_value(&tokens, &rule()),
            PricingResult::Available(1_100_000)
        );
    }

    #[test]
    fn unknown_cache_or_reasoning_components_return_a_safe_partial_value() {
        let mut no_cache = rule();
        no_cache.cached_input_usd_micros_per_million = None;
        assert_eq!(
            calculate_api_value(
                &TokenCounts {
                    cached_input_tokens: 1,
                    input_tokens: 1,
                    ..Default::default()
                },
                &no_cache
            ),
            PricingResult::Partial(0)
        );
        let mut no_reasoning = rule();
        no_reasoning.reasoning_pricing_behavior = ReasoningPricingBehavior::Unavailable;
        assert_eq!(
            calculate_api_value(
                &TokenCounts {
                    reasoning_tokens: 1,
                    ..Default::default()
                },
                &no_reasoning
            ),
            PricingResult::Partial(0)
        );
    }

    #[test]
    fn additive_claude_components_are_each_priced_once() {
        let mut claude = rule();
        claude.input_token_semantics = InputTokenSemantics::CacheAdditive;
        let tokens = TokenCounts {
            input_tokens: 1_000_000,
            cached_input_tokens: 1_000_000,
            cache_write_tokens: 2_000_000,
            cache_write_5m_tokens: 1_000_000,
            cache_write_1h_tokens: 1_000_000,
            output_tokens: 1_000_000,
            reasoning_tokens: 100_000,
            total_tokens: 5_000_000,
        };
        assert_eq!(
            calculate_api_value(&tokens, &claude),
            PricingResult::Available(8_500_000)
        );
    }

    #[test]
    fn aggregate_only_cache_write_is_not_assigned_a_guessed_ttl() {
        let mut claude = rule();
        claude.input_token_semantics = InputTokenSemantics::CacheAdditive;
        assert_eq!(
            calculate_api_value(
                &TokenCounts {
                    input_tokens: 1_000_000,
                    cache_write_tokens: 1_000_000,
                    total_tokens: 2_000_000,
                    ..Default::default()
                },
                &claude
            ),
            PricingResult::Partial(1_000_000)
        );
    }

    #[test]
    fn context_tiers_are_enforced() {
        let mut limited = rule();
        limited.max_input_tokens = Some(200_000);
        assert_eq!(
            calculate_api_value(
                &TokenCounts {
                    input_tokens: 200_001,
                    ..Default::default()
                },
                &limited
            ),
            PricingResult::Unavailable
        );
    }

    #[test]
    fn seeded_pricing_selects_the_exact_context_tier() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("pricing.db")).unwrap();
        let connection = Connection::open(database.path()).unwrap();
        let rule = find_rule(
            &connection,
            "codex",
            "gpt-5.6-sol",
            &Utc::now().to_rfc3339(),
            &TokenCounts {
                input_tokens: 1_000_000,
                ..Default::default()
            },
        )
        .unwrap()
        .expect("a long-context GPT-5.6 Sol rule");
        assert_eq!(rule.min_input_tokens, 272_001);
        assert_eq!(rule.input_usd_micros_per_million, 8_000_000);
    }

    #[test]
    fn seeded_claude_pricing_uses_additive_cache_rates_and_safe_context_limits() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("claude-pricing.db")).unwrap();
        let connection = Connection::open(database.path()).unwrap();
        let sonnet = find_rule(
            &connection,
            "claude",
            "claude-sonnet-5-20260801",
            "2026-08-29T00:00:00Z",
            &TokenCounts {
                input_tokens: 100_000,
                cached_input_tokens: 50_000,
                cache_write_tokens: 25_000,
                ..Default::default()
            },
        )
        .unwrap()
        .expect("a Claude Sonnet 5 rule");
        assert_eq!(
            sonnet.input_token_semantics,
            InputTokenSemantics::CacheAdditive
        );
        assert_eq!(
            sonnet.cache_write_5m_usd_micros_per_million,
            Some(2_500_000)
        );
        assert_eq!(
            sonnet.cache_write_1h_usd_micros_per_million,
            Some(4_000_000)
        );

        let haiku_over_limit = find_rule(
            &connection,
            "claude",
            "claude-haiku-4-5-20251001",
            "2026-08-29T00:00:00Z",
            &TokenCounts {
                input_tokens: 200_001,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(haiku_over_limit.is_none());
    }

    #[test]
    fn seeded_pricing_uses_the_rate_effective_when_usage_occurred() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("historical-pricing.db")).unwrap();
        let connection = Connection::open(database.path()).unwrap();

        let gpt_55 = find_rule(
            &connection,
            "codex",
            "gpt-5.5-2026-04-23",
            "2026-05-01T00:00:00Z",
            &TokenCounts {
                input_tokens: 100_000,
                ..Default::default()
            },
        )
        .unwrap()
        .expect("a GPT-5.5 launch-era rule");
        assert_eq!(gpt_55.input_usd_micros_per_million, 5_000_000);
        assert_eq!(gpt_55.output_usd_micros_per_million, 30_000_000);

        let pre_promo = find_rule(
            &connection,
            "codex",
            "gpt-5.6-sol",
            "2026-08-20T00:00:00Z",
            &TokenCounts {
                input_tokens: 100_000,
                ..Default::default()
            },
        )
        .unwrap()
        .expect("a pre-promotion GPT-5.6 Sol rule");
        let promotional = find_rule(
            &connection,
            "codex",
            "gpt-5.6-sol",
            "2026-08-28T00:00:00Z",
            &TokenCounts {
                input_tokens: 100_000,
                ..Default::default()
            },
        )
        .unwrap()
        .expect("a promotional GPT-5.6 Sol rule");
        assert_eq!(pre_promo.input_usd_micros_per_million, 5_000_000);
        assert_eq!(promotional.input_usd_micros_per_million, 4_000_000);
    }
}
