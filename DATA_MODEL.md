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
| token fields | Non-negative authoritative counters; explicit provider total wins over derived input + output. |
| `estimatedApiValueUsdMicros` | Derived estimate, never actual spend. Null when pricing is unavailable. |
| `pricingStatus` | `available`, `partial`, or `unavailable`. |
| `measurementKind` | `measured`, `estimated`, or `activity_only`. |
| `deviceId` | Stable ArcMeter installation UUID. |

Codex reports cached input as a subset of input and reasoning as a subset of output. The pricing engine subtracts cached input before applying the ordinary input rate and does not add reasoning twice when the pricing rule says it is included in output.

## Local SQLite

- `devices`: stable local/remote installations and sync state.
- `usage_events`: normalized ledger with aggregate and sync indexes.
- `subscriptions`: editable recurring costs; default families are inactive and cost $0 until the user configures them.
- `collector_state`: opaque source fingerprints and parser progress—never an uploaded path.
- `sync_state`: last successful remote cursor and supporting sync metadata.
- `pricing`: versioned effective model price rules.
- `app_settings`: allow-listed local preferences and the persistent local device ID.
- `schema_migrations`: applied migration record.

All monetary integers use exact minor units: subscription prices are USD cents and API-equivalent value is USD micros. Floating point is used only to display a ratio.

## Identity and collision behavior

When a source provides native IDs, identity is:

```text
sha256(provider + unit-separator + nativeSessionId + unit-separator + nativeEventId)
```

Codex's native event identity is its JSONL ordinal inside the native session. Fallback identities hash session, timestamp, model, token counters, and the smallest stable source discriminator. SHA-256 collision risk is negligible; if one occurs, database uniqueness treats the records as one event and diagnostics provide investigation context. ArcMeter never increments a duplicate.

## Pricing and value

A pricing rule is versioned by provider, model pattern, effective date, input-context tier, and version. Calculations use uncached input, cached input, output, and an explicit reasoning behavior. If the model/rate/effective mapping is not safe, the event stays `pricing_status = unavailable` and aggregate API-equivalent value/value multiple is hidden. Actual subscription cost is never called API spend.
