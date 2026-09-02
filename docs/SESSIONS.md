# Sessions view

The Sessions view is a local, read-time summary of authoritative work. It creates no summary table, does not call a provider, and makes no cloud or Supabase change.

## Eligibility and identity

Only canonical events with `measurement_kind = measured` and `superseded_by_event_id IS NULL` participate. Estimated events and activity-only foreground minutes never appear in a session.

The logical identity is:

```text
(provider, source, native_session_id)
```

This keeps coincident native IDs from different providers or sources separate. Canonical event IDs already deduplicate cross-device copies, so the same logical session can safely combine its devices. The native session ID is an opaque detail lookup key and is never rendered as a title or user-facing identifier.

## Summary rules

Session summaries are grouped locally and include only sanitized metadata: provider/source, sanitized project basename, model, device label, timestamps, token counters, and fixed-point monetary totals. The primary project is chosen by event count, then most recent activity; the primary model is chosen by token count, then event count. A date range qualifies a session by its `last_activity_at`; after qualification, every canonical event in that session contributes to its totals, including an event immediately before the range boundary. Likewise, a text match on any safe metadata event qualifies the session but never narrows its aggregate.

API-equivalent value remains separate from recorded provider cost. Pricing coverage is `complete` only when every token-bearing event has exact `available` pricing and a value. It is `partial` when one or more `available` or `partial` events provide a defensible subtotal but full coverage is not complete, including a partial-only session. It is `unavailable` when no token-bearing event has a defensible value. Recorded native cost is always labeled as provider-recorded, never estimated.

## Query behavior

The list loads at most 50 grouped sessions at a time. Opening a detail panel runs its targeted model, device, and event queries; the event timeline loads at most 100 events per page. Provider, date, project search, and sort filters apply only to the session query. Project search uses safe metadata only: project, provider, source, model, and friendly device name.

The local ledger's `idx_usage_events_measurement(measurement_kind, occurred_at DESC)` supports the canonical measured-event scan used to find session candidates. No new migration or index is required for this feature; the native test suite exercises 10,000 measured events across 1,000 sessions.
