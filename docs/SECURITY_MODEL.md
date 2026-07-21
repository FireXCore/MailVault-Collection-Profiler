# Security model

## Trust boundaries

### Canonical MailVault archive

High-value, read-only source. The profiler must never use it as a workspace, write temporary files
inside it, modify its database or repair its object stores.

### Profiler workspace

Writable, disposable derived data. It contains a source database snapshot, profiler database,
checkpoints and derived metadata. It is outside the source archive by invariant.

### Runtime evidence directory

Writable output for logs, manifests and JSON results. It must also remain outside the source
archive.

### Archive metadata

All paths, filenames, MIME labels, subjects, domains and database values are untrusted input. They
may be malformed, hostile, unexpectedly large or privacy-sensitive.

## Enforced invariants

- SQLite source connections use read-only behavior.
- Workspace and evidence roots are checked for path overlap with the archive.
- The source database snapshot uses SQLite's backup API instead of copying an active database file.
- Snapshot publication uses a durable temporary file and atomic rename sequence.
- On Windows, durable file sync uses a write-capable handle required by `FlushFileBuffers`.
- Blob locators are parsed and validated against the expected content-addressed layout.
- Resolved file paths must remain contained by the canonical object-store root.
- Locator mismatches are not opened.
- Physical files are inspected as files; payloads are not executed or rendered.
- Explorer commands use the active profiler run and do not accept arbitrary source paths.
- Read-side profiler connections are opened read-only/query-only.
- Progress counters must be monotonic within a stage.

## Failure policy

The profiler fails closed for:

- unsupported or newer source schema;
- required path or structure failures;
- active writer lock where consistency cannot be guaranteed;
- source integrity failures;
- workspace/source overlap;
- invalid persisted run state;
- non-monotonic progress events;
- snapshot or publication I/O failure.

Physical content findings such as missing blobs and size mismatches are generally non-fatal because
they are evidence the profiler is designed to record.

## Explicit non-capabilities

`0.1.0-alpha.3` does not:

- execute attachments;
- render HTML email or embedded active content;
- extract archives or containers;
- run OCR, LLMs or semantic classifiers;
- send telemetry;
- upload data;
- repair the MailVault archive.

## Vulnerability reporting

Use GitHub private vulnerability reporting. Do not publish exploit details or real archive samples.
See the root [security policy](../SECURITY.md).

## Review-store security

Only one process may write review state. Concurrent sessions open read-only. Review events are append-only, hash chained and validated on open. Corruption disables review writes. Notes are excluded from routine logs and sanitized exports. Existing workspace open uses SQLite read-only/read-write flags without create.
