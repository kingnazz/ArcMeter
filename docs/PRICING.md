# Pricing provenance

ArcMeter's local V1 pricing rows are effective from `2026-08-27T00:00:00Z`. That boundary is deliberate: historical events before the verification date remain unavailable rather than being valued retroactively with an unproven rate.

The seed migration records standard text/API-equivalent input, cached-input, output, and documented long-context tiers from official provider pages:

- OpenAI API pricing and GPT-5.6 model documentation: <https://developers.openai.com/api/docs/pricing> and <https://developers.openai.com/api/docs/models/gpt-5.6-sol>
- Anthropic Claude pricing: <https://platform.claude.com/docs/en/about-claude/pricing>
- Google Gemini API pricing: <https://ai.google.dev/gemini-api/docs/pricing>

Rates are stored as integer USD micros per million tokens. Context tiers are separate rows keyed by provider, model pattern, effective date, minimum input-token boundary, and version. Cached input is subtracted from ordinary input before calculation. Reasoning is not added twice when the provider reports it as part of output.

Grok events remain `pricing_status = unavailable` in V1 because no exact official static rate mapping was safely established for the observed Grok Build model identifiers. Unknown models and events before a rule's effective date also remain unavailable. If any measured event in a selected range is unavailable, ArcMeter hides the aggregate API-equivalent value and value multiple instead of guessing.
