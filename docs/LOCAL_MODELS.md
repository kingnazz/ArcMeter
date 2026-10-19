# Local Models

ArcMeter discovers oMLX, Ollama and LM Studio passively on this computer. Inventory
and runtime observations are distinct from measured usage. No inference requests,
model loads/unloads, proxy, prompt capture or hardware monitoring are involved.

## Repository and UI audit

The source branch is `codex/arcmeter-v1-foundation` at `fd54af1`, the verified remote
default on 2026-10-18. PR #1 was merged into `main` as `3552c8b`; both branches
contain its `f5521d2` feature checkpoint. `main` additionally contains v0.3.0 release
metadata/workflows, while the default branch has two later repository settings
commits. This change branches from and targets the current default. It does not
change versions, merge branches, invoke release workflows or publish installers.
The original checkout's uncommitted README/sync diagnostic edits remain untouched
in that checkout; this work uses an isolated managed worktree.

The audit covered the Rust collectors and scheduler, immutable/revisable event
identities, six SQLite migrations, normalized sync payloads, session/cache/quota
queries, React views, browser empty state, screenshots, tests and CI. The canonical
ledger accepts arbitrary provider strings; sync needs no provider enum migration.
Session grouping previously assumed every measured row was a request/session.

Visual findings: the 570px onboarding panel dominated an empty Overview; labels
were very small and low contrast; navigation needed room for another view; runtime
state had no visual hierarchy; light mode had no theme implementation; narrow
layouts lacked a useful navigation/ledger arrangement. The pass preserves charcoal
and lime, uses system fonts, reduces unused space, raises metadata contrast, and
adds restrained chart/status transitions, visible focus and reduced-motion support.
Native window behavior and the existing provider/quota workflows remain intact.

## Runtime adapters and exact coverage

| Runtime | Read-only discovery | Authoritative metadata | Passive usage |
| --- | --- | --- | --- |
| oMLX | `/v1/models`, `/v1/models/status`; empty inventory can be identified by the `oMLX API` OpenAPI title | Model ID, loaded flag and effective context limit from status; last engine access when supplied. `/v1/models` fallback provides inventory/context with unknown loaded state. | Schema v1 `usage.sqlite3`: model/hour requests, full prompt tokens, completion tokens and cached prompt tokens. Total is the source-defined prompt + completion; cache is already included in input. |
| Ollama | `/api/version`, `/api/tags`, `/api/ps` | IDs/tags, available/loaded models, weight-file size, reported parameter and quantization strings; loaded context from `/api/ps` | No passive historical token/request API used. Runtime transitions only. |
| LM Studio | `/api/v1/models`, falling back to `/api/v0/models` only for HTTP 404 | IDs, inventory/loaded instances, quantization, parameters and size when returned; loaded context only when instances agree. v0 accepts explicit loaded context. | No passive historical token/request API used. Runtime transitions only. |

oMLX's `estimated_size` and `actual_size` describe memory, so they are not presented
as weight-file size. Parameters and quantization are never parsed from a name.
LM Studio trained maximum context is not presented as active context. Multiple
loaded instances with different contexts produce an unknown context, not a guess.

All adapters use GET requests, two-second request timeouts, a 1 MiB response cap
and a 512-model inventory cap. Defaults are loopback ports 8000, 11434 and 1234.
Settings can disable each adapter, restore automatic discovery, override the port,
or test an unsaved draft endpoint without changing analytics. Loopback IPv4/IPv6
and `localhost` are accepted with an optional `/v1` suffix; other paths, remote
hosts, URL credentials, query strings and fragments are rejected. `localhost`
is normalized to 127.0.0.1 for requests, proxies are bypassed, and redirects are
disabled. Authentication-required responses produce a safe diagnostic; ArcMeter
does not read runtime keys or weaken server authentication.

Automatic discovery checks the standard candidates; it does not scan arbitrary
ports. `localhost` uses IPv4; use `[::1]` explicitly for an IPv6-only service.
An OpenAI-compatible server on port 8000 alone is not identified as oMLX.
Generic OpenAI-compatible/llama.cpp support is deferred.

## oMLX source investigation

Primary sources inspected:

- [Usage history schema, accounting and retention](https://github.com/jundot/omlx/blob/main/docs/usage-analytics.md)
- [Hourly SQLite writer](https://github.com/jundot/omlx/blob/main/omlx/usage_history.py)
- [OpenAI models and native status routes](https://github.com/jundot/omlx/blob/main/omlx/server.py)
- [Structured application log formatter](https://github.com/jundot/omlx/blob/main/omlx/logging_config.py)
- [Ollama running models](https://docs.ollama.com/api/ps) and [inventory](https://docs.ollama.com/api/tags)
- [LM Studio native model inventory](https://lmstudio.ai/docs/developer/rest/list)

The OpenAI-compatible usage object belongs to the caller's response. ArcMeter
cannot observe it passively. oMLX structured logs include a free-form message and
optional ambiguous `tokens`/latency metadata; they are not a reliable input/output
usage ledger. ArcMeter therefore does not read application/debug logs. It reads
the documented dedicated metadata database instead, opening it read-only with a
250ms busy timeout and checking its version before selecting allowed columns.

Discovery uses `~/.omlx/usage.sqlite3`, or `OMLX_BASE_PATH` when already configured.
The directory is used in memory only; neither the environment value nor a source
path is stored, returned to React, logged or synced. The source is not modified,
checkpointed, repaired or copied. Unsupported/corrupt/inaccessible databases leave
previous data intact and report only `unavailable`. Import is transaction-bound
and capped at 250,000 rows. Only safe identifier-shaped model IDs are retained.

## Ledger, replay and sync

The canonical source is `omlx_usage_hourly`, source type `api`, measurement kind
`measured`. One deterministic row represents one installation/model/hour. The
opaque bucket series key is hashed; it is not a real session ID. Growing buckets
replace their prior counters and requeue the same event ID for existing sync.
Unchanged replay adds no rows and does not requeue synced rows. Lower counters
after a history reset cannot reduce known totals or create guessed deltas; later
hours continue independently. Current-hour totals may still grow.

Hourly buckets appear in Overview, token/model/provider/device breakdowns,
Activity, Insights, cache analytics and the tray. Sessions deliberately exclude
them. Activity labels the hour start and its aggregation, rather than implying an
exact request time. Cache semantics include cached tokens inside prompt input;
there are no inferred cache writes or reasoning counters. Pricing remains
unavailable because there is no verified runtime/model price mapping. No local
cost or arbitrary API equivalent is invented.

Cloud sync uses the existing normalized event payload, timestamp-guarded upsert
and owner/device RLS. There is no cloud migration or network deployment. Current
bucket revisions replay as one row on a receiving device. Existing CLI collection
and sync semantics are unchanged. Runtime configuration, inventory, state changes
and request counts stay on the source computer; synced token buckets still
participate in combined analytics. The Local Models Today panel is scoped to
this computer; Overview and Insights use the combined ledger.

SQLite migration 7 adds three tables:

- `local_runtime_state`: provider config, safe diagnostic, detection timestamps and
  serialized typed inventory. It is never a raw API response.
- `local_runtime_observations`: server online/unavailable and model loaded/unloaded
  transitions; never foreground minutes, requests or tokens. Retained for 90 days,
  with a 10,000-row ceiling.
- `local_usage_details`: measured request count attached to an hourly event. It
  remains local because the existing sync schema has no request-count field.

Migration tests retain older usage/pricing/quota rows and advance to version 7.
No existing table is destructively rebuilt. Adapter configuration invalidates live
inventory immediately on endpoint changes. Failed polls clear current loaded
state and preserve stale inventory, last successful detection and measured history.

## Privacy review

Only model identifiers, documented numeric metadata, timestamps and runtime states
are constructed at the adapter boundary. Unknown fields (including model paths,
descriptions, messages, prompts, responses, client data and secrets) are discarded.
Identifiers reject absolute paths, path traversal, control characters and URL
syntax. Diagnostics are constant messages; HTTP bodies, URLs with credentials,
SQLite errors and log messages are never echoed. No runtime endpoint, inventory,
raw response or request count enters the existing cloud event serializer. Fixtures
include forbidden extra fields and assert their absence after normalization.

## Limits and next pass

- No live macOS oMLX installation was available in this Windows workspace. API and
  source-schema coverage uses sanitized fixtures and real loopback mock servers.
- Models loaded between polls may not be seen. Loaded does not prove inference.
  oMLX last-access time includes engine load/acquire/release, not just generation.
- Hourly data cannot establish exact request times, conversations, client/tool,
  sessions, foreground time or hardware utilization. Day boundaries use whole
  source buckets; requests straddling a boundary are not interpolated.
- oMLX history must exist and recording must be enabled by oMLX. Older servers and
  Ollama/LM Studio show inventory/state observations without token estimates.
  History predating oMLX's recorder is unavailable. Source retention, unflushed
  batches and failures are inherited; deleting/resetting the source within an
  existing hour can lead to undercounting, never manufactured replacement usage.
- Provider classification cannot establish where a CLI routed its inference.
  A request logged by a CLI and oMLX may appear in both source totals. The split
  is provider-based, not a deduplicated inference workload or cost-savings claim.
- One endpoint per runtime is supported. Standard oMLX local history is collected
  independently of its configured HTTP endpoint; do not assume an overridden
  endpoint identifies the owner of that database.
- Next pass: validate on a live Apple Silicon oMLX host, add explicit source
  coverage/double-count controls, then consider opt-in metadata-only native
  request streams for Ollama/LM Studio if an authoritative passive interface exists.
