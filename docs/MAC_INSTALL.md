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
- Claude Code CLI: `~/.claude/projects` (does not include Claude Desktop)
- Grok Build: `~/.grok`
- Gemini CLI: `~/.gemini`

`Not detected` means the standard root or supported JSON/JSONL telemetry is absent. A warning means a source exists but at least one record did not match the V1 parser. A permission error is distinct and should name the affected collector without exposing a full source path. ArcMeter only needs read access to these local telemetry directories; do not grant broader macOS permissions unless the collector reports an actual denial.

## Validate optional activity tracking

Claude Desktop and the consumer Grok web app do not expose authoritative local token counts. ArcMeter records their opt-in activity separately and never adds it to measured-token or API-equivalent-value totals.

1. In **Settings → Activity tracking**, enable **Claude Desktop active minutes**.
2. Bring Claude Desktop to the foreground for at least one minute, return to ArcMeter, and confirm an `activity only` row appears with `1 min` and zero tokens. ArcMeter reads the frontmost app bundle identifier, not window titles or conversation content.
3. To track grok.com, enable **Grok web active minutes** and load `extensions/arcmeter-browser-activity` as an unpacked extension in a Chrome-compatible browser.
4. Paste ArcMeter's pairing token into the extension Options page and choose **Save and test**.
5. Keep a grok.com tab focused for a minute, then confirm a `Grok web` activity-only row appears. The extension checks the domain locally and does not send the URL, title, prompt, response, or token count.

## Build and verify the personal-use artifacts

Create the private-test build with:

```sh
pnpm tauri build --target aarch64-apple-darwin --bundles dmg
pnpm tauri build --target x86_64-apple-darwin --bundles dmg
```

The default outputs are:

- `apps/desktop/src-tauri/target/<target>/release/bundle/macos/ArcMeter.app`
- `apps/desktop/src-tauri/target/<target>/release/bundle/dmg/ArcMeter_<version>_<arch>.dmg`

ArcMeter's macOS bundles use a complete ad-hoc signature for personal use. They are not signed with an Apple Developer ID and are not notarized by Apple. Ad-hoc signing seals the app resources but does not establish a trusted publisher identity, so a normally configured Mac will still require Gatekeeper approval on first launch.

Verify the SHA-256 supplied by the builder before opening a trusted artifact, then verify the app inside the final DMG. Choose the expected architecture for the artifact being checked:

```sh
shasum -a 256 apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/*.dmg
shasum -a 256 apps/desktop/src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/*.dmg
scripts/verify-macos-dmg.sh path/to/ArcMeter_<version>_aarch64.dmg arm64
scripts/verify-macos-dmg.sh path/to/ArcMeter_<version>_x64.dmg x86_64
```

The verifier runs `hdiutil verify`, mounts the DMG read-only, requires `codesign --verify --deep --strict --verbose=4` to pass on its `ArcMeter.app`, checks the enclosed executable with `file` and `lipo`, and prints the DMG SHA-256. A linker-generated signature on only the executable is insufficient.

Tauri updater signing is independent of Apple bundle signing. `TAURI_SIGNING_PRIVATE_KEY` signs updater metadata and must not be presented as an Apple signing identity.

## First launch of a trusted build

After comparing the DMG SHA-256 with the value supplied by the builder, drag ArcMeter to **Applications**. Because the app has no Developer ID signature or Apple notarization, open it using one of Apple's per-app approval paths:

1. In Finder, Control-click or right-click **ArcMeter.app**, choose **Open**, then confirm **Open**.
2. If macOS blocks the first attempt, open **System Settings → Privacy & Security**, find the ArcMeter notice, choose **Open Anyway**, and confirm.

Do not disable Gatekeeper system-wide. If both approval paths are unavailable, and only after independently verifying that this exact artifact is trusted, remove quarantine from ArcMeter alone as a fallback:

```sh
xattr -dr com.apple.quarantine /Applications/ArcMeter.app
```

Removing quarantine is only a local launch workaround; it neither repairs nor replaces the build-time resource seal.

## Validate the Windows-to-Mac sync

1. In **Settings → Account**, sign in with the exact ArcMeter Auth account used on Windows.
2. Use **Sync now** once. Confirm Windows and Mac appear as distinct device identities with distinct friendly names.
3. Record the combined usage-event count and measured-token total.
4. Quiesce supported CLI writers before the idempotency pass. The Settings **Sync now** action performs a local scan before cloud sync, so an active CLI can legitimately add events between snapshots. The tray **Sync Now** action invokes cloud sync directly and is preferable when validating a live CLI session.
5. Use **Sync now** a second time. Confirm it uploads and downloads zero usage events, that the row count still equals the distinct event-ID and natural-key counts, and that the combined event count and token total are unchanged.

For activity-only rows, also confirm the active-minute count remains unchanged after the second sync. The self-built/personal-use distribution strategy intentionally stops at complete ad-hoc bundle signing. Developer ID signing and Apple notarization are not configured.
