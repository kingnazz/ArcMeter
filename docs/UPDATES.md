# ArcMeter signed updates

ArcMeter checks for signed updates in the background and exposes a manual check in **Settings → Application → Signed updates**. A newer release is never installed silently. The user must choose **Install**, and the updater verifies the downloaded bundle against the public key compiled into ArcMeter before it replaces the installed version.

The stable update endpoint is:

```text
https://github.com/kingnazz/ArcMeter/releases/latest/download/latest.json
```

The initial `0.2.0` installation is the bootstrap release: an older build without the updater still needs one manual installer update. After `0.2.0` is installed, later versions can arrive through the in-app flow.

Version `0.2.1` is the first updater-delivered maintenance release. It keeps safely priced API-equivalent value visible as a lower-bound subtotal when the selected period also contains events whose model pricing cannot be verified.

## Signing identity

The updater public key is committed in `apps/desktop/src-tauri/tauri.conf.json`. Its matching private key must never be committed. The Windows setup machine currently keeps it at:

```text
%USERPROFILE%\.tauri\arcmeter-updater.key
```

Its generated password is stored as the generic Windows Credential Manager entry `ArcMeter.UpdaterSigning`. Back up both the private key and its password in the organization's secure credential vault before the first public release. Losing either one prevents all installed copies from accepting future updates.

Updater signatures are separate from Windows Authenticode signing and Apple bundle code signing. Tauri updater signatures protect the update channel but do not establish an Apple-trusted publisher identity. ArcMeter's self-built/personal-use macOS strategy applies a complete ad-hoc bundle signature and intentionally does not use Developer ID signing or Apple notarization, so Gatekeeper approval is still expected.

## GitHub release secrets

Configure these repository-scoped Actions secrets only in `kingnazz/ArcMeter`:

- `TAURI_SIGNING_PRIVATE_KEY`: the full contents of `arcmeter-updater.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the updater key password
- `ARCMETER_SUPABASE_URL`: the ArcMeter Supabase project URL
- `ARCMETER_SUPABASE_ANON_KEY`: the active client-safe publishable/anon key

Do not configure Apple Developer ID or notarization secrets for the personal-use macOS build. Windows platform signing remains a separate decision. Do not put a Supabase service-role key in a desktop build or in these client configuration values.

## Publishing a version

1. Update the version consistently in the root and desktop `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.
2. Merge a fully green CI revision.
3. Run **Release ArcMeter** manually in GitHub Actions. The workflow creates the version tag and private draft; publishing the draft must not trigger a second build.
4. The workflow merges `src-tauri/tauri.release.conf.json`, builds Windows x64 NSIS plus ad-hoc signed macOS Apple-silicon and Intel app/DMG bundles, creates updater archives and `.sig` files, and writes `latest.json`.
5. Test the Windows installer and both macOS installers from the draft release on clean systems. For each Mac DMG, verify its SHA-256 and run `scripts/verify-macos-dmg.sh` with the expected architecture before launch.
6. Publish the draft. Only then does GitHub's `releases/latest` endpoint expose the update to installed apps.
7. From the previous ArcMeter version, check that the update is offered, install it, and verify the new version and local database remain intact.

Never replace an existing release asset or reuse a version number. Publish a higher semantic version so the updater can make a deterministic decision.
