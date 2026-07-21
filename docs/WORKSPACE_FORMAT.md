# Workspace format — schema 5

MailVault Collection Profiler stores all derived state outside the canonical MailVault archive. A workspace is a local directory owned by the profiler, not by MailVault.

## Layout

```text
D:\MailVault-Profiler-Workspace\
├── .mailvault-profiler.workspace.lock
├── profiler\
│   └── profiler.sqlite3
├── snapshots\
│   └── <run-id>\
│       ├── mailvault.sqlite3
│       └── snapshot-manifest.json
└── backups\
    └── profiler-before-workspace-schema-<version>.sqlite3
```

The lock file contains temporary local process metadata. It is not evidence and must not be published. The profiler database contains run metadata, derived inventory, findings, checkpoints and finding-review state.

## Compatibility inspection

Workspace inspection opens `profiler/profiler.sqlite3` read-only and checks:

- SQLite application ID;
- `PRAGMA user_version`;
- database integrity;
- required metadata and review tables;
- migration state;
- registered source archive roots;
- source/workspace path overlap;
- active operating-system file lock;
- review history integrity after open.

A missing profiler database is reported as `WORKSPACE_DATABASE_MISSING`; it is never created by an open-existing operation. A workspace newer than the application fails closed with `WORKSPACE_SCHEMA_NEWER_THAN_APPLICATION`.

## Schema migration

Schema 5 adds workspace metadata and append-only finding review storage. Migration is explicit at the application boundary. Before any schema change, the profiler uses the SQLite Backup API to create:

```text
backups/profiler-before-workspace-schema-<old-version>.sqlite3
```

The migration runs transactionally. A failure retains the backup and writes a local migration-failure marker. A workspace containing that marker opens neither read-write nor silently downgraded.

## Locking and access modes

The operating-system lock, not the mere presence of a lock file, determines ownership.

| Access mode | Browse | Review writes | Migration |
|---|---:|---:|---:|
| `read_write` | yes | yes | yes |
| `read_only_locked` | yes | no | no |
| `read_only_compatibility` | yes | no | no |

Only one process may hold the review-writer lock. A second process may browse the workspace but cannot change review state.

## Review storage

`finding_review_events` is append-only. Update and delete triggers reject historical mutation. `finding_review_state` is a transactionally maintained projection of the latest event for each finding.

Each event stores a SHA-256 hash over stable canonical fields and links to the previous event hash. Opening a workspace validates sequence continuity, identity, action semantics, prior status, prior hash, recomputed hash and projection consistency. Integrity failure disables review writes while preserving read-only browsing.

## Evidence boundary

Original run evidence and source snapshots are not rewritten by findings review. Review decisions live only in the profiler database. Sanitized export creates a new file outside the source archive and does not include full paths, filenames, email addresses or review notes.
