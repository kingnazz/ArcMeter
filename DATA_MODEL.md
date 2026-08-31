# ArcMeter data model

## Canonical usage event

`UsageEvent` is the normalization boundary shared by every collector.

| Field | Meaning |
| --- | --- |
| `id` | SHA-256 of provider, native session ID, and native event ID. |
| `provider`, `source`, `sourceType` | Ecosystem and authoritative origin. |
| `nativeSessionId`, `nativeEventId` | Stable source identity or documented fallback fingerprint. |
| `occurredAt` | UTC RFC 3339 event time. |
| `model` | Provider-reported model when present. |
| `projectName` | Sanitized basename only. |
| token fields | Non-negative authoritative counters, including generic 5m/1h cache-write detail. Provider documentation defines whether cache counters are included or additive. Explicit provider total wins over a derived value. |
| `nativeCostUsdTicks` | Optional provider-recorded fixed-point cost in exact 10^-10 USD ticks; never an ArcMeter estimate. |
| `estimatedApiValueUsdMicros` | Derived estimate, never actual spend. Null when pricing is unavailable. |
| `pricingStatus` | `available`, `partial`, or `unavailable`. |
| `measurementKind` | `measured`, `estimated`, or `activity_only`. |
| `deviceId` | Stable ArcMeter installation UUID. |

Codex reports cached input as a subset of input and reasoning as a subset of output. The pricing engine subtracts cached input before applying the ordinary input rate and does not add reasoning twice when the pricing rule says it is included in output.

Claude reports fresh input, cache reads, cache writes, and output as separate additive counters. `cacheWriteTokens` remains the authoritative aggregate; `cacheWrite5mTokens` and `cacheWrite1hTokens` preserve TTL detail when present. Claude reasoning detail is a subset of output and is never added to total tokens a second time.

## Local SQLite

- `devices`: stable local/remote installations and sync state.
- `usage_events`: normalized ledger with aggregate and sync indexes.
- `subscriptions`: editable recurring costs; default families are inactive and cost $0 until the user configures them.
- `collector_state`: opaque source fingerprints and parser progress—never an uploaded path.
- `sync_state`: last successful remote cursor and supporting sync metadata.
- `pricing`: versioned effective model price rules.
- `app_settings`: allow-listed local preferences and the persistent local device ID.
- `schema_migrations`: applied migration record.

All monetary integers use exact units: subscription prices are USD cents, API-equivalent value is USD micros, and provider-recorded native cost is preserved as 10^-10 USD ticks. Floating point is used only for display.

## Identity and collision behavior

When a source provides native IDs, identity is:

```text
sha256(provider + unit-separator + nativeSessionId + unit-separator + nativeEventId)
```

Codex's native event identity is its JSONL ordinal inside the native session. Grok Build identity hashes session, completed turn, and model so one multi-model turn can produce independently deduplicated child events. Fallback identities hash session, timestamp, model, token counters, and the smallest stable source discriminator. SHA-256 collision risk is negligible; if one occurs, database uniqueness treats the records as one event and diagnostics provide investigation context. ArcMeter never increments a duplicate.

Legacy Grok rows that uniquely match an authoritative completed turn are retained with `supersededByEventId` and excluded from analytics. Ambiguous rows remain active and surface a diagnostic rather than being deleted or silently hidden.

Claude parser version 2 uses request identity before message/UUID identity and carries exact, local-only reconciliation hints for IDs produced by parser version 1. Proven legacy records are retained as superseded; ambiguous rows remain active. These hints contain only deterministic hashes and are not synced as a separate payload.

## Pricing and value

A pricing rule is versioned by provider, model pattern, effective date, input-context tier, and version. `inputTokenSemantics` distinguishes providers whose cache counters are included in input from providers whose counters are additive. Calculations can price fresh input, cache reads, 5m writes, 1h writes, output, and an explicit reasoning behavior. Known components may produce a `partial` lower-bound subtotal while an unknown component remains unpriced. If no safe mapping exists, the event stays `pricing_status = unavailable`. Actual subscription cost is never called API spend.
