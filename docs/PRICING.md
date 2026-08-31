# Pricing provenance

ArcMeter uses a local, versioned catalog of public API list prices. Each event is valued with the rate whose `effective_from` date applied when the event occurred; later price changes never rewrite older usage at the new rate. Version `0.2.2` adds the prior year of observed model pricing for GPT-5.3 Codex, GPT-5.4, GPT-5.5, GPT-5.6 Sol/Terra/Luna, Claude Sonnet 4.6, and Claude Opus 4.7. The GPT-5.6 promotional schedule remains effective from `2026-08-27T00:00:00Z`.

The seed migration records standard text/API-equivalent input, cached-input, output, and documented long-context tiers from official provider pages:

- OpenAI API pricing, model pages, and dated Codex releases: <https://developers.openai.com/api/docs/pricing>, <https://developers.openai.com/api/docs/models/gpt-5.6-sol>, <https://developers.openai.com/api/docs/models/gpt-5.5>, <https://developers.openai.com/api/docs/models/gpt-5.4>, <https://developers.openai.com/api/docs/models/gpt-5.3-codex>, and <https://learn.chatgpt.com/docs/changelog>
- Anthropic Claude pricing and dated releases: <https://platform.claude.com/docs/en/about-claude/pricing>, <https://www.anthropic.com/news/claude-sonnet-4-6>, and <https://www.anthropic.com/news/claude-opus-4-7>
- Google Gemini API pricing: <https://ai.google.dev/gemini-api/docs/pricing>

Rates are stored as integer USD micros per million tokens. Context tiers are separate rows keyed by provider, model pattern, effective date, minimum input-token boundary, and version. Cached input is subtracted from ordinary input before calculation. Reasoning is not added twice when the provider reports it as part of output.

Grok API-equivalent pricing remains `pricing_status = unavailable` because activity-only browser tracking contains no token telemetry and no exact static rate mapping was seeded for Grok Build identifiers. Completed Grok Build turns may separately carry provider-recorded `costUsdTicks`; ArcMeter preserves and labels that value as recorded provider cost without treating it as an API-equivalent estimate. Unknown models and events before a model's verified availability date also remain unavailable. ArcMeter sums only events with an exact pricing match. When a selected range also contains unavailable events, the UI marks the result as a lower-bound partial estimate and shows the share of measured tokens covered by safe pricing instead of guessing a rate for the remainder.

## Public list price and scenario value

“Historical public API list value” is the closest verifiable MSRP-like comparison: it answers what the measured tokens would have cost at the provider's published API rate at that time. It is not the user's subscription spend, the provider's internal cost, or proof of a subsidy.

The Overview calculator applies a user-selected 1×–10× multiplier to that verified subtotal. Values above 1× are explicitly hypothetical “unsubsidized” scenarios. No provider publishes enough cost and margin data for ArcMeter to label one multiplier as the actual unsubsidized price.
