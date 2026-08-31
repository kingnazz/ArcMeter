use crate::db::{Database, DatabaseError, Result as DatabaseResult};
use crate::domain::TokenCounts;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
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
    pub output_usd_micros_per_million: i64,
    pub reasoning_pricing_behavior: ReasoningPricingBehavior,
    pub reasoning_usd_micros_per_million: Option<i64>,
    pub version: i64,
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
    Unavailable,
}

pub fn calculate_api_value(tokens: &TokenCounts, rule: &PricingRule) -> PricingResult {
    if tokens.input_tokens < rule.min_input_tokens
        || rule
            .max_input_tokens
            .is_some_and(|maximum| tokens.input_tokens > maximum)
    {
        return PricingResult::Unavailable;
    }
    if rule.reasoning_pricing_behavior == ReasoningPricingBehavior::Unavailable
        && tokens.reasoning_tokens > 0
    {
        return PricingResult::Unavailable;
    }
    let uncached_input = tokens
        .input_tokens
        .saturating_sub(tokens.cached_input_tokens)
        .max(0);
    let cached_rate = match (
        tokens.cached_input_tokens,
        rule.cached_input_usd_micros_per_million,
    ) {
        (0, _) => 0,
        (_, Some(rate)) => rate,
        (_, None) => return PricingResult::Unavailable,
    };
    let mut total = priced(uncached_input, rule.input_usd_micros_per_million)
        .saturating_add(priced(tokens.cached_input_tokens, cached_rate))
        .saturating_add(priced(
            tokens.output_tokens,
            rule.output_usd_micros_per_million,
        ));
    if rule.reasoning_pricing_behavior == ReasoningPricingBehavior::Separate {
        let Some(rate) = rule.reasoning_usd_micros_per_million else {
            return PricingResult::Unavailable;
        };
        total = total.saturating_add(priced(tokens.reasoning_tokens, rate));
    }
    PricingResult::Available(total)
}

/// Revalues measured events from versioned, local pricing metadata. Unknown models and
/// unsupported context tiers intentionally remain unavailable.
pub fn reprice_events(database: &Database) -> DatabaseResult<usize> {
    let mut connection = Connection::open(database.path())?;
    let events = {
        let mut statement = connection.prepare(
            "SELECT id, provider, model, occurred_at, input_tokens, cached_input_tokens,
                    cache_write_tokens, output_tokens, reasoning_tokens, total_tokens,
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
                    output_tokens: row.get(7)?,
                    reasoning_tokens: row.get(8)?,
                    total_tokens: row.get(9)?,
                },
                previous_value: row.get(10)?,
                previous_status: row.get(11)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };

    let transaction = connection.transaction()?;
    let mut changed = 0;
    for event in events {
        let rule = find_rule(
            &transaction,
            &event.provider,
            &event.model,
            &event.occurred_at,
            event.tokens.input_tokens,
        )?;
        let (value, status) = match rule
            .as_ref()
            .map(|rule| calculate_api_value(&event.tokens, rule))
        {
            Some(PricingResult::Available(value)) => (Some(value), "available"),
            _ => (None, "unavailable"),
        };
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
    input_tokens: i64,
) -> DatabaseResult<Option<PricingRule>> {
    let mut statement = connection.prepare(
        "SELECT provider, model_pattern, effective_from, min_input_tokens, max_input_tokens,
                input_usd_micros_per_million, cached_input_usd_micros_per_million,
                output_usd_micros_per_million, reasoning_pricing_behavior,
                reasoning_usd_micros_per_million, version
         FROM pricing
         WHERE provider = ?1 AND effective_from <= ?3
           AND min_input_tokens <= ?4
           AND (max_input_tokens IS NULL OR max_input_tokens >= ?4)
         ORDER BY (model_pattern = ?2) DESC, length(model_pattern) DESC,
                  effective_from DESC, min_input_tokens DESC, version DESC",
    )?;
    let mut rows = statement.query(params![provider, model, occurred_at, input_tokens.max(0)])?;
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
        let reasoning_behavior: String = row.get(8)?;
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
            min_input_tokens: row.get(3)?,
            max_input_tokens: row.get(4)?,
            input_usd_micros_per_million: row.get(5)?,
            cached_input_usd_micros_per_million: row.get(6)?,
            output_usd_micros_per_million: row.get(7)?,
            reasoning_pricing_behavior,
            reasoning_usd_micros_per_million: row.get(9)?,
            version: row.get(10)?,
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
    fn unavailable_cache_or_reasoning_rates_do_not_guess() {
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
            PricingResult::Unavailable
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
            PricingResult::Unavailable
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
            1_000_000,
        )
        .unwrap()
        .expect("a long-context GPT-5.6 Sol rule");
        assert_eq!(rule.min_input_tokens, 272_001);
        assert_eq!(rule.input_usd_micros_per_million, 8_000_000);
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
            100_000,
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
            100_000,
        )
        .unwrap()
        .expect("a pre-promotion GPT-5.6 Sol rule");
        let promotional = find_rule(
            &connection,
            "codex",
            "gpt-5.6-sol",
            "2026-08-28T00:00:00Z",
            100_000,
        )
        .unwrap()
        .expect("a promotional GPT-5.6 Sol rule");
        assert_eq!(pre_promo.input_usd_micros_per_million, 5_000_000);
        assert_eq!(promotional.input_usd_micros_per_million, 4_000_000);
    }
}
