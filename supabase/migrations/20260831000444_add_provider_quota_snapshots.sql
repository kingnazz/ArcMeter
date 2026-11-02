-- Normalized provider quota readings only. Credentials and raw provider responses are prohibited.
create table public.provider_quota_snapshots (
  id text primary key check (char_length(id) = 64),
  snapshot_id text not null check (char_length(snapshot_id) = 64),
  user_id uuid not null default auth.uid() references public.profiles(id) on delete cascade,
  provider text not null,
  window_key text not null,
  label text not null check (char_length(label) between 1 and 80),
  kind text not null check (kind in ('rolling', 'weekly', 'model_weekly', 'other')),
  scope text check (scope is null or char_length(scope) <= 80),
  utilization_bps integer not null check (utilization_bps between 0 and 10000),
  resets_at timestamptz,
  observed_at timestamptz not null,
  source text not null check (source in ('provider_api', 'cloud_sync')),
  source_device_id uuid not null,
  extra_usage_enabled boolean,
  extra_monthly_limit_minor bigint check (extra_monthly_limit_minor is null or extra_monthly_limit_minor >= 0),
  extra_used_credits_minor bigint check (extra_used_credits_minor is null or extra_used_credits_minor >= 0),
  extra_utilization_bps integer check (extra_utilization_bps is null or extra_utilization_bps between 0 and 10000),
  extra_currency text check (extra_currency is null or char_length(extra_currency) between 3 and 8),
  created_at timestamptz not null,
  updated_at timestamptz not null,
  foreign key (source_device_id, user_id) references public.devices(id, user_id) on delete cascade
);

create index provider_quota_user_latest_idx
  on public.provider_quota_snapshots(user_id, provider, observed_at desc);
create index provider_quota_user_window_history_idx
  on public.provider_quota_snapshots(user_id, provider, window_key, observed_at desc);
create index provider_quota_user_updated_idx
  on public.provider_quota_snapshots(user_id, updated_at);

alter table public.provider_quota_snapshots enable row level security;

create policy provider_quota_select_own on public.provider_quota_snapshots for select to authenticated
using ((select auth.uid()) is not null and (select auth.uid()) = user_id);
create policy provider_quota_insert_own on public.provider_quota_snapshots for insert to authenticated
with check ((select auth.uid()) is not null and (select auth.uid()) = user_id);
create policy provider_quota_update_own on public.provider_quota_snapshots for update to authenticated
using ((select auth.uid()) is not null and (select auth.uid()) = user_id)
with check ((select auth.uid()) is not null and (select auth.uid()) = user_id);
create policy provider_quota_delete_own on public.provider_quota_snapshots for delete to authenticated
using ((select auth.uid()) is not null and (select auth.uid()) = user_id);

revoke all on public.provider_quota_snapshots from anon, authenticated;
grant select, insert, update, delete on public.provider_quota_snapshots to authenticated;

comment on table public.provider_quota_snapshots is
  'Normalized account quota percentages and reset metadata only. OAuth tokens, cookies, account identity, and raw provider responses are prohibited.';
