-- ArcMeter V1 cloud metadata schema.
-- The desktop uses only the client-safe Supabase key. Ownership is always derived from auth.uid().

create extension if not exists pgcrypto;

create table public.profiles (
  id uuid primary key references auth.users(id) on delete cascade,
  email text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table public.devices (
  id uuid primary key,
  user_id uuid not null default auth.uid() references public.profiles(id) on delete cascade,
  friendly_name text not null check (char_length(friendly_name) between 1 and 80),
  os text not null,
  architecture text not null,
  app_version text not null,
  created_at timestamptz not null,
  last_seen_at timestamptz not null,
  last_sync_at timestamptz,
  updated_at timestamptz not null default now(),
  unique (id, user_id)
);

create table public.usage_events (
  id text primary key check (char_length(id) = 64),
  user_id uuid not null default auth.uid() references public.profiles(id) on delete cascade,
  device_id uuid not null,
  provider text not null,
  source text not null,
  source_type text not null check (source_type in ('local_cli', 'browser', 'api', 'manual')),
  native_session_id text not null,
  native_event_id text not null,
  occurred_at timestamptz not null,
  model text,
  project_name text check (project_name is null or (char_length(project_name) <= 96 and project_name !~ '[/\\]')),
  input_tokens bigint not null default 0 check (input_tokens >= 0),
  cached_input_tokens bigint not null default 0 check (cached_input_tokens >= 0),
  output_tokens bigint not null default 0 check (output_tokens >= 0),
  reasoning_tokens bigint not null default 0 check (reasoning_tokens >= 0),
  total_tokens bigint not null default 0 check (total_tokens >= 0),
  estimated_api_value_usd_micros bigint,
  pricing_status text not null default 'unavailable' check (pricing_status in ('available', 'unavailable', 'partial')),
  measurement_kind text not null check (measurement_kind in ('measured', 'estimated', 'activity_only')),
  created_at timestamptz not null,
  updated_at timestamptz not null default now(),
  foreign key (device_id, user_id) references public.devices(id, user_id) on delete cascade,
  unique (user_id, device_id, provider, native_session_id, native_event_id)
);

create index usage_events_user_occurred_idx on public.usage_events(user_id, occurred_at desc);
create index usage_events_user_provider_idx on public.usage_events(user_id, provider, occurred_at desc);
create index usage_events_user_updated_idx on public.usage_events(user_id, updated_at);
create index usage_events_user_device_idx on public.usage_events(user_id, device_id, occurred_at desc);

create table public.subscriptions (
  id text not null,
  user_id uuid not null default auth.uid() references public.profiles(id) on delete cascade,
  provider text not null,
  plan_name text not null,
  monthly_price_usd_cents integer not null check (monthly_price_usd_cents >= 0),
  billing_cadence text not null check (billing_cadence in ('monthly', 'annual')),
  active boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (user_id, id)
);

create table public.pricing (
  id uuid primary key default gen_random_uuid(),
  provider text not null,
  model_pattern text not null,
  effective_from timestamptz not null,
  min_input_tokens bigint not null default 0 check (min_input_tokens >= 0),
  max_input_tokens bigint check (max_input_tokens is null or max_input_tokens >= min_input_tokens),
  input_usd_micros_per_million bigint not null check (input_usd_micros_per_million >= 0),
  cached_input_usd_micros_per_million bigint,
  output_usd_micros_per_million bigint not null check (output_usd_micros_per_million >= 0),
  reasoning_pricing_behavior text not null check (reasoning_pricing_behavior in ('included_in_output', 'separate', 'unavailable')),
  reasoning_usd_micros_per_million bigint,
  version integer not null,
  created_at timestamptz not null default now(),
  unique (provider, model_pattern, effective_from, min_input_tokens, version)
);

create or replace function public.set_updated_at()
returns trigger
language plpgsql
security invoker
set search_path = ''
as $$
begin
  new.updated_at = now();
  return new;
end;
$$;

create trigger profiles_set_updated_at before update on public.profiles
for each row execute function public.set_updated_at();
create trigger devices_set_updated_at before update on public.devices
for each row execute function public.set_updated_at();
create trigger usage_events_set_updated_at before update on public.usage_events
for each row execute function public.set_updated_at();
create trigger subscriptions_set_updated_at before update on public.subscriptions
for each row execute function public.set_updated_at();

create or replace function public.handle_new_user()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  insert into public.profiles (id, email) values (new.id, new.email)
  on conflict (id) do update set email = excluded.email;
  return new;
end;
$$;

create trigger on_auth_user_created
after insert or update of email on auth.users
for each row execute function public.handle_new_user();

alter table public.profiles enable row level security;
alter table public.devices enable row level security;
alter table public.usage_events enable row level security;
alter table public.subscriptions enable row level security;
alter table public.pricing enable row level security;

create policy profiles_select_own on public.profiles for select to authenticated
using ((select auth.uid()) = id);
create policy profiles_update_own on public.profiles for update to authenticated
using ((select auth.uid()) = id) with check ((select auth.uid()) = id);

create policy devices_select_own on public.devices for select to authenticated
using ((select auth.uid()) = user_id);
create policy devices_insert_own on public.devices for insert to authenticated
with check ((select auth.uid()) = user_id);
create policy devices_update_own on public.devices for update to authenticated
using ((select auth.uid()) = user_id) with check ((select auth.uid()) = user_id);
create policy devices_delete_own on public.devices for delete to authenticated
using ((select auth.uid()) = user_id);

create policy usage_events_select_own on public.usage_events for select to authenticated
using ((select auth.uid()) = user_id);
create policy usage_events_insert_own on public.usage_events for insert to authenticated
with check ((select auth.uid()) = user_id);
create policy usage_events_update_own on public.usage_events for update to authenticated
using ((select auth.uid()) = user_id) with check ((select auth.uid()) = user_id);
create policy usage_events_delete_own on public.usage_events for delete to authenticated
using ((select auth.uid()) = user_id);

create policy subscriptions_select_own on public.subscriptions for select to authenticated
using ((select auth.uid()) = user_id);
create policy subscriptions_insert_own on public.subscriptions for insert to authenticated
with check ((select auth.uid()) = user_id);
create policy subscriptions_update_own on public.subscriptions for update to authenticated
using ((select auth.uid()) = user_id) with check ((select auth.uid()) = user_id);
create policy subscriptions_delete_own on public.subscriptions for delete to authenticated
using ((select auth.uid()) = user_id);

create policy pricing_read_authenticated on public.pricing for select to authenticated using (true);

revoke all on public.profiles, public.devices, public.usage_events, public.subscriptions, public.pricing from anon;
grant select, update on public.profiles to authenticated;
grant select, insert, update, delete on public.devices, public.usage_events, public.subscriptions to authenticated;
grant select on public.pricing to authenticated;

comment on table public.usage_events is
  'Normalized usage metadata only. Prompts, responses, source code, commands, files, full paths, secrets, and environment variables are prohibited.';
