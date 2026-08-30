-- Historical public API list prices for models observed in ArcMeter telemetry.
-- Rates are USD micros per million tokens. Later effective_from rows in 0001
-- remain authoritative after the 2026-08-27 GPT-5.6 promotional price change.
INSERT OR IGNORE INTO pricing(
  id, provider, model_pattern, effective_from, min_input_tokens, max_input_tokens,
  input_usd_micros_per_million, cached_input_usd_micros_per_million,
  output_usd_micros_per_million, reasoning_pricing_behavior, version
) VALUES
  ('openai-gpt-5.3-codex-v1', 'codex', 'gpt-5.3-codex*', '2026-02-05T00:00:00Z', 0, NULL, 1750000, 175000, 14000000, 'included_in_output', 1),
  ('openai-gpt-5.4-short-v1', 'codex', 'gpt-5.4*', '2026-03-05T00:00:00Z', 0, 272000, 2500000, 250000, 15000000, 'included_in_output', 1),
  ('openai-gpt-5.4-long-v1', 'codex', 'gpt-5.4*', '2026-03-05T00:00:00Z', 272001, NULL, 5000000, 500000, 22500000, 'included_in_output', 1),
  ('openai-gpt-5.5-short-v1', 'codex', 'gpt-5.5*', '2026-04-23T00:00:00Z', 0, 272000, 5000000, 500000, 30000000, 'included_in_output', 1),
  ('openai-gpt-5.5-long-v1', 'codex', 'gpt-5.5*', '2026-04-23T00:00:00Z', 272001, NULL, 10000000, 1000000, 45000000, 'included_in_output', 1),
  ('openai-gpt-5.6-sol-pre-promo-short-v1', 'codex', 'gpt-5.6-sol*', '2026-07-10T00:00:00Z', 0, 272000, 5000000, 500000, 30000000, 'included_in_output', 1),
  ('openai-gpt-5.6-sol-pre-promo-long-v1', 'codex', 'gpt-5.6-sol*', '2026-07-10T00:00:00Z', 272001, NULL, 10000000, 1000000, 45000000, 'included_in_output', 1),
  ('openai-gpt-5.6-terra-pre-promo-short-v1', 'codex', 'gpt-5.6-terra*', '2026-07-10T00:00:00Z', 0, 272000, 2500000, 250000, 15000000, 'included_in_output', 1),
  ('openai-gpt-5.6-terra-pre-promo-long-v1', 'codex', 'gpt-5.6-terra*', '2026-07-10T00:00:00Z', 272001, NULL, 5000000, 500000, 22500000, 'included_in_output', 1),
  ('openai-gpt-5.6-luna-pre-promo-short-v1', 'codex', 'gpt-5.6-luna*', '2026-08-10T00:00:00Z', 0, 272000, 1000000, 100000, 6000000, 'included_in_output', 1),
  ('openai-gpt-5.6-luna-pre-promo-long-v1', 'codex', 'gpt-5.6-luna*', '2026-08-10T00:00:00Z', 272001, NULL, 2000000, 200000, 9000000, 'included_in_output', 1),
  ('anthropic-claude-sonnet-4.6-launch-v1', 'claude', 'claude-sonnet-4-6*', '2026-02-17T00:00:00Z', 0, NULL, 3000000, 300000, 15000000, 'included_in_output', 1),
  ('anthropic-claude-opus-4.7-launch-v1', 'claude', 'claude-opus-4-7*', '2026-04-16T00:00:00Z', 0, NULL, 5000000, 500000, 25000000, 'included_in_output', 1);
