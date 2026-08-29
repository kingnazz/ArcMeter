# Phase 1 verification evidence

Verification date: 2026-08-28 on Windows.

## Automated gates

- Frontend strict typecheck: passed.
- Frontend ESLint: passed.
- Vitest: 7 passed across 2 files.
- Rust formatting: passed.
- Rust Clippy with warnings denied: passed.
- Rust core suite: 22 passed; the two environment-specific smoke tests are ignored by default.
- Production React build: passed (`230.37 kB` JavaScript and `27.05 kB` CSS before gzip).
- Tauri production build and NSIS packaging: passed.

The private-test installer is `artifacts/windows/ArcMeter_0.1.0_x64-setup.exe` (5,761,942 bytes, SHA-256 `B6628136B183DC4F40508D7D047AAAF7767AFA765AD059128AFF54277C35FFA7`). It is intentionally unsigned; code signing was explicitly outside Phase 1. The GNU fallback build passed an explicit `x86_64-pc-windows-gnu` target, installed `WebView2Loader.dll` beside the executable, used the Windows GUI subsystem, and passed a clean uninstall/reinstall launch test.

The final dedicated read-only Codex snapshot test found 1,926 measured events across 41 files and 220,936,594 tokens. Re-reading the identical snapshot inserted zero records. An earlier packaged-app launch against the live profile produced a SQLite ledger with 1,875 measured events and 215,215,638 tokens. After a process restart it retained one device identity, passed `PRAGMA integrity_check`, resumed its 41 collector checkpoints, and grew only for newly written Codex usage (1,888 events and 216,188,675 tokens at the second check).

## Desktop and visual checks

- The packaged Tauri process launched, remained responsive, and relaunched successfully.
- SQLite schema version 1, 26 pricing rules, one stable device, and the live measured ledger were confirmed from the launched app database.
- The Overview, Activity, Settings, onboarding, offline, unavailable-pricing, collector-error, and narrow-window states were inspected in the local renderer. The visual pass fixed a status-label concatenation defect and a stale preview timestamp.
- Browser-preview screenshots intentionally show honest local/offline empty states; they contain no production demo analytics.
- Direct Windows window/tray automation was unavailable because the desktop-control approval timed out. The tray and close-to-tray paths compile in the production binary, the process remains resident, tray Sync Now invokes native sync directly, and dynamic tray totals are updated after every background scan. A human click-through remains appropriate before wider distribution.

## External validation boundary

A dedicated Supabase project now has both repository migrations applied. The transaction-wrapped RLS verification passed without leaving test rows, all five public tables have RLS enabled, the security advisor returned no findings, and the active client-safe publishable configuration was compiled into the Windows installer. A live sign-in and two-machine exchange have not yet been performed; complete the Windows-to-Mac sync checklist in `docs/MAC_INSTALL.md` using the same ArcMeter Auth account on both machines.
