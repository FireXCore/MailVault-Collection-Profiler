# Changelog

All notable changes to MailVault Collection Profiler are documented here.

The project follows Semantic Versioning once the public API reaches a stable release. Alpha builds
may change local profiler schemas and command contracts through explicit migrations.

## 0.1.0-alpha.4 — 2026-07-21

- Added exact format identification with pinned Siegfried `1.11.6` and PRONOM `v124`.
- Added versioned PUID assertions, all-match retention, bounded batches, resume checkpoints and format UI/CLI.
- Added verified sidecar acquisition, Tauri resource bundling and third-party notices.
- Added migration `0006_exact_format_identification.sql`.
- Updated English/Persian documentation and sanitized release screenshots.
- Fixed Windows JSON probing and real-batch execution by resolving the verified signature through
  `-home` and passing only `default.sig`, avoiding Siegfried 1.11.6's unescaped header path.
- Added Windows regression coverage for the sidecar command contract.


### Added

- Existing workspace inspection, compatibility reporting and explicit open modes.
- Workspace schema 5 with metadata, append-only review events and review projections.
- Pre-migration SQLite backup and retained migration-failure marker.
- Run catalog and completed-run reopening after a full process restart.
- Single review writer with read-only fallback when another process holds the workspace lock.
- Finding review statuses, required-note policy and append-only SHA-256 event chains.
- Review history integrity validation and write-disable fallback after detected tampering.
- Findings categories for requires-attention, informational evidence, reviewed and all findings.
- Sanitized JSON summary and CSV finding exports using non-reversible short tokens.
- CLI commands for workspace inspection, run listing, finding review and sanitized export.
- Desktop start journey for profiling a new archive or opening an existing workspace.
- Integration tests for reopen persistence, lock contention, review history and sanitized exports.

### Changed

- Public application version advanced to `0.1.0-alpha.4`; WiX/MSI version maps to `0.1.0.4`.
- Informational filename/content relationships are separated from warnings requiring attention.
- Quality, CI and release commands use locked Cargo resolution and validate alpha.3 release notes.

### Fixed during release hardening

- Centralized signed-to-unsigned SQLite count conversion for explorer, review and workspace queries.
- Split review-event persistence into transactional preparation, insert and projection helpers.
- Replaced manual guarded division with checked review-completion arithmetic.
- Replaced the oversized CLI dispatcher with command-specific handlers.
- Split workspace inspection/opening into focused compatibility, access and integrity helpers.
- Replaced the flat eight-argument Tauri findings command with a typed request object.
- Kept the desktop findings request contract synchronized with the nested Tauri payload.
- Made the documentation screenshot gate recurse through versioned screenshot directories and require the public alpha.3 images explicitly.

### Validation

- Passed the complete Windows quality gate, including strict Clippy, 36 Rust tests, TypeScript, Vite, native desktop compilation and npm audit with 0 vulnerabilities.
- Completed the full private exact-format run with 13,684/13,684 objects, 13,636 identified, 46 unknown, 0 ambiguous, 0 tool errors, 51 extension mismatches and 64 distinct PUIDs.
- Verified schema 5-to-6 workspace migration, complete byte coverage and unchanged canonical MailVault SHA-256.

### Security and privacy

- Review data stays outside MailVault and original runtime evidence.
- Review notes are excluded from application logs and sanitized exports.
- Existing profiler databases are never created implicitly during reopen.
- Workspace/source overlap is checked after canonicalization.

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
