# Architecture

## Product boundary

```text
MailVault canonical archive
  ↓ read-only adapter and consistent snapshot
Physical inventory
  ↓ unique SHA-256 content objects and occurrences
Exact format identification
  ↓ versioned PUID/format assertions
Future document corpus
  ↓ safe extraction and selective OCR
RMS intelligence
```

MailVault remains the source of truth. The profiler database is derived and rebuildable, except for
human review events that should be backed up.

## Workspace crates

- `profiler-core`: contracts, progress, errors, run and exact-format types;
- `profiler-storage-sqlite`: migrations, inventory/review/format persistence and queries;
- `profiler-adapter-mailvault`: schema-v3 read-only adapter and snapshot;
- `profiler-engine`: physical-profile, workspace and exact-format orchestration;
- `profiler-format-siegfried`: pinned sidecar probe, staging, bounded execution and JSON parsing;
- `mailvault-profiler-cli`: headless operations and evidence streams;
- `mailvault-profiler-desktop`: Tauri command boundary and React UI.

## Exact format stage

The engine processes content objects in stable SHA-256 order. It skips unavailable and zero-byte
objects, resolves each canonical locator beneath the archive root, invokes Siegfried in bounded
batches and commits observations, all matches and a checkpoint in one transaction.

The runner is deliberately outside `profiler-engine` so a future identifier can implement the same
core contract without changing physical inventory or UI persistence.

## Concurrency

- one exact-format writer per workspace, enforced by an OS file lock;
- one sidecar process per batch;
- Siegfried worker count is configured explicitly;
- stdout/stderr readers are bounded and joined;
- failed batches are split recursively to isolate per-object failures;
- SQLite commits remain batch-scoped.

## Evidence identity

A completed format run stores:

- physical baseline run ID;
- application contract version;
- executable and signature hashes;
- observed tool and signature versions;
- configuration fingerprint;
- counts and byte totals;
- durable checkpoint;
- object observations and all matches.

A run cannot be marked complete unless committed object count equals the expected total.

## Security

Source paths are untrusted. Every file is canonicalized and required to remain beneath the MailVault
root. No command shell is used. The sidecar receives argument arrays and a generated list file.
Container expansion is disabled. See [Security model](SECURITY_MODEL.md).
