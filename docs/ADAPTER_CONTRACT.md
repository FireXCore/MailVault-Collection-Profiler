# MailVault adapter contract

## Required archive layout

```text
<root>/database/mailvault.sqlite3
<root>/objects/raw/sha256/
<root>/objects/blobs/sha256/
<root>/state/
```

The adapter must never call MailVault's path builder because that function creates missing
folders. Profiling preflight is observation-only.

## Required source schema

The initial adapter supports MailVault schema version 3 and reads these canonical entities:

- `schema_meta`
- `accounts`
- `messages`
- `message_occurrences`
- `message_participants`
- `message_parts`
- `blobs`
- `message_relations`

Required columns are declared in code and checked through `PRAGMA table_info`. Recommended source
indexes are checked through `sqlite_master`; missing indexes are reported as performance warnings,
not silently recreated in the source.

## Writer lock

MailVault uses `state/sync.lock`. When present, the profiler attempts a non-blocking compatible
exclusive lock. A held lock blocks snapshot creation. An indeterminate lock result fails closed
for snapshot creation.

## Paths

`raw_path`, `blob_path` and `storage_path` are untrusted source metadata. Later inventory stages
must resolve them beneath the canonical archive root and reject escapes, reparse-point escapes and
NUL-containing values.
