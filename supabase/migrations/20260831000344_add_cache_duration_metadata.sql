-- Additive normalized telemetry metadata only. No cloud migration is applied by this change.
alter table if exists public.usage_events
  add column if not exists cache_write_5m_tokens bigint not null default 0
    check (cache_write_5m_tokens >= 0),
  add column if not exists cache_write_1h_tokens bigint not null default 0
    check (cache_write_1h_tokens >= 0);

alter table if exists public.pricing
  add column if not exists cache_write_5m_usd_micros_per_million bigint
    check (cache_write_5m_usd_micros_per_million is null or cache_write_5m_usd_micros_per_million >= 0),
  add column if not exists cache_write_1h_usd_micros_per_million bigint
    check (cache_write_1h_usd_micros_per_million is null or cache_write_1h_usd_micros_per_million >= 0),
  add column if not exists input_token_semantics text not null default 'cache_included'
    check (input_token_semantics in ('cache_included', 'cache_additive'));

update public.pricing
set input_token_semantics = 'cache_additive',
    cache_write_5m_usd_micros_per_million = input_usd_micros_per_million * 5 / 4,
    cache_write_1h_usd_micros_per_million = input_usd_micros_per_million * 2
where provider = 'claude';

update public.pricing
set max_input_tokens = 200000
where provider = 'claude'
  and (model_pattern like 'claude-sonnet-4-5%'
    or model_pattern like 'claude-opus-4-5%'
    or model_pattern like 'claude-haiku-4-5%');
