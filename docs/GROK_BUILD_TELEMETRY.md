# Grok Build telemetry

ArcMeter reads Grok Build's persisted completed-turn ledger. It does not read prompts,
responses, tool calls, commands, source code, or Grok's rolling unified debug log.

## Discovery and record shape

The supported roots are, in priority order:

1. `${GROK_HOME}/sessions`
2. `~/.grok/sessions`

Within a root, ArcMeter accepts only this verified shape:

```text
sessions/<url-encoded-cwd>/<session-id>/updates.jsonl
```

It does not recursively accept arbitrary JSON or JSONL files. A typical sanitized usage
record is an ACP envelope:

```json
{
  "timestamp": "2026-08-20T12:01:00Z",
  "params": {
    "sessionId": "<session-id>",
    "update": {
      "sessionUpdate": "turn_completed",
      "prompt_id": "<turn-id>",
      "usage": {
        "inputTokens": 100,
        "cachedReadTokens": 40,
        "cacheCreationTokens": 10,
        "outputTokens": 20,
        "reasoningTokens": 8,
        "totalTokens": 120,
        "costUsdTicks": 120000000,
        "modelUsage": {
          "grok-4.5-build": {
            "inputTokens": 100,
            "cachedReadTokens": 40,
            "cacheCreationTokens": 10,
            "outputTokens": 20,
            "reasoningTokens": 8,
            "totalTokens": 120,
            "costUsdTicks": 120000000
          }
        }
      }
    }
  }
}
```

Only `turn_completed` updates with a usable usage breakdown become measurements. Streaming
chunks, tool updates, retries represented only as intermediate updates, and
`session_summary_generated` updates are ignored. A killed in-progress turn has no completed
record and therefore cannot be measured.

## Model and token mapping

ArcMeter emits one `UsageEvent` per valid `modelUsage` item. This preserves raw model IDs and
lets a completed turn that used multiple models remain analyzable without duplicating the
top-level turn total. When `modelUsage` is absent, ArcMeter emits one event from the top-level
usage and preserves a safe model ID when one is present. Unknown future IDs remain collectable;
they simply have unavailable API-equivalent pricing.

Grok Build's counters map as follows:

| Grok Build | ArcMeter | Semantics |
| --- | --- | --- |
| `inputTokens` | `inputTokens` | Full input, including cache reads and cache writes. |
| `cachedReadTokens` | `cachedInputTokens` | Cache-read subset of input; never added to input. |
| `cacheCreationTokens` | `cacheWriteTokens` | Cache-write subset of input; never added to input. |
| `outputTokens` | `outputTokens` | Full output, including reasoning. |
| `reasoningTokens` | `reasoningTokens` | Informational subset of output; never added to output. |
| `totalTokens` | `totalTokens` | Provider total. Current Grok source defines it as input plus output. |

Safe snake_case and older prompt/completion aliases are accepted when their semantics are the
same. Unknown counters are ignored rather than inferred.

## Native cost and API-equivalent value

Current Grok Build source defines `costUsdTicks` as exact fixed-point units of 10^-10 USD.
ArcMeter stores the integer ticks losslessly in `nativeCostUsdTicks` and labels the value
"Recorded provider cost." A partial or incomplete cost is stored as unavailable. For a
single-model turn only, a complete top-level cost may safely fill a missing per-model cost;
ArcMeter never divides a multi-model total heuristically.

`nativeCostUsdTicks` and `estimatedApiValueUsdMicros` are separate fields. No Grok pricing rule
is seeded by this upgrade, so a recorded Grok value is never relabeled as an ArcMeter estimate.

## Identity, metadata, and reconciliation

The stable event identity is derived from the native session, completed-turn ID (or a stable
fallback fingerprint), and raw model ID. Model map ordering and JSONL row position do not affect
identity. Replaying the same file inserts zero duplicates.

The session directory supplies a session ID when the envelope omits it. The URL-encoded project
directory is decoded and passed through ArcMeter's basename-only sanitizer. Only the basename,
session ID, model, timestamp, token counters, exact cost ticks, and device identity can enter the
ledger.

Parser-v1 Grok rows are never deleted. When a legacy row matches one completed turn uniquely on
session, timestamp, and every available token counter, it is marked as superseded and excluded
from analytics while remaining recoverable and syncable. Ambiguous legacy rows stay active and
produce a Settings warning so ArcMeter does not make a destructive guess.

## Validation and references

On the Windows development machine used for this upgrade, `GROK_HOME` was unset and no standard
Grok data directory existed under the user profile or AppData. Consequently there is no real
local before/after usage total to report; validation uses sanitized fixtures.

The format and fixed-point semantics were checked against SpaceXAI's published Grok Build source
(Apache-2.0) and ccusage's Grok documentation/reference behavior (MIT). ArcMeter's Rust parser is
an independent implementation and has no runtime dependency on either project.
