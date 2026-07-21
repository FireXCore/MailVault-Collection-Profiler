# Workspace format

## Layout

```text
<workspace>/
  profiler/profiler.sqlite3
  snapshots/<run-id>/mailvault.sqlite3
  .mailvault-profiler.lock
  .mailvault-profiler.format.lock
  format-staging/<format-run-id>/...   # temporary, removable
```

Actual temporary directory names are implementation details and must not be used as API contracts.

## Schema versions

- 1–4: initial physical inventory and explorer;
- 5: workspace reopen and append-only finding review;
- 6: exact format runs, observations, all matches, checkpoints and indexes.

Migrations are append-only. Back up the workspace before explicit migration. The MailVault source is
not migrated.

## Exact-format persistence

A physical run can have multiple format runs with different fingerprints. Current object projection
is indexed for UI queries; historical run metadata and matches remain versioned.

A format checkpoint is committed in the same transaction as its observation batch. Resume begins
after the last durable SHA-256.

## Locks

The normal workspace lock protects review/migration writers. The dedicated format lock prevents
concurrent exact-format jobs. Another session may open read-only when appropriate.

## Backup

Before migration or review-heavy use, back up `profiler.sqlite3`. Snapshots and inventory metadata
are rebuildable; human review history is not automatically recoverable from MailVault.
