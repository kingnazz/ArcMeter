CREATE TABLE IF NOT EXISTS provider_quota_snapshots (
  id TEXT PRIMARY KEY CHECK(length(id) = 64),
  snapshot_id TEXT NOT NULL CHECK(length(snapshot_id) = 64),
  provider TEXT NOT NULL,
  window_key TEXT NOT NULL,
  label TEXT NOT NULL CHECK(length(label) BETWEEN 1 AND 80),
  kind TEXT NOT NULL CHECK(kind IN ('rolling', 'weekly', 'model_weekly', 'other')),
  scope TEXT CHECK(scope IS NULL OR length(scope) <= 80),
  utilization_bps INTEGER NOT NULL CHECK(utilization_bps BETWEEN 0 AND 10000),
  resets_at TEXT,
  observed_at TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('provider_api', 'cloud_sync')),
  source_device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  extra_usage_enabled INTEGER CHECK(extra_usage_enabled IS NULL OR extra_usage_enabled IN (0, 1)),
  extra_monthly_limit_minor INTEGER CHECK(extra_monthly_limit_minor IS NULL OR extra_monthly_limit_minor >= 0),
  extra_used_credits_minor INTEGER CHECK(extra_used_credits_minor IS NULL OR extra_used_credits_minor >= 0),
  extra_utilization_bps INTEGER CHECK(extra_utilization_bps IS NULL OR extra_utilization_bps BETWEEN 0 AND 10000),
  extra_currency TEXT CHECK(extra_currency IS NULL OR length(extra_currency) BETWEEN 3 AND 8),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  sync_status TEXT NOT NULL DEFAULT 'pending'
    CHECK(sync_status IN ('pending', 'synced', 'error'))
);

CREATE INDEX IF NOT EXISTS idx_quota_latest
  ON provider_quota_snapshots(provider, observed_at DESC, snapshot_id);
CREATE INDEX IF NOT EXISTS idx_quota_history
  ON provider_quota_snapshots(provider, window_key, observed_at DESC);
CREATE INDEX IF NOT EXISTS idx_quota_sync
  ON provider_quota_snapshots(sync_status, created_at);

CREATE TABLE IF NOT EXISTS provider_quota_refresh_state (
  provider TEXT PRIMARY KEY,
  status TEXT NOT NULL CHECK(status IN (
    'credential_unavailable', 'permission_denied', 'expired_login', 'forbidden',
    'rate_limited', 'provider_unavailable', 'offline', 'invalid_response', 'healthy'
  )),
  message TEXT NOT NULL CHECK(length(message) <= 240),
  attempted_at TEXT NOT NULL,
  retry_at TEXT,
  consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK(consecutive_failures >= 0)
);

INSERT OR IGNORE INTO app_settings(key, value, updated_at)
VALUES('claude_live_quota_enabled', 'false', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
