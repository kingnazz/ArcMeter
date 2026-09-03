# Cache Efficiency

ArcMeter's cache analytics are read-time views over the local normalized usage ledger. They do not create a sync table or collect any new provider data.

## Canonical source and date ranges

Only `measurement_kind = 'measured'` events whose `superseded_by_event_id` is null are eligible. Activity-only browser/desktop minutes, estimated browser telemetry, and superseded cumulative revisions are excluded. Today, 7-day, and 30-day ranges start at local midnight; All includes all eligible history. Unlike the Sessions page, a cache range totals only events that occurred inside the period rather than selecting whole sessions by their last activity.

The provider filter is applied to the same bounded event query and never changes token semantics. Provider, model, and project breakdowns are produced in native Rust from that single result set. Project labels use only the ledger's sanitized `project_name`; missing values become `Unassigned`.

## Input normalization

ArcMeter uses source-specific semantics established by the collectors and mirrored by versioned pricing metadata:

- Codex CLI: `cache_included`. Codex `input_tokens` includes its cached-input counter.
- Claude Code: `cache_additive`. Anthropic reports fresh input, cache reads, and cache creation separately.
- Grok Build: `cache_included`. Its normalized input counter includes cached input; cache creation is an aggregate write with no invented TTL.
- Gemini CLI: `cache_included` when `cachedContentTokenCount` is present. Ordinary input is never converted into cache activity.
- Unknown provider/source pairs: unknown semantics. Absolute measured counters remain visible, while the reuse percentage is suppressed or explicitly partial in a mixed aggregate.

For cache-included telemetry:

```text
fresh input = max(0, input - cache read - total cache write)
```

For cache-additive telemetry:

```text
fresh input = input
```

The normalized input context and reuse share are:

```text
context = fresh input + cache read + total cache write
reuse share = cache read / context
```

The native layer returns reuse share as integer basis points; floats are not persisted. Semantic coverage is `complete`, `partial`, or `unavailable`. A partial aggregate's percentage covers only events with known semantics and is labeled accordingly.

This is called **input reuse**, not cache hit rate: provider telemetry does not expose every internal cache lookup attempt or miss.

## Cache writes and TTL detail

`cache_write_tokens` is the total. Five-minute and one-hour writes are subcategories and are never added to that total. The safe remainder is:

```text
unspecified write = max(0, total write - 5m write - 1h write)
```

Aggregate-only writes are not assigned to a TTL. Claude's reported 5-minute and 1-hour detail is retained. Grok's aggregate cache creation remains aggregate-only.

## API-equivalent cache impact

For a cache-bearing event, ArcMeter selects the same effective-date, model-pattern, and context-tier pricing rule used by the existing pricing engine. It compares each safely priced cache read/write component with the same token volume at that rule's standard fresh-input rate:

```text
impact = counterfactual fresh-input value - actual cache-component value
```

Output and reasoning cancel out of this comparison and are unchanged. A positive result means a lower API-equivalent value; a negative result correctly shows that cache creation cost exceeded cache-read savings in the period. Replacing cache categories does not alter context volume or select a different tier.

Coverage is `complete` when every cache-bearing event/component is safely comparable, `partial` when only a defensible subtotal exists, and `unavailable` when none does. Missing components are never extrapolated. Aggregate writes without a known duration-specific price make that component unavailable.

This value is not actual subscription savings. Provider-recorded native cost remains separate and is never used in the counterfactual.

## Wording and privacy

Cache reads are described as tokens reused, cached input, or cache reads—not “tokens saved,” because the context tokens were still served. ArcMeter does not attribute a read to a specific earlier write.

The feature uses only normalized token counts, model/provider/source identifiers, sanitized project names, session identifiers used for local grouping, timestamps, and versioned pricing rules. It does not collect or expose prompts, responses, code, commands, paths, browser content, identity, credentials, cookies, or raw provider payloads.
