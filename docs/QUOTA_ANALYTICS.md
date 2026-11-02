# Quota pace analytics

ArcMeter derives quota pace only from normalized percentages reported by Claude and Grok. It never converts local tokens, active minutes, API-equivalent value, provider-recorded cost, or subscription price into subscription quota. No credentials, account identity, prompts, responses, code, or raw provider payloads enter this analysis.

The Rust native layer is the source of truth. It queries at most 512 normalized rows for each current provider/window, requires the same `source_device_id`, and requires exact compatibility with the current provider period start and reset metadata. This conservative same-device rule avoids combining synced devices that may be signed into different provider accounts. A changed period/reset starts a new series; an unexplained utilization drop greater than 500 basis points also starts a new segment.

Recent pace uses the last 60 minutes. If that contains fewer than three observations or less than ten minutes of span, ArcMeter expands within the active segment up to six hours. With enough data, it computes all pairwise time-normalized slopes and takes their median (Theil–Sen). Values remain integer basis points; downward corrections are neutralized so quota jitter cannot appear as regeneration. Flat series are presented as “No recent change.”

Confidence is intentionally coarse:

- `insufficient`: fewer than three observations or less than ten minutes of span;
- `low`: stale data, fewer than four samples, or less than 30 minutes;
- `medium`: sufficient history that does not meet the high threshold;
- `high`: at least seven samples, at least 60 minutes, and at least 80% monotonic consistency.

Only `rolling`, `weekly`, `monthly`, and `model_weekly` windows are treated as independent caps. `product` and unknown windows may show their current percentage and recent change, but never receive an exhaustion ETA. For medium/high-confidence positive burn, projected exhaustion is `remaining basis points / recent basis points per hour`. The UI compares that estimate with the provider reset and avoids displaying an irrelevant post-reset date. Projections are suppressed for stale data, low/insufficient confidence, flat pace, and already-reached limits.

When a provider period start is known, period-average pace is the current reported utilization divided by elapsed period time. It is kept separate from recent Theil–Sen pace and is not fabricated when the start is unknown.

All analytics are computed on read from existing normalized history. Burn rate, confidence, and projections are not persisted, no provider requests are added, history is not deleted, and no schema migration is required.
