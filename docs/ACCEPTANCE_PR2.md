# PR #2 accuracy and acceptance pass

## Telemetry semantics

| Evidence | UI classification | Inference routing |
| --- | --- | --- |
| Codex CLI, Claude Code, Gemini CLI, Grok Build documented collectors | Tool telemetry | Unknown; a tool can use hosted or local inference |
| oMLX schema v1 hourly usage database | Local runtime telemetry | Local runtime inference; no client/tool attribution |
| Runtime inventory or model state change | Runtime observation | Availability only; no inferred request, token or active time |
| An unfamiliar collector/provider pair | Unclassified telemetry | Unknown |
| Authoritatively identified hosted inference | Reserved for future evidence | No current collector establishes this classification |

Classification matches documented collector source/provider pairs, never a tool or
model name alone. Local Models Today uses the same authoritative oMLX source and
this device. Overview/Insights have separate layer totals. Activity filters use
**Telemetry layer**, and tool rows say **Tool · route unknown**. Settings explains
unknown tool routing. The tray and combined charts/headlines say telemetry tokens
and disclose possible overlap. Source percentages describe recorded token counts,
not shares of unique inference. There is no local percentage or local/cloud split.

If Codex records 120 Qwen tokens and oMLX records 120 in the same hour, ArcMeter
retains both records: tool telemetry 120, local runtime telemetry 120. A combined
ledger count of 240 is explicitly telemetry and may include overlapping inference.
Neither model equality nor timestamps prove a request relationship. No overlap
score, automatic subtraction, content matching or cross-source deduplication is
performed. Ordinary deterministic replay prevention within each source remains.
This example is a regression fixture, not a live observation.

## Live environment evidence and outstanding acceptance

The accessible execution host is Windows. Read-only checks found no oMLX service
on loopback ports 8000 or 8080, no standard oMLX usage database, and no configured
`OMLX_BASE_PATH`. Ollama/LM Studio default endpoints were also unreachable.
The existing ArcMeter database has no runtime-state table or saved endpoint
override. Available WSL inventory contains Docker Desktop only. No configuration
was changed, runtime request submitted, model loaded/unloaded or raw log copied.
Access details for the actual oMLX installation have been requested in the chat.

| Live acceptance item | Result on this accessible host |
| --- | --- |
| oMLX discovery / endpoint | Not detected at the checked standard loopback candidates |
| Loaded Qwen model, exact model ID/context and health | Not observable; no live model detected |
| `usage.sqlite3`, historical input/output/cache/request counts | Not accessible; no live telemetry read |
| Current-hour growth and repeated live scan replay | Pending access; covered with native schema fixtures |
| Codex running through oMLX simultaneously | Not observable; no claim of live overlap confirmation |
| Model unload/reload and UI updates/responsiveness under live inference | Pending live host; transitions covered with loopback/fixture tests |
| Native app restart stale until redetection | Covered by native state tests; live host acceptance pending |
| Native tray appearance | Wording/code and native build checked; visual native tray acceptance pending |
| Privacy | Allowlisted metadata normalization, forbidden-field fixtures and bidirectional sync guards pass; live-host persistence audit pending |

The live pass must run on the actual oMLX host, or through an already authorized
loopback connection with its selected metadata database available. A remote URL
alone is intentionally not accepted by ArcMeter. On that host: record the initial
allowlisted runtime/model metadata and hourly counters, observe real Codex traffic
over the next collection interval, compare both separate streams, repeat scans,
observe unload/reload, then restart ArcMeter and verify stale-to-redetected state.
Do not use request contents or assume matching counts prove identity.

## Collector performance

Measured with the actual Rust collector in an unoptimized Windows GNU test build
against synthetic schema-v1 history: 400 days × 24 hours × four models = 38,400
buckets. These are performance fixtures, not the user's oMLX history.

| Measurement | Before | After |
| --- | --- | --- |
| Initial import | 7,440 ms | 7,952 ms |
| Three unchanged replays | 6,188 / 6,432 / 6,566 ms | 397 / 476 / 475 ms |

The collector now loads durable per-bucket high-water counters from its existing
canonical ledger, skips unchanged event-ID hashing/writes, and handles request-only growth
locally without requeueing token sync. It still reads the bounded source history
read-only so older changed buckets, source resets and restored history remain
detectable. No file-time cursor or extra persisted collector schema is needed.
The slowest measured steady replay uses about 0.8% of a 60-second wall-time budget;
the initial import remains on the background blocking worker. SQLite uses WAL.

## Regression coverage and local validation

- Frontend: 41 tests, TypeScript, ESLint and production build passed.
- Native: 130 passed, two existing environment-specific tests ignored; formatting,
  Clippy with warnings denied, and unbundled Windows GNU debug build passed.
- New accuracy regressions cover all four tools as tool telemetry, undocumented
  sources as unclassified, separate Codex/oMLX rows with identical model/time,
  explicit unknown routing/hosted inference unavailable, no percentage, and
  qualified combined counts.
- Collector tests cover 38,400-bucket replay without sync requeue, request-only
  growth, missing local-detail repair, late historical growth, current-hour
  growth, resets, 100 repeated scans and existing privacy/cache/sync behavior.
- Model identifiers now share the runtime metadata whitelist at tool ingestion
  and both sync boundaries. Paths/URLs become unavailable model metadata without
  losing measured counters. Accepted IDs retain all 160 allowed bytes; hourly
  replay repairs the earlier 128-character truncation in place without creating
  another bucket. Regressions cover path rejection, legacy sync metadata and
  long-ID repair/replay. Existing local rows are not destructively rewritten.
- The local GNU test loader issue was resolved by embedding a common-controls v6
  manifest into local test executables. This does not change repository sources.
- The loopback test harness explicitly clears inherited nonblocking mode on
  accepted sockets (BSD/macOS) and reads complete, bounded request headers. This
  fixes a CI timing race without increasing production collector timeouts.

CI runs the full frontend suite plus Rust formatting, Clippy, tests and native
production builds on Windows and macOS. The current PR checks provide the final
commit result; passing CI does not substitute for the outstanding live oMLX pass.

## Visual QA and recommendation

Overview, Local Models, Activity, Insights and Settings were inspected in the real
browser-safe empty state at normal desktop and narrower widths. No model or token
data was fabricated in the application. A light-mode Activity empty-state contrast
issue found during QA was corrected using the existing theme surface token.

- [Revised Overview, empty state, dark](screenshots/telemetry-overview-empty-dark.png)
- [Revised Insights, empty state, light](screenshots/telemetry-insights-empty-light.png)

No real local-model screenshot is available because the live host is inaccessible.
Keep PR #2 in draft and do not merge or release until live Qwen/oMLX and simultaneous
Codex acceptance are completed. The implementation and automated checks can be
reviewed independently of that remaining acceptance evidence.
