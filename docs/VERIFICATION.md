# Phase 1 verification evidence

Verification date: 2026-08-28 on Windows.

## Automated gates

- Frontend strict typecheck: passed.
- Frontend ESLint: passed.
- Vitest: 7 passed across 2 files.
- Rust formatting: passed.
- Rust Clippy with warnings denied: passed.
- Rust core suite: 22 passed; the two environment-specific smoke tests are ignored by default.
- Production React build: passed (`230.19 kB` JavaScript and `27.23 kB` CSS before gzip).
- Tauri production build and NSIS packaging: passed.

The private-test installer is `artifacts/windows/ArcMeter_0.1.0_x64-setup.exe` (5,677,287 bytes, SHA-256 `26CC814C8700A607B049BE6610B3ABFC442CC17C84984DBC9F247109D2087817`). It is intentionally unsigned; code signing was explicitly outside Phase 1.

The final dedicated read-only Codex snapshot test found 1,926 measured events across 41 files and 220,936,594 tokens. Re-reading the identical snapshot inserted zero records. An earlier packaged-app launch against the live profile produced a SQLite ledger with 1,875 measured events and 215,215,638 tokens. After a process restart it retained one device identity, passed `PRAGMA integrity_check`, resumed its 41 collector checkpoints, and grew only for newly written Codex usage (1,888 events and 216,188,675 tokens at the second check).

## Desktop and visual checks

- The packaged Tauri process launched, remained responsive, and relaunched successfully.
- SQLite schema version 1, 26 pricing rules, one stable device, and the live measured ledger were confirmed from the launched app database.
- The Overview, Activity, Settings, onboarding, offline, unavailable-pricing, collector-error, and narrow-window states were inspected in the local renderer. The visual pass fixed a status-label concatenation defect and a stale preview timestamp.
- Browser-preview screenshots intentionally show honest local/offline empty states; they contain no production demo analytics.
- Direct Windows window/tray automation was unavailable because the desktop-control approval timed out. The tray and close-to-tray paths compile in the production binary, the process remains resident, tray Sync Now invokes native sync directly, and dynamic tray totals are updated after every background scan. A human click-through remains appropriate before wider distribution.

## External validation boundary

No Supabase project credentials were provided. Authentication and sync are implemented, but a live sign-in and two-machine exchange were not performed. Apply the migration to a disposable/local Supabase instance, run `supabase/tests/rls_verification.sql`, configure the client-safe project values, and complete the Windows-to-Mac sync checklist in `docs/MAC_INSTALL.md`.
