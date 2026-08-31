# Claude Code telemetry

ArcMeter reads Claude Code CLI transcript metadata from known project roots only:

- `CLAUDE_CONFIG_DIR/projects` (path-list entries are supported and deduplicated)
- `~/.claude/projects`
- `~/.config/claude/projects`

It does not recursively search the home directory or read Claude credentials. Anthropic documents the default hierarchy as `projects/<project>/<session-id>.jsonl` and warns that individual JSONL records are an internal, version-varying format. ArcMeter therefore ignores unknown record fields and only accepts assistant records that expose a usage object, session identity, and a valid timestamp.

## Request identity

One API request can appear in more than one assistant record. ArcMeter emits one event using the first available identity in this order:

1. `requestId` / `request_id`
2. `message.id`
3. `uuid`
4. a SHA-256 fallback over session, parent identity when present, timestamp, and raw model

The fallback never uses a JSONL row number. When duplicate snapshots share the same request identity, ArcMeter keeps the most complete cumulative usage snapshot instead of summing them. Database identity remains the SHA-256 of provider, session, and the selected native request identity, so replay and equivalent source roots insert no duplicates.

Claude request events are revisable because Claude Code can append increasingly complete snapshots after ArcMeter has already scanned a file. A new request is inserted normally. A same-ID request updates the existing row in place only when its authority tuple improves, in this exact order:

1. greater authoritative total tokens
2. greater 5-minute plus 1-hour cache-write detail
3. greater output tokens
4. newly available model metadata, then newly available basename-only project metadata
5. later provider timestamp when the usage and metadata above are otherwise equivalent

A lower tuple is stale and cannot replace the stored row. The update additionally requires the same provider, source, source type, session, native request identity, originating device, measurement kind, and deterministic ID. Missing model or project metadata never erases a stored value. An accepted revision preserves `id` and `created_at`, advances `updated_at`, and returns `sync_status` to `pending`.

ArcMeter recalculates local pricing from the incoming model and complete token breakdown before this write. The existing Supabase upload remains a primary-key merge on the same event ID, so the pending revision updates the cloud row rather than creating another event. This live same-ID revision is separate from parser-v1-to-v2 reconciliation, which maps an old event ID to the new request-level event ID.

Parser version 2 reconstructs the exact event IDs produced by parser version 1 for every source record. Existing legacy rows are retained and marked with `superseded_by_event_id` only when that source identity proves the replacement. An additional exact token-shape reconciliation handles a uniquely matching legacy row. Ambiguous or unmatched rows remain active and produce a Settings diagnostic.

## Token accounting

Anthropic's usage counters are additive:

- `input_tokens` → fresh, non-cache input
- `cache_read_input_tokens` → cache reads
- `cache_creation_input_tokens` → total cache writes
- `cache_creation.ephemeral_5m_input_tokens` → 5-minute cache writes
- `cache_creation.ephemeral_1h_input_tokens` → 1-hour cache writes
- `output_tokens` → authoritative output total
- `output_tokens_details.thinking_tokens` → optional reasoning subset of output

ArcMeter total tokens are fresh input + cache reads + cache writes + output. Reasoning is not added again. If detailed cache creation is present, the two TTL counters are preserved and total cache writes are at least their sum. If only aggregate creation is present, ArcMeter stores that aggregate but does not invent a TTL split.

The raw provider model is stored unchanged. Pricing-family prefix matching is separate and does not rewrite displayed model identity. Claude transcript usage records inspected through the documented schema do not expose a trustworthy provider-native cost field, so ArcMeter leaves native cost null.

## Pricing and context

Claude rules use additive input semantics. Fresh input, cache reads, 5-minute writes, 1-hour writes, and output are priced independently using effective-dated public API list prices. An aggregate cache write without TTL detail yields a safe partial subtotal rather than an assumed write price. Unknown models and unsupported context tiers remain measured with unavailable pricing.

Current Claude 4.6-and-later cataloged models use standard pricing throughout their documented 1M context window. Cataloged 4.5 families are capped at their documented 200k window. Claude Code usage records provide request token counters but not an authoritative live context-occupancy value or model capacity; ArcMeter does not estimate context utilization or store compaction text in this pass.

## Privacy and local validation

Only opaque request/session IDs, UTC timestamps, raw model, basename-only project labels, token counters, and derived pricing metadata enter the event ledger. Prompts, responses, tool arguments, shell commands, source code, full working directories, and absolute paths are not stored or synced.

On the Windows validation system used for this upgrade, `CLAUDE_CONFIG_DIR` was unset and none of the three supported project roots existed. No real Claude transcript was available, so validation used a manually authored content-free fixture containing synthetic IDs and counters only. The launched ArcMeter database contained zero Claude events and zero Claude tokens before migration.

## References

- Anthropic Claude Code sessions: <https://code.claude.com/docs/en/sessions>
- Anthropic Messages usage schema: <https://platform.claude.com/docs/en/api/messages/create>
- Anthropic prompt caching: <https://platform.claude.com/docs/en/build-with-claude/prompt-caching>
- Anthropic pricing: <https://platform.claude.com/docs/en/about-claude/pricing>
- Anthropic context windows: <https://platform.claude.com/docs/en/build-with-claude/context-windows>
