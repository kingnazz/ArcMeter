PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
  id TEXT PRIMARY KEY,
  friendly_name TEXT NOT NULL CHECK(length(friendly_name) BETWEEN 1 AND 80),
  os TEXT NOT NULL,
  architecture TEXT NOT NULL,
  app_version TEXT NOT NULL,
  created_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  last_sync_at TEXT,
  sync_status TEXT NOT NULL DEFAULT 'local_only'
    CHECK(sync_status IN ('local_only', 'synced', 'syncing', 'error'))
);

CREATE TABLE IF NOT EXISTS usage_events (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  source TEXT NOT NULL,
  source_type TEXT NOT NULL CHECK(source_type IN ('local_cli', 'browser', 'api', 'manual')),
  native_session_id TEXT NOT NULL,
  native_event_id TEXT NOT NULL,
  occurred_at TEXT NOT NULL,
  model TEXT,
  project_name TEXT,
  input_tokens INTEGER NOT NULL DEFAULT 0 CHECK(input_tokens >= 0),
  cached_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK(cached_input_tokens >= 0),
  output_tokens INTEGER NOT NULL DEFAULT 0 CHECK(output_tokens >= 0),
  reasoning_tokens INTEGER NOT NULL DEFAULT 0 CHECK(reasoning_tokens >= 0),
  total_tokens INTEGER NOT NULL DEFAULT 0 CHECK(total_tokens >= 0),
  estimated_api_value_usd_micros INTEGER,
  pricing_status TEXT NOT NULL DEFAULT 'unavailable'
    CHECK(pricing_status IN ('available', 'unavailable', 'partial')),
  measurement_kind TEXT NOT NULL
    CHECK(measurement_kind IN ('measured', 'estimated', 'activity_only')),
  device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE RESTRICT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  sync_status TEXT NOT NULL DEFAULT 'pending'
    CHECK(sync_status IN ('pending', 'synced', 'error')),
  sync_attempts INTEGER NOT NULL DEFAULT 0,
  last_sync_error TEXT,
  UNIQUE(provider, native_session_id, native_event_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_usage_events_occurred_at
  ON usage_events(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_events_provider_occurred
  ON usage_events(provider, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_events_device_occurred
  ON usage_events(device_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_events_project_occurred
  ON usage_events(project_name, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_events_model_occurred
  ON usage_events(model, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_events_sync
  ON usage_events(sync_status, created_at);
CREATE INDEX IF NOT EXISTS idx_usage_events_measurement
  ON usage_events(measurement_kind, occurred_at DESC);

CREATE TABLE IF NOT EXISTS subscriptions (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  plan_name TEXT NOT NULL,
  monthly_price_usd_cents INTEGER NOT NULL CHECK(monthly_price_usd_cents >= 0),
  billing_cadence TEXT NOT NULL DEFAULT 'monthly'
    CHECK(billing_cadence IN ('monthly', 'annual')),
  active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  sync_status TEXT NOT NULL DEFAULT 'pending'
    CHECK(sync_status IN ('pending', 'synced', 'error'))
);

CREATE TABLE IF NOT EXISTS collector_state (
  source_key TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  safe_file_fingerprint TEXT NOT NULL,
  file_size INTEGER NOT NULL DEFAULT 0,
  modified_at_ms INTEGER NOT NULL DEFAULT 0,
  last_processed_offset INTEGER NOT NULL DEFAULT 0,
  parser_version INTEGER NOT NULL,
  state_json TEXT NOT NULL DEFAULT '{}',
  last_scan_at TEXT NOT NULL,
  last_usage_at TEXT,
  status TEXT NOT NULL DEFAULT 'healthy'
    CHECK(status IN ('healthy', 'warning', 'error')),
  diagnostic TEXT
);

CREATE TABLE IF NOT EXISTS sync_state (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pricing (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  model_pattern TEXT NOT NULL,
  effective_from TEXT NOT NULL,
  min_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK(min_input_tokens >= 0),
  max_input_tokens INTEGER CHECK(max_input_tokens IS NULL OR max_input_tokens >= min_input_tokens),
  input_usd_micros_per_million INTEGER NOT NULL,
  cached_input_usd_micros_per_million INTEGER,
  output_usd_micros_per_million INTEGER NOT NULL,
  reasoning_pricing_behavior TEXT NOT NULL
    CHECK(reasoning_pricing_behavior IN ('included_in_output', 'separate', 'unavailable')),
  reasoning_usd_micros_per_million INTEGER,
  version INTEGER NOT NULL,
  UNIQUE(provider, model_pattern, effective_from, min_input_tokens, version)
);

CREATE INDEX IF NOT EXISTS idx_pricing_lookup
  ON pricing(provider, model_pattern, effective_from DESC, min_input_tokens);

-- Standard text/API-equivalent rates verified from official provider pricing on 2026-08-27.
-- The effective date deliberately prevents retroactive valuation before verification.
-- USD prices per million tokens are stored as integer USD micros.
INSERT OR IGNORE INTO pricing(
  id, provider, model_pattern, effective_from, min_input_tokens, max_input_tokens,
  input_usd_micros_per_million, cached_input_usd_micros_per_million,
  output_usd_micros_per_million, reasoning_pricing_behavior, version
) VALUES
  ('openai-gpt-5.6-sol-short-v1', 'codex', 'gpt-5.6-sol*', '2026-08-27T00:00:00Z', 0, 272000, 4000000, 400000, 20000000, 'included_in_output', 1),
  ('openai-gpt-5.6-sol-long-v1', 'codex', 'gpt-5.6-sol*', '2026-08-27T00:00:00Z', 272001, NULL, 8000000, 800000, 30000000, 'included_in_output', 1),
  ('openai-gpt-5.6-alias-short-v1', 'codex', 'gpt-5.6', '2026-08-27T00:00:00Z', 0, 272000, 4000000, 400000, 20000000, 'included_in_output', 1),
  ('openai-gpt-5.6-alias-long-v1', 'codex', 'gpt-5.6', '2026-08-27T00:00:00Z', 272001, NULL, 8000000, 800000, 30000000, 'included_in_output', 1),
  ('openai-gpt-5.6-terra-short-v1', 'codex', 'gpt-5.6-terra*', '2026-08-27T00:00:00Z', 0, 272000, 2000000, 200000, 12000000, 'included_in_output', 1),
  ('openai-gpt-5.6-terra-long-v1', 'codex', 'gpt-5.6-terra*', '2026-08-27T00:00:00Z', 272001, NULL, 4000000, 400000, 18000000, 'included_in_output', 1),
  ('openai-gpt-5.6-luna-short-v1', 'codex', 'gpt-5.6-luna*', '2026-08-27T00:00:00Z', 0, 272000, 200000, 20000, 1200000, 'included_in_output', 1),
  ('openai-gpt-5.6-luna-long-v1', 'codex', 'gpt-5.6-luna*', '2026-08-27T00:00:00Z', 272001, NULL, 400000, 40000, 1800000, 'included_in_output', 1),
  ('claude-sonnet-5-v1', 'claude', 'claude-sonnet-5*', '2026-08-27T00:00:00Z', 0, NULL, 2000000, 200000, 10000000, 'included_in_output', 1),
  ('claude-sonnet-4.6-v1', 'claude', 'claude-sonnet-4-6*', '2026-08-27T00:00:00Z', 0, NULL, 3000000, 300000, 15000000, 'included_in_output', 1),
  ('claude-sonnet-4.5-v1', 'claude', 'claude-sonnet-4-5*', '2026-08-27T00:00:00Z', 0, NULL, 3000000, 300000, 15000000, 'included_in_output', 1),
  ('claude-opus-5-v1', 'claude', 'claude-opus-5*', '2026-08-27T00:00:00Z', 0, NULL, 5000000, 500000, 25000000, 'included_in_output', 1),
  ('claude-opus-4.8-v1', 'claude', 'claude-opus-4-8*', '2026-08-27T00:00:00Z', 0, NULL, 5000000, 500000, 25000000, 'included_in_output', 1),
  ('claude-opus-4.7-v1', 'claude', 'claude-opus-4-7*', '2026-08-27T00:00:00Z', 0, NULL, 5000000, 500000, 25000000, 'included_in_output', 1),
  ('claude-opus-4.6-v1', 'claude', 'claude-opus-4-6*', '2026-08-27T00:00:00Z', 0, NULL, 5000000, 500000, 25000000, 'included_in_output', 1),
  ('claude-opus-4.5-v1', 'claude', 'claude-opus-4-5*', '2026-08-27T00:00:00Z', 0, NULL, 5000000, 500000, 25000000, 'included_in_output', 1),
  ('claude-haiku-4.5-v1', 'claude', 'claude-haiku-4-5*', '2026-08-27T00:00:00Z', 0, NULL, 1000000, 100000, 5000000, 'included_in_output', 1),
  ('gemini-3.7-flash-v1', 'gemini', 'gemini-3.7-flash*', '2026-08-27T00:00:00Z', 0, NULL, 750000, 75000, 3750000, 'included_in_output', 1),
  ('gemini-3.5-flash-lite-v1', 'gemini', 'gemini-3.5-flash-lite*', '2026-08-27T00:00:00Z', 0, NULL, 300000, 30000, 2500000, 'included_in_output', 1),
  ('gemini-3.1-flash-lite-v1', 'gemini', 'gemini-3.1-flash-lite*', '2026-08-27T00:00:00Z', 0, NULL, 250000, 25000, 1500000, 'included_in_output', 1),
  ('gemini-3.1-pro-short-v1', 'gemini', 'gemini-3.1-pro-preview*', '2026-08-27T00:00:00Z', 0, 200000, 2000000, 200000, 12000000, 'included_in_output', 1),
  ('gemini-3.1-pro-long-v1', 'gemini', 'gemini-3.1-pro-preview*', '2026-08-27T00:00:00Z', 200001, NULL, 4000000, 400000, 18000000, 'included_in_output', 1),
  ('gemini-2.5-pro-short-v1', 'gemini', 'gemini-2.5-pro*', '2026-08-27T00:00:00Z', 0, 200000, 1250000, 125000, 10000000, 'included_in_output', 1),
  ('gemini-2.5-pro-long-v1', 'gemini', 'gemini-2.5-pro*', '2026-08-27T00:00:00Z', 200001, NULL, 2500000, 250000, 15000000, 'included_in_output', 1),
  ('gemini-2.5-flash-v1', 'gemini', 'gemini-2.5-flash*', '2026-08-27T00:00:00Z', 0, NULL, 300000, 30000, 2500000, 'included_in_output', 1),
  ('gemini-2.5-flash-lite-v1', 'gemini', 'gemini-2.5-flash-lite*', '2026-08-27T00:00:00Z', 0, NULL, 100000, 10000, 400000, 'included_in_output', 1);

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

INSERT OR IGNORE INTO app_settings(key, value, updated_at) VALUES
  ('close_to_tray', 'true', strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  ('theme', 'dark', strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  ('sync_enabled', 'true', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
