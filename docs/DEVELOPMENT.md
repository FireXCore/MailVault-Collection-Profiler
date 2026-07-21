# Development

## Repository layout

```text
apps/cli                         headless command-line application
apps/desktop                     React/Vite desktop frontend
apps/desktop/src-tauri           native Tauri application boundary
crates/profiler-core             domain contracts, state and progress
crates/profiler-adapter-mailvault MailVault schema/layout adapter
crates/profiler-engine           snapshot and inventory pipeline
crates/profiler-storage-sqlite   derived profiler database and queries
docs                             versioned user and maintainer documentation
scripts                          deterministic local/release gates
```

## Pinned toolchain

- Rust `1.97.1`, pinned by `rust-toolchain.toml`;
- Rust 2024 edition;
- Node.js `24` recommended and `22+` supported;
- npm `10+`;
- Visual Studio 2026 or Build Tools with **Desktop development with C++**;
- Windows 10/11 SDK and WebView2 Runtime.

Dependencies are reproducible through `Cargo.lock` and `package-lock.json`. Do not use `npm update`
or regenerate lockfiles as part of unrelated work.

## Initial setup

```cmd
git clone https://github.com/FireXCore/mailvault-collection-profiler.git
cd mailvault-collection-profiler
npm ci
```

On Windows, load the MSVC x64 environment before Cargo or Tauri commands:

```cmd
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64
```

## Development commands

Frontend development server:

```cmd
npm run dev
```

Tauri development application:

```cmd
npm run tauri -- dev
```

CLI build:

```cmd
cargo build --release -p mailvault-profiler-cli --locked
```

Desktop installer build:

```cmd
npm run tauri:desktop:bundle
```

## Canonical quality gates

Windows:

```cmd
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
```

Linux/macOS core and frontend gate:

```bash
./scripts/quality.sh
```

The Windows gate is authoritative for release because it compiles every Tauri desktop target and
runs the Windows durable-snapshot regression. It stops on the first non-zero native exit code.

Individual checks:

```cmd
npm run check:release-config
npm run check:docs
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --exclude mailvault-profiler-desktop -- -D warnings
cargo test --workspace --all-features --exclude mailvault-profiler-desktop
cargo test -p profiler-engine --test profile_pipeline -- --nocapture
cargo clippy -p mailvault-profiler-desktop --all-targets --all-features -- -D warnings
npm run type-check
npm run build
```

## Test-data policy

Fixtures must be synthetic. They may reproduce schema structure, fan-out paths, hashes and
relationships, but must not contain real:

- email addresses or domains;
- subjects or message bodies;
- attachment names or bytes;
- message IDs;
- customer or employee names;
- archive paths, usernames or volume labels.

Use deterministic placeholders such as `D:\MailVault-Demo`, `example.invalid` and fixed test hashes.

## Database migrations

Profiler migrations are append-only SQL embedded in the storage crate. Never edit a migration that
has shipped. Add a new numbered migration and tests proving:

- clean creation;
- upgrade from the previous schema;
- idempotent application;
- read-only explorer rejection of unknown/newer schemas.

## Snapshot and filesystem changes

Changes in snapshot, path resolution or physical verification require explicit tests for:

- source database opened read-only;
- byte-for-byte source non-mutation;
- workspace/source overlap rejection;
- containment after canonicalization;
- symlink/reparse-point behavior;
- partial-file publication and cleanup;
- Windows write-capable durable sync;
- missing, unreadable, irregular and size-mismatch outcomes.

## Progress changes

Progress must use measurable backend units and remain monotonic within a stage. Do not synthesize an
overall percentage from unrelated rows, objects and bytes. Add tests whenever a stage unit, total,
checkpoint or terminal transition changes.

## Documentation and screenshots

Behavior changes require corresponding docs. Public screenshots must follow
[`SCREENSHOTS.md`](SCREENSHOTS.md). Run `npm run check:docs` before committing.

## Pull requests

Use a focused branch and the repository template. Include failure/recovery behavior, privacy impact,
compatibility impact and sanitized commands/results. Do not mix lockfile upgrades or large formatting
changes into a behavioral patch.

## Release work

Follow [`RELEASE_PROCESS.md`](RELEASE_PROCESS.md). Tags are immutable and must match the public
application version exactly.
