# Contributing

Work on a feature branch and leave pull requests unmerged until review. Never commit credentials or raw local telemetry.

Before opening a pull request:

```powershell
pnpm install --frozen-lockfile
pnpm typecheck
pnpm lint
pnpm test
pnpm build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
git diff --check
```

Collector changes require sanitized fixtures for a valid event, malformed JSON, unknown records, duplicate identity, missing optional counters, missing required identity, partial records, empty input, and re-ingestion. Do not paste real prompt or response content into fixtures or test output.

New renderer capabilities and native commands require a security review. Prefer structured, allow-listed command parameters and parameterized SQL. Do not add broad filesystem or shell permissions.
