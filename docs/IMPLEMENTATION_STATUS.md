# Implementation status — 0.1.0-alpha.3

**Validation date:** 2026-07-20
**Release state:** implementation complete for the alpha.3 scope; Windows quality gate and controlled real-workspace acceptance passed.

## Runtime status

```text
read-only preflight and physical inventory
→ durable profiler workspace
→ workspace inspection and explicit migration
→ run catalog after process restart
→ reopened inventory/findings/object detail
→ append-only review events and projection
→ review hash-chain verification
→ sanitized aggregate export
→ Windows and real-archive validation
```

Recorded alpha.3 status:

- Rust format, Clippy and tests: green;
- TypeScript and Vite production build: green;
- native Tauri desktop Clippy/compile gate: green;
- real MailVault schema-v3 profile: completed;
- source/snapshot aggregate comparison: matched;
- workspace reopen after full restart: passed;
- Windows single-writer/read-only fallback: passed;
- review persistence and append-only integrity: passed;
- sanitized JSON/CSV acceptance: passed;
- source mutation: none.

See [Validation evidence](VALIDATION_0.1.0-alpha.3.md).

## Completed implementation

### Workspace reopening

- Existing profiler databases open with explicit read-only/read-write flags.
- Missing databases are not created.
- Schema 4 migrates to schema 5 only after confirmation and a retained SQLite backup.
- Newer, corrupted, overlapping or partially migrated workspaces fail closed.
- One operating-system lock owns review writes; concurrent instances fall back to read-only.
- Completed, failed and interrupted run records appear in the run catalog after process restart.
- Completed runs reopen without profiling the MailVault source again.

### Findings review

- statuses: acknowledged, expected, needs investigation and resolved externally;
- status clearing and notes append new events;
- historical update/delete is blocked by SQLite triggers;
- event sequence, prior status, prior hash, event hash and projection are validated on open;
- notes are normalized, bounded and excluded from logs and sanitized exports;
- informational evidence is separated from warnings and errors;
- integrity failure disables writes while retaining read-only browsing.

### Inventory and evidence

- MailVault schema-v3 read-only preflight;
- consistent SQLite Online Backup snapshot;
- streaming metadata and physical object inventory;
- SHA-256 content identity and occurrence reconciliation;
- filename history and duplicate-relationship evidence;
- bounded file-stat inspection;
- missing, unreadable, invalid-locator, non-regular and size-mismatch findings;
- cursor-paginated inventory and finding search;
- content-object detail and related message evidence;
- sanitized JSON summary and CSV findings export.

### User surfaces

- desktop start screen for a new profile or existing workspace;
- compatibility, migration and lock-state result;
- run catalog;
- reopened inventory and findings;
- content-object and finding detail drawers;
- review controls and append-only history;
- sanitized export;
- equivalent CLI inspection, listing, review and export commands.

## Explicitly incomplete

- resuming interrupted profiling;
- user-facing pause/cancel controls;
- archive repair, deletion, renaming or deduplication;
- full payload fixity pass;
- exact format identification, JHOVE and container expansion;
- OCR or semantic classification;
- cloud synchronization, authentication and multi-user review;
- automatic updates;
- public Windows code signing.

## Release classification

`0.1.0-alpha.3` is a development pre-release. Runtime Green means the implemented alpha scope passed
its declared gates; it does not claim the deferred capabilities listed above.

See [Workspace format](WORKSPACE_FORMAT.md), [Findings review](FINDINGS_REVIEW.md),
[Validation evidence](VALIDATION_0.1.0-alpha.3.md) and [Roadmap](ROADMAP.md).
