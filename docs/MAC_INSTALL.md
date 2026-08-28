# Installing ArcMeter on a Mac after Windows V1 review

ArcMeter uses the same source tree on both platforms. There is no Mac fork.

1. Install Xcode Command Line Tools, Node.js 22+, pnpm 11, and Rust stable on the Mac.
2. Check out the reviewed ArcMeter branch/commit.
3. Configure the same Supabase project URL and client-safe publishable key used by Windows. Never copy a service-role key.
4. Run `pnpm install --frozen-lockfile`, `pnpm check`, and `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`.
5. Run `pnpm tauri dev`; verify macOS permission behavior and local Codex/Claude/Grok/Gemini detection.
6. Sign in with the same ArcMeter account and use **Sync now**. Confirm both stable device IDs appear with distinct friendly names and combined totals remain unchanged after a second sync.
7. Run `pnpm tauri build` to produce the `.app`/`.dmg` locally.
8. For distribution beyond private testing, configure an Apple Developer certificate, hardened runtime, notarization credentials, and Tauri updater signing. These release credentials are intentionally not part of V1 source control.
