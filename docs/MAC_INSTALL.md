# Installing ArcMeter on a Mac after Windows V1 review

ArcMeter uses the same source tree and Supabase project on both platforms. There is no Mac fork.

## Prerequisites

Install Xcode Command Line Tools, Node.js 22 or newer, pnpm 11, and Rust stable. Confirm the active shell is using the expected toolchain before installing dependencies:

```sh
node --version
pnpm --version
rustc --version
```

If more than one Node installation is present, put the Node 22 `bin` directory first on `PATH` for every install, check, development, and build command. Do not replace unrelated system symlinks merely to build ArcMeter.

## Configure the client-safe Supabase values

Use the same Supabase project URL and active `sb_publishable_...` key used for the reviewed Windows build. Never use a service-role or secret key. Keep both native and renderer names aligned in the same shell:

```sh
export ARCMETER_SUPABASE_URL="https://your-project.supabase.co"
export ARCMETER_SUPABASE_ANON_KEY="<active-client-safe-publishable-key>"
export VITE_SUPABASE_URL="$ARCMETER_SUPABASE_URL"
export VITE_SUPABASE_ANON_KEY="$ARCMETER_SUPABASE_ANON_KEY"
```

These values are compiled into an installed build, so export them before both `tauri dev` and `tauri build`. Do not commit `.env`, `.env.local`, credentials, or build logs containing credentials.

## Install and validate

Check out the reviewed ArcMeter branch/commit, then run:

```sh
pnpm install --frozen-lockfile
pnpm check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm tauri dev
```

In **Settings → AI sources**, run **Scan now** and inspect every collector. The standard macOS roots are:

- Codex: `~/.codex/sessions` and `~/.codex/archived_sessions`
- Claude Code: `~/.claude/projects`
- Grok Build: `~/.grok`
- Gemini CLI: `~/.gemini`

`Not detected` means the standard root or supported JSON/JSONL telemetry is absent. A warning means a source exists but at least one record did not match the V1 parser. A permission error is distinct and should name the affected collector without exposing a full source path. ArcMeter only needs read access to these local telemetry directories; do not grant broader macOS permissions unless the collector reports an actual denial.

## Build and verify the unsigned artifacts

Create the private-test build with:

```sh
pnpm tauri build
```

The default outputs are:

- `apps/desktop/src-tauri/target/release/bundle/macos/ArcMeter.app`
- `apps/desktop/src-tauri/target/release/bundle/dmg/ArcMeter_<version>_<arch>.dmg`

Record a SHA-256 for the DMG and the app executable. For a reproducible app-bundle content digest, hash the sorted file-hash manifest:

```sh
shasum -a 256 apps/desktop/src-tauri/target/release/bundle/dmg/*.dmg
shasum -a 256 apps/desktop/src-tauri/target/release/bundle/macos/ArcMeter.app/Contents/MacOS/arcmeter
find apps/desktop/src-tauri/target/release/bundle/macos/ArcMeter.app -type f -print0 \
  | LC_ALL=C sort -z \
  | xargs -0 shasum -a 256 \
  | shasum -a 256
```

The private-test build may have only a linker-generated ad-hoc signature. Do not configure a Developer ID certificate, hardened runtime, notarization, or updater signing until the unsigned build and two-device sync tests pass.

## Validate the Windows-to-Mac sync

1. In **Settings → Account**, sign in with the exact ArcMeter Auth account used on Windows.
2. Use **Sync now** once. Confirm Windows and Mac appear as distinct device identities with distinct friendly names.
3. Record the combined usage-event count and measured-token total.
4. Quiesce supported CLI writers before the idempotency pass. The Settings **Sync now** action performs a local scan before cloud sync, so an active CLI can legitimately add events between snapshots. The tray **Sync Now** action invokes cloud sync directly and is preferable when validating a live CLI session.
5. Use **Sync now** a second time. Confirm it uploads and downloads zero usage events, that the row count still equals the distinct event-ID and natural-key counts, and that the combined event count and token total are unchanged.

Only after this checklist passes should release signing and notarization be configured for distribution beyond private testing.
