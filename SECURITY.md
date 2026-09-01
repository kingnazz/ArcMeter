# ArcMeter security and privacy

## Privacy boundary

The synchronized usage ledger may contain only:

- deterministic event ID;
- provider and local tool source;
- source/measurement classification;
- native session/event identifiers;
- timestamp and model;
- sanitized project basename;
- token counters and calculated API-equivalent value;
- ArcMeter device ID.

When the experimental Claude live-limits setting is enabled, ArcMeter may also synchronize normalized quota-window metadata: provider/window keys, generic labels and scope, utilization basis points, provider-supplied UTC reset time, observation time, and source ArcMeter device ID. Extra-usage state may include only the provider-returned enabled flag, currency, utilization, monthly limit, and used credits in fixed-point minor units.

It must never contain prompts, responses, reasoning text, source code, shell commands, uploaded files, conversation bodies, absolute paths, secrets, credentials, or environment variables. `project_name` is sanitized in Rust and rejected by the cloud schema if it contains either path separator.

Raw provider records remain in their provider-owned locations. ArcMeter does not copy them into SQLite.

## Claude live-limits credential boundary

Claude live limits are opt-in and use Claude Code's existing OAuth access token only for a direct, read-only request to Anthropic's experimental usage endpoint. ArcMeter reads the credential in the trusted Rust layer from the Claude Code credentials file or the macOS Keychain. It does not refresh, modify, delete, copy, or persist Claude credentials.

The Claude access token is never stored in ArcMeter SQLite, synced to Supabase, returned to JavaScript, placed in React state, logged, emitted in diagnostics, included in crash or telemetry payloads, or sent to any third party. The raw credential document and raw usage response are parsed in memory and discarded. Only normalized quota readings may enter SQLite and sync.

## Threat model

| Threat | Control |
| --- | --- |
| Compromised renderer reads arbitrary files | No renderer filesystem permissions; collectors are narrow Rust commands. |
| Collector accidentally uploads content | Parsers construct strongly typed normalized events; sync serializes database columns, never raw JSON records. |
| Claude OAuth token leaks through quota UI or sync | Credential discovery and Anthropic network access remain in Rust; native commands return normalized quota only; quota tables and sync payloads have no credential or raw-response columns. |
| Path disclosure | Only basename sanitization enters `UsageEvent`; cloud constraint rejects separators; diagnostics avoid paths. |
| Duplicate or replayed logs inflate totals | SHA-256 identities, local unique constraints, remote primary keys, deterministic upserts. |
| One account reads another account | RLS on every exposed table, `auth.uid()` ownership policies, composite device ownership FK, verification SQL. |
| Desktop impersonates another user ID | `user_id` defaults to `auth.uid()` and RLS checks ownership; the service-role key is prohibited. |
| Refresh token stolen from application files | Session is stored only in Windows Credential Manager or macOS Keychain; never SQLite, JSON, localStorage, or config. |
| Native command injection | Commands accept bounded structured values; setting names are allow-listed; database values use parameters. |
| Malformed/untrusted logs crash collection | Per-record JSON parsing, safe defaults, bounded diagnostics, unknown-record tolerance. |
| Sync outage blocks dashboard | UI queries local SQLite; sync is background, batched, retried, and non-blocking. |
| Dependency compromise | Exact JavaScript versions, exact direct Rust versions, committed lockfiles, CI checks, secret scanning. |

## Tauri capabilities

The main window receives core window/event permissions and the three autostart operations only. It does not receive wildcard filesystem, shell, process, HTTP, or SQL plugin access. Supabase traffic is executed by trusted Rust. The CSP permits same-origin assets and the configured Supabase HTTPS domain family.

## Credential rules

- Only Supabase's client-safe publishable/anon key may be compiled into the app.
- A service-role key must never appear in source, CI variables used by desktop builds, release artifacts, or `.env`.
- User access/refresh tokens are never returned by native commands.
- Claude Code OAuth credentials are read-only, are never refreshed by ArcMeter, and are used only with Anthropic's usage endpoint after explicit opt-in.
- Sign out attempts remote revocation, then removes the OS credential entry.

## Security review checklist

Any collector or sync change must answer:

1. Can this new field contain user-authored content?
2. Can this value contain a parent directory or absolute path?
3. Is its event identity stable across 100 re-reads?
4. Does the remote schema and RLS still enforce account ownership?
5. Can malformed data cause a panic, unbounded allocation, or scan abort?

Report vulnerabilities privately to the repository owner. Do not include sensitive local telemetry in an issue.
