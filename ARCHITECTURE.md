# ArcMeter architecture

## System shape

ArcMeter is one Tauri codebase for Windows and macOS. The React renderer owns presentation and user interaction. It has no filesystem capability. Rust owns filesystem discovery, parsing, SQLite, credential access, aggregation, and sync.

```text
Codex / Claude Code / Grok Build / Gemini CLI
                    │ read-only native discovery
                    ▼
        resilient provider parsers (Rust)
                    │ normalized, sanitized UsageEvent
                    ▼
           local SQLite transaction
                    │
           ┌────────┴────────┐
           ▼                 ▼
  local aggregate API   pending sync batches
           │                 │ authenticated + RLS
           ▼                 ▼
     React dashboard    Supabase Postgres
                             │ pull normalized metadata
                             ▼
                      other ArcMeter devices
```

## Modules

- `domain.rs` defines the canonical usage event, token normalization, measurement labels, sanitization, and deterministic identities.
- `collectors/` contains provider discovery and resilient parsers. A malformed record never aborts another record or file.
- `db.rs` owns SQLite connections, migrations, transaction boundaries, device identity, ingestion, collector state, subscriptions, and settings.
- `analytics.rs` executes local-first aggregate queries for all four views.
- `pricing.rs` computes API-equivalent value only when an exact, effective pricing rule exists.
- `auth.rs` calls Supabase Auth and stores the serialized session in the OS credential store.
- `sync.rs` pulls remote metadata, resolves local records, uploads deterministic batches, and advances sync state only after success.
- `commands.rs` is the renderer's narrow native API.

## Startup and background behavior

The app opens SQLite and resolves the persistent local device ID before creating collector work. Collection runs immediately and then every 60 seconds on Tauri's async runtime, with blocking parsing isolated from the UI thread. The renderer reads SQLite immediately, receives narrow data-change events, and can request a new scan. A signed-in app runs background sync at a five-minute interval; failures use capped exponential backoff.

Close-to-tray is a persisted local preference. Autostart uses the official Tauri plugin with Launch Agent on macOS and the native Windows mechanism.

## Incremental collection

`collector_state` stores an opaque source key, safe filename fingerprint, size, modification time, processed offset, parser version, scan time, last usage time, and safe diagnostic. Unchanged files are skipped. A changed file is replayed to reconstruct session/model/turn context; only that file is replayed, not the complete provider history. Inserts use `ON CONFLICT(id) DO NOTHING`, so replay is idempotent.

Future parser versions can force a replay by incrementing `PARSER_VERSION`. Log truncation and rotation naturally appear as changed/new sources.

## Sync consistency

Usage events are immutable and use deterministic SHA-256 IDs. Device IDs are generated UUIDs persisted in `app_settings.local_device_id`; hostnames are friendly labels only. The sync engine:

1. refreshes the OS-protected Supabase session when needed;
2. pulls remote devices, events, and subscriptions;
3. applies immutable event inserts and timestamp-guarded subscription changes;
4. upserts the local device;
5. uploads events in batches of 250;
6. uploads pending subscriptions;
7. marks local rows synced and advances `last_remote_sync` only after successful responses.

Remote pulls are ordered and paginated in 1,000-row pages. Each sync captures a high-water timestamp before requesting rows, filters events to `(previous_cursor, high_water]`, and advances to that high-water value only after the entire operation succeeds. This avoids skipping a row committed while uploads are still running.

Supabase derives ownership from the JWT and RLS. The client omits `user_id` on writes and cannot override another user's ownership.

## Future browser sources

The canonical model already supports `source_type = browser` and `measurement_kind = estimated | activity_only`. No browser collector exists in V1. A future source cannot use `measured` unless it provides authoritative telemetry. Aggregate queries can maintain visual/semantic separation by measurement kind.
