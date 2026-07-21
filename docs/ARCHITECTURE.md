# Architecture

```text
MailVault archive (read-only)
        â”‚
        â–¼
Capability preflight
        â”‚
        â–¼
SQLite Online Backup snapshot
        â”‚
        â–¼
Streaming metadata inventory
        â”‚
        â–¼
SHA-256 content reconciliation
        â”‚
        â–¼
Bounded physical file-stat workers
        â”‚
        â–¼
Single-writer profiler SQLite
        â”‚
        â”œâ”€â”€ read-only CLI/report queries
        â””â”€â”€ state-bound Tauri desktop explorer
```

## Crates

- `profiler-core`: domain contracts, errors, run state, explorer types and progress events;
- `profiler-storage-sqlite`: disposable profiler database, migrations, checkpoints and explorer queries;
- `profiler-adapter-mailvault`: MailVault v2.0.6 schema/layout adapter and physical object resolver;
- `profiler-engine`: ordered snapshot, inventory, reconciliation and file-stat pipeline;
- `mailvault-profiler-cli`: headless preflight, snapshot and profile commands;
- `mailvault-profiler-desktop`: Tauri command boundary and React application.

## Source snapshot

The source database is never copied with a raw filesystem copy. The adapter opens SQLite using
read-only flags and uses the SQLite Online Backup API to create a consistent destination database.
The destination is written only in the profiler workspace. Source metrics are compared before and
after backup; a changing source invalidates the unpublished snapshot.

## Physical inventory

Messages, MIME parts, blob rows and relationships are streamed in stable source order. Attachment
occurrences remain separate from SHA-256 content objects, so one binary may retain many filenames,
messages and thread contexts without duplicate payload processing.

The file-stat stage works on unique content objects only. It uses bounded batches and a dedicated
Rayon pool, validates the exact MailVault fan-out locator, rejects paths outside the archive root,
and persists each result with the matching durable checkpoint transaction.

## Storage and explorer boundary

The profiler database is disposable and rebuildable. One controlled writer is used during a run.
The desktop explorer opens a separate SQLite connection in read-only/query-only mode and verifies
the profiler application ID and exact schema version before executing queries.

Desktop explorer commands do not accept an arbitrary database path from the webview. A completed
profile activates a server-side session containing the profiler database, collection and run IDs.

## Compatibility

Compatibility is capability-based. The adapter checks required tables and columns, then records
recommended-index warnings separately. A cosmetic package version string is not used as an archive
contract.

The current adapter accepts MailVault schema version 3 only. A newer source schema fails closed
until the adapter is reviewed.

## Progress

Progress events contain exact stage units:

- snapshot: SQLite pages;
- metadata inventory: source rows;
- reconciliation and file stat: unique objects;
- physical verification: expected bytes.

The UI uses byte totals where available, does not fabricate an overall pipeline percentage and
leaves ETA unavailable until the backend has enough measurable work. Warning, error, worker,
queue and checkpoint counters come directly from the current stage event.

## Workspace reopen and review boundary

`profiler-engine::workspace` owns canonical path validation, operating-system locking, open modes and atomic export publication. `profiler-storage-sqlite` owns schema migration, backup, run catalog, review events/projection and integrity verification. Tauri and CLI call typed engine operations and do not execute SQL. Review state is derived metadata and never enters MailVault or original evidence files.
