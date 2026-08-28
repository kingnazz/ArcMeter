-- Run after migrations in a Supabase local database. The transaction is always rolled back.
begin;

insert into auth.users(id, aud, role, email, email_confirmed_at, created_at, updated_at) values
  ('10000000-0000-0000-0000-000000000001', 'authenticated', 'authenticated', 'owner-a@example.test', now(), now(), now()),
  ('20000000-0000-0000-0000-000000000002', 'authenticated', 'authenticated', 'owner-b@example.test', now(), now(), now());

insert into public.devices(id, user_id, friendly_name, os, architecture, app_version, created_at, last_seen_at) values
  ('10000000-0000-0000-0000-000000000011', '10000000-0000-0000-0000-000000000001', 'A device', 'windows', 'x86_64', 'test', now(), now()),
  ('20000000-0000-0000-0000-000000000022', '20000000-0000-0000-0000-000000000002', 'B device', 'macos', 'aarch64', 'test', now(), now());

insert into public.usage_events(
  id, user_id, device_id, provider, source, source_type, native_session_id, native_event_id,
  occurred_at, total_tokens, measurement_kind, created_at
) values
  (repeat('a', 64), '10000000-0000-0000-0000-000000000001', '10000000-0000-0000-0000-000000000011', 'codex', 'codex_cli', 'local_cli', 'session-a', 'event-a', now(), 100, 'measured', now()),
  (repeat('b', 64), '20000000-0000-0000-0000-000000000002', '20000000-0000-0000-0000-000000000022', 'codex', 'codex_cli', 'local_cli', 'session-b', 'event-b', now(), 200, 'measured', now());

insert into public.subscriptions(id, user_id, provider, plan_name, monthly_price_usd_cents, billing_cadence) values
  ('subscription-a', '10000000-0000-0000-0000-000000000001', 'openai', 'A plan', 2000, 'monthly'),
  ('subscription-b', '20000000-0000-0000-0000-000000000002', 'anthropic', 'B plan', 3000, 'monthly');

set local role authenticated;
select set_config('request.jwt.claim.sub', '10000000-0000-0000-0000-000000000001', true);

do $$
begin
  if (select count(*) from public.devices) <> 1 then
    raise exception 'RLS failure: account A can see another account device';
  end if;
  if (select count(*) from public.usage_events) <> 1 then
    raise exception 'RLS failure: account A can see another account usage event';
  end if;
  if (select coalesce(sum(total_tokens), 0) from public.usage_events) <> 100 then
    raise exception 'RLS failure: account A totals include another account';
  end if;
  if (select count(*) from public.subscriptions) <> 1 then
    raise exception 'RLS failure: account A can see another account subscription';
  end if;
end;
$$;

do $$
begin
  begin
    insert into public.devices(id, user_id, friendly_name, os, architecture, app_version, created_at, last_seen_at)
    values ('30000000-0000-0000-0000-000000000033', '20000000-0000-0000-0000-000000000002', 'Cross-account', 'windows', 'x86_64', 'test', now(), now());
    raise exception 'RLS failure: cross-account device insert unexpectedly succeeded';
  exception when insufficient_privilege or check_violation then
    null;
  end;
end;
$$;

do $$
declare
  affected integer;
begin
  update public.devices
  set friendly_name = 'Cross-account update'
  where id = '20000000-0000-0000-0000-000000000022';
  get diagnostics affected = row_count;
  if affected <> 0 then
    raise exception 'RLS failure: cross-account device update unexpectedly succeeded';
  end if;

  delete from public.usage_events
  where id = repeat('b', 64);
  get diagnostics affected = row_count;
  if affected <> 0 then
    raise exception 'RLS failure: cross-account usage event delete unexpectedly succeeded';
  end if;
end;
$$;

rollback;
