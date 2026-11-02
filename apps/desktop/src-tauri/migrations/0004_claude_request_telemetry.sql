-- Generic cache-duration metadata and pricing semantics. Existing rows remain intact.
ALTER TABLE usage_events
  ADD COLUMN cache_write_5m_tokens INTEGER NOT NULL DEFAULT 0
    CHECK(cache_write_5m_tokens >= 0);

ALTER TABLE usage_events
  ADD COLUMN cache_write_1h_tokens INTEGER NOT NULL DEFAULT 0
    CHECK(cache_write_1h_tokens >= 0);

ALTER TABLE pricing
  ADD COLUMN cache_write_5m_usd_micros_per_million INTEGER
    CHECK(cache_write_5m_usd_micros_per_million IS NULL OR cache_write_5m_usd_micros_per_million >= 0);

ALTER TABLE pricing
  ADD COLUMN cache_write_1h_usd_micros_per_million INTEGER
    CHECK(cache_write_1h_usd_micros_per_million IS NULL OR cache_write_1h_usd_micros_per_million >= 0);

ALTER TABLE pricing
  ADD COLUMN input_token_semantics TEXT NOT NULL DEFAULT 'cache_included'
    CHECK(input_token_semantics IN ('cache_included', 'cache_additive'));

-- Anthropic API usage reports fresh input, cache reads, and cache writes as
-- additive counters. Public cache-write rates are 1.25x and 2x base input.
UPDATE pricing
SET input_token_semantics = 'cache_additive',
    cache_write_5m_usd_micros_per_million = input_usd_micros_per_million * 5 / 4,
    cache_write_1h_usd_micros_per_million = input_usd_micros_per_million * 2
WHERE provider = 'claude';

-- Current first-party context limits: Claude 4.6+ families in this catalog use
-- standard pricing through 1M input tokens; these earlier 4.5 families stop at 200k.
UPDATE pricing
SET max_input_tokens = 200000
WHERE provider = 'claude'
  AND (model_pattern LIKE 'claude-sonnet-4-5%'
    OR model_pattern LIKE 'claude-opus-4-5%'
    OR model_pattern LIKE 'claude-haiku-4-5%');
