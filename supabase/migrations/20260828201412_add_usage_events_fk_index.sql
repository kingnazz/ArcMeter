-- Cover the composite device foreign key in its declared column order.
create index usage_events_device_user_fk_idx
on public.usage_events(device_id, user_id);
