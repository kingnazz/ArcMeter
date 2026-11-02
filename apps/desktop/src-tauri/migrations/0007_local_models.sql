-- Runtime inventory and observations stay on this computer. They are not usage minutes.
CREATE TABLE local_runtime_state (
  provider TEXT PRIMARY KEY CHECK(provider IN ('omlx', 'ollama', 'lmstudio')),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
  endpoint_override TEXT,
  status TEXT NOT NULL DEFAULT 'not_detected',
  checked_at TEXT,
  detected_at TEXT,
  models_json TEXT NOT NULL DEFAULT '[]',
  diagnostic TEXT NOT NULL DEFAULT 'Not checked yet'
);
INSERT INTO local_runtime_state(provider) VALUES ('omlx'), ('ollama'), ('lmstudio');
CREATE TABLE local_runtime_observations (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  model TEXT,
  state TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  device_id TEXT NOT NULL REFERENCES devices(id)
);
CREATE INDEX local_runtime_observations_time ON local_runtime_observations(observed_at DESC);
-- Extra source metadata is local-only; token aggregates use the existing sync ledger.
CREATE TABLE local_usage_details (
  event_id TEXT PRIMARY KEY REFERENCES usage_events(id),
  requests INTEGER NOT NULL CHECK(requests >= 0)
);
