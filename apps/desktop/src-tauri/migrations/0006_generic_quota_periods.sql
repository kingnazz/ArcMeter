-- Rebuild only the normalized quota table so generic monthly/product kinds can
-- coexist with all existing Claude rows. No raw provider data is introduced.
ALTER TABLE provider_quota_snapshots RENAME TO provider_quota_snapshots_v5;

CREATE TABLE provider_quota_snapshots (
  id TEXT PRIMARY KEY CHECK(length(id) = 64),
  snapshot_id TEXT NOT NULL CHECK(length(snapshot_id) = 64),
  provider TEXT NOT NULL,
  window_key TEXT NOT NULL,
  label TEXT NOT NULL CHECK(length(label) BETWEEN 1 AND 80),
  kind TEXT NOT NULL CHECK(kind IN ('rolling', 'weekly', 'monthly', 'model_weekly', 'product', 'other')),
  scope TEXT CHECK(scope IS NULL OR length(scope) <= 80),
  utilization_bps INTEGER NOT NULL CHECK(utilization_bps BETWEEN 0 AND 10000),
  period_starts_at TEXT,
  resets_at TEXT,
  observed_at TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('provider_api', 'cloud_sync')),
  source_device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  extra_usage_enabled INTEGER CHECK(extra_usage_enabled IS NULL OR extra_usage_enabled IN (0, 1)),
  extra_monthly_limit_minor INTEGER CHECK(extra_monthly_limit_minor IS NULL OR extra_monthly_limit_minor >= 0),
  extra_used_credits_minor INTEGER CHECK(extra_used_credits_minor IS NULL OR extra_used_credits_minor >= 0),
  extra_utilization_bps INTEGER CHECK(extra_utilization_bps IS NULL OR extra_utilization_bps BETWEEN 0 AND 10000),
  extra_currency TEXT CHECK(extra_currency IS NULL OR length(extra_currency) BETWEEN 3 AND 8),
  extra_prepaid_balance_minor INTEGER CHECK(extra_prepaid_balance_minor IS NULL OR extra_prepaid_balance_minor >= 0),
  plan_label TEXT CHECK(plan_label IS NULL OR length(plan_label) BETWEEN 1 AND 80),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  sync_status TEXT NOT NULL DEFAULT 'pending'
    CHECK(sync_status IN ('pending', 'synced', 'error'))
);

INSERT INTO provider_quota_snapshots(
  id, snapshot_id, provider, window_key, label, kind, scope, utilization_bps,
  resets_at, observed_at, source, source_device_id, extra_usage_enabled,
  extra_monthly_limit_minor, extra_used_credits_minor, extra_utilization_bps,
  extra_currency, created_at, updated_at, sync_status
)
SELECT id, snapshot_id, provider, window_key, label, kind, scope, utilization_bps,
  resets_at, observed_at, source, source_device_id, extra_usage_enabled,
  extra_monthly_limit_minor, extra_used_credits_minor, extra_utilization_bps,
  extra_currency, created_at, updated_at, sync_status
FROM provider_quota_snapshots_v5;

DROP TABLE provider_quota_snapshots_v5;

CREATE INDEX idx_quota_latest
  ON provider_quota_snapshots(provider, observed_at DESC, snapshot_id);
CREATE INDEX idx_quota_history
  ON provider_quota_snapshots(provider, window_key, observed_at DESC);
CREATE INDEX idx_quota_sync
  ON provider_quota_snapshots(sync_status, created_at);

INSERT OR IGNORE INTO app_settings(key, value, updated_at)
VALUES('grok_live_quota_enabled', 'false', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
