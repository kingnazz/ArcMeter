-- Additive metadata only: existing usage rows and ownership policies are preserved.
alter table if exists public.usage_events
  add column if not exists cache_write_tokens bigint not null default 0
    check (cache_write_tokens >= 0),
  add column if not exists native_cost_usd_ticks bigint
    check (native_cost_usd_ticks is null or native_cost_usd_ticks >= 0),
  add column if not exists superseded_by_event_id text;

create index if not exists usage_events_user_superseded_idx
  on public.usage_events(user_id, superseded_by_event_id);
