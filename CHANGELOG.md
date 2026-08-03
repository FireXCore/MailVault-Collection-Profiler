# Changelog

All notable changes to MailVault Collection Profiler are documented here.

## 0.1.0-alpha.6 — 2026-07-28

- Unified initial, incremental and rebuild profiling into one crash-resumable pipeline.
- Added full consistent metadata reconciliation on every source revision while reusing successful SHA-256 file-stat and exact-format results.
- Added automatic Siegfried/PRONOM identification before a physical run can succeed.
- Added retryable unavailable/tool-error states, stable collection identity aliases and bounded snapshot retention.
- Added Windows gates for initial/no-change, real delta, force-kill resume, 100-cycle soak and independent rebuild equivalence.
- Workspace schema advanced to 7; public application version is `0.1.0-alpha.6` and MSI version is `0.1.0.6`.

## 0.1.0-alpha.2 — 2026-07-19

### Added

- Physical inventory explorer with SHA-256 keyset pagination.
- Search across filename variants, SHA-256, MIME, message subject and sender domain.
- Availability, size-state and finding-code filters.
- Content-object detail with filename history, email occurrences and active-run findings.
- Findings explorer with code and severity filters.
- Read-only/query-only profiler database connections for desktop exploration.
- Explorer-specific SQLite schema migration and indexes.
- Collection-scoped content-object detail boundary.
- End-to-end tests for pagination, filter search, detail, findings and source immutability.
- GitHub-ready public documentation suite, sanitized screenshot set and social preview.
- Structured issue forms, pull-request template, CodeQL, Dependency Review and Dependabot.
- Draft Windows release workflow with NSIS/MSI bundles, SHA-256 manifest and provenance attestation.
- Documentation/privacy gate for local links, required assets, private paths and private registries.

### Changed

- Persisted numeric and state values now fail closed when invalid instead of silently defaulting.
- Inventory and findings filters now distinguish draft values from applied query state.
- Workspace package version advanced to `0.1.0-alpha.2`.

### Fixed in corrected source package

- Open completed SQLite snapshot files with a write-capable handle before `sync_all`, preventing
  Windows `ERROR_ACCESS_DENIED` / `LNK`-independent pipeline failures during durable publication.
- Make `scripts/quality.ps1` fail immediately when any native Cargo or npm command exits non-zero.
- Invoke `npm.cmd` directly on Windows so the npm PowerShell shim cannot fail under StrictMode.
- Replace build-environment-only npm tarball URLs with the public npm registry and pin the project
  registry in `.npmrc`.
- Run the physical profile pipeline integration test on Windows CI so the durable-sync regression
  is covered on the platform where it occurred.
- Remove the invalid `Debug` derive from the Tauri progress-channel adapter, restoring native
  desktop compilation with Tauri 2.11.
- Compile and lint all native desktop targets in the Windows PowerShell quality gate.
- Preserve the SemVer application label `0.1.0-alpha.2` while mapping the WiX/MSI version to
  numeric `0.1.0.2`, and validate that mapping in every quality run.
- Configure CodeQL Rust analysis with build mode `none`; the Rust extractor does not support
  `manual` build mode.

### Known limitations

- Existing workspaces cannot yet be reopened after desktop restart.
- Pause/resume controls are not exposed in the desktop UI.
- Exact format identification, fixity passes and container expansion are not implemented.
- The native Tauri crate is validated in Windows CI; Linux desktop compilation requires the normal
  Tauri system development packages.

## 0.1.0-alpha.1 — 2026-07-18

### Added

- Rust/Tauri workspace foundation.
- MailVault schema/layout preflight.
- Consistent SQLite source snapshots.
- Streaming physical inventory and SHA-256 reconciliation.
- Bounded file-stat stage with exact progress and findings.
