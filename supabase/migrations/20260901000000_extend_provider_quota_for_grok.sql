-- Generic quota metadata for product/monthly periods and verified fixed-point balances.
alter table public.provider_quota_snapshots
  drop constraint if exists provider_quota_snapshots_kind_check;

alter table public.provider_quota_snapshots
  add constraint provider_quota_snapshots_kind_check
    check (kind in ('rolling', 'weekly', 'monthly', 'model_weekly', 'product', 'other')),
  add column period_starts_at timestamptz,
  add column extra_prepaid_balance_minor bigint
    check (extra_prepaid_balance_minor is null or extra_prepaid_balance_minor >= 0),
  add column plan_label text
    check (plan_label is null or char_length(plan_label) between 1 and 80);

comment on column public.provider_quota_snapshots.period_starts_at is
  'Optional provider-supplied start of the normalized quota period.';
comment on column public.provider_quota_snapshots.extra_prepaid_balance_minor is
  'Optional verified minor currency units; never an OAuth token or provider response.';
comment on column public.provider_quota_snapshots.plan_label is
  'Optional sanitized subscription display label without account identity.';
