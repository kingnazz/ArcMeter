# ArcMeter

ArcMeter is a local-first desktop application that measures AI coding-tool usage across a user's computers. It reads authoritative local telemetry from supported CLIs, normalizes token metadata into SQLite, and can synchronize that metadata through a user-owned Supabase account. Prompts, responses, source code, commands, uploaded files, secrets, environment variables, and absolute paths are outside the sync boundary.

V1 supports native collectors for OpenAI Codex, Claude Code, Grok Build, and Gemini CLI. The product UI contains only Overview, Activity, Insights, and Settings.

## Stack

- Tauri `2.11.5` with Tauri CLI `2.11.4`
- Rust `1.98.0` (the crate declares an MSRV of `1.88`)
- React `19.2.8`
- TypeScript `6.0.3` in strict mode (TypeScript 7 is not yet supported by the current typed ESLint parser)
- Vite `8.2.2`
- SQLite through bundled `rusqlite 0.40.2`
- Supabase Auth, Postgres, and Row Level Security
- Native Windows Credential Manager / macOS Keychain through `keyring 4.1.6`

All JavaScript package versions are exact-pinned and both `pnpm-lock.yaml` and `Cargo.lock` are committed.

## Repository layout

```text
apps/desktop/                 React renderer and Tauri application
  src/                        Four product views and design system
  src-tauri/                  Trusted Rust layer
    migrations/               Local SQLite schema
    src/collectors/           Native read-only collectors
    tests/fixtures/           Sanitized parser fixtures
supabase/
  migrations/                 Cloud schema, triggers, indexes, and RLS
  tests/                      Account-isolation verification SQL
docs/                         Operational and platform notes
.github/workflows/            Cross-platform validation
```

The more detailed component and data flow is in [ARCHITECTURE.md](ARCHITECTURE.md). The trust boundary and threat model are in [SECURITY.md](SECURITY.md). Pricing provenance is recorded in [docs/PRICING.md](docs/PRICING.md), and the Phase 1 evidence is in [docs/VERIFICATION.md](docs/VERIFICATION.md).

## Development

Prerequisites on Windows:

1. Node.js 22 or newer and pnpm 11.
2. Rust stable.
3. Microsoft C++ Build Tools with the Desktop development with C++ workload and a Windows SDK.
4. WebView2 (present on supported Windows 10/11 installations).

```powershell
pnpm install --frozen-lockfile
pnpm check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm tauri dev
```

Create a Windows installer with:

```powershell
pnpm tauri build
```

The renderer has a browser-safe empty state for visual development, but it never supplies fake production analytics. Real data is available through Tauri commands only.

## Supabase setup

1. Create a Supabase project.
2. Apply `supabase/migrations/202608280001_arcmeter_v1.sql` with the Supabase CLI or dashboard migration workflow.
3. Run `supabase/tests/rls_verification.sql` against a disposable/local database.
4. Copy `.env.example` to `.env` and set only the project URL and client-safe publishable/anon key.
5. Never use a service-role key in the desktop environment.
6. Export `ARCMETER_SUPABASE_URL` and `ARCMETER_SUPABASE_ANON_KEY` in the build shell, then rebuild the desktop app. These client-safe values are compiled into the trusted native layer for installed builds; runtime environment overrides remain available for development.

Email/password authentication is used for V1 to avoid fragile native magic-link callbacks. The renderer sends credentials to one narrow native command; tokens are never returned to JavaScript. The refresh token is persisted only in the operating system credential store.

## Collection behavior

Collectors discover standard cross-platform locations and supported environment overrides. Files are read-only. Unchanged files are skipped using local collector state; a changed file is safely replayed so parser context is reconstructed, while deterministic database identities prevent duplicates.

Codex V1 uses the observed native JSONL structure:

- session identity from `session_meta.payload.session_id` or `id`
- event identity from the top-level `ordinal`
- model and sanitized project basename from `turn_context`
- authoritative per-call counters from `event_msg/token_count.payload.info.last_token_usage`

The fallback fingerprint is SHA-256 over the smallest reliable identity tuple. A collision is handled as the same event and logged through collector diagnostics; no totals are incremented.

Collection runs immediately at startup and every 60 seconds while ArcMeter remains in the tray. Cloud sync runs independently in the background, uses paginated metadata pulls and batched uploads, and advances its cursor to the pre-request watermark so concurrent remote writes cannot be skipped.

## Privacy guarantee

Cloud rows are intentionally incapable of carrying conversation content. The schema accepts normalized provider/model/project/device/token metadata only, and `project_name` rejects path separators. Local collector diagnostics avoid source paths. See [SECURITY.md](SECURITY.md) before changing a collector or sync payload.
