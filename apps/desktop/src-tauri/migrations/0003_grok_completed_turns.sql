ALTER TABLE usage_events
  ADD COLUMN cache_write_tokens INTEGER NOT NULL DEFAULT 0 CHECK(cache_write_tokens >= 0);

ALTER TABLE usage_events
  ADD COLUMN native_cost_usd_ticks INTEGER CHECK(native_cost_usd_ticks IS NULL OR native_cost_usd_ticks >= 0);

ALTER TABLE usage_events
  ADD COLUMN superseded_by_event_id TEXT;

CREATE INDEX IF NOT EXISTS idx_usage_events_superseded
  ON usage_events(superseded_by_event_id);
