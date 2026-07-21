# MailVault Collection Profiler
## Technical Architecture & Benchmark Specification — 0.1 Candidate

**Status:** Canonical baseline accepted; implementation may proceed
**Date:** 2026-07-18
**Target:** Open-source, local-first desktop profiler for large MailVault collections
**Canonical archive behavior:** Read-only, evidence-preserving

---

## 1. Ground truth

This design is based on the supplied RMS discovery package and the validated MailVault archive, not sample data.

| Metric | Observed |
|---|---:|
| Archive scale | approximately 20–30 GB |
| Canonical messages | 17,296 |
| Message occurrences | 17,307 |
| MIME parts | 54,450 |
| Attachment occurrences | 21,946 |
| Messages containing attachment metadata | 10,020 |
| Unique attachment SHA-256 values | 13,592 |
| Blob rows | 13,684 |
| Message relationships | 12,115 |
| Participant rows | 51,101 |
| Known security-excluded message | 1 |

Derived directly from `attachment_inventory.csv`:

| Metric | Observed |
|---|---:|
| Total attachment occurrence bytes | 9,954,841,724 bytes (~9.27 GiB) |
| Unique attachment payload bytes | 6,442,427,318 bytes (~6.00 GiB) |
| Repeated attachment occurrences | 8,354 |
| Exact-duplicate occurrence ratio | 38.07% |
| Unique binaries seen under more than one normalized filename | 367 |
| Occurrences covered by those filename variants | 2,236 |
| Normalized filenames referring to multiple different hashes | 1,123 |
| Occurrences covered by same-name/different-content cases | 8,413 |
| Unique zero-byte binary | 1 |
| Zero-byte occurrences | 16 |
| Distinct filenames attached to the zero-byte SHA-256 | 12 |

The exported metadata also shows that the existing MIME detector often reports legacy Office files only as `application/x-ole-storage`. Exact format identification therefore must go beyond extension and generic MIME.

---

## 2. Product boundary

The profiler is not:

- a replacement for MailVault;
- an email acquisition engine;
- a document-management system;
- an OCR engine;
- a procurement classifier;
- a graph database;
- an attachment renderer;
- an antivirus product.

It is a separate derived-product application:

```text
MailVault canonical archive
        â†“ read-only adapter
Consistent source snapshot
        â†“
Physical attachment inventory
        â†“
Content-object reconciliation
        â†“
Technical format profiling
        â†“
Searchable local profiler database
        â†“
Desktop UI + reports
```

MailVault remains the source of truth for raw EML objects, blob objects, message identity, MIME structure and evidence paths.

The profiler database is disposable and rebuildable.

---

## 3. Repository strategy

Create a separate repository:

```text
FireXCore/mailvault-profiler
```

Reasons:

- MailVault currently has a Python acquisition/runtime lifecycle.
- The profiler will have a Rust/Tauri desktop lifecycle.
- The applications require separate release cadence, packaging and security boundaries.
- The profiler must eventually support adapters other than MailVault.
- A profiler failure must never affect archive acquisition.

The integration boundary is a versioned adapter contract, not shared internal imports.

Suggested license:

```text
Apache-2.0
```

matching MailVault unless a later dependency audit requires a different decision.

---

## 4. Canonical MailVault baseline

The current published `v2.0.6` release and current repository state are the canonical implementation baseline for the profiler.

A stale or mismatched package-version literal is treated as release-metadata debt only. It does not block profiler development and must not cause the adapter to infer compatibility from the package version string.

Compatibility is established through explicit archive capabilities:

- required archive layout;
- SQLite schema and schema metadata;
- required tables, columns and indexes;
- canonical object-path conventions;
- read-only snapshot compatibility;
- supported evidence roles;
- adapter contract version.

The profiler must inspect these capabilities during preflight and fail with a precise compatibility report when required structures are absent. It must not bind behavior to a cosmetic version field.

---

## 5. Technology decision

### Core

```text
Rust 2024 edition
```

The core is synchronous and pipeline-oriented. Tauri's async runtime must not own the scanning algorithm.

Use dedicated bounded worker pools for blocking file-system operations. Local disk access is not made faster merely by wrapping it in async tasks.

### Desktop

```text
Tauri 2
React 19
TypeScript
```

Tauri commands handle control operations. Tauri channels carry structured progress and findings to the frontend.

### Profiler metadata store

```text
SQLite
WAL mode
single controlled writer
many UI readers
prepared statements
batched transactions
FTS5 for textual metadata search
```

At this dataset size, metadata volume is small. DuckDB is not required in the interactive runtime. Optional Parquet exports can be added later for analytics without making DuckDB an application dependency.

### Format identification

Primary:

```text
Siegfried + PRONOM
```

Run as a pinned sidecar or bundled tool with recorded executable and signature-database versions.

Do not spawn one process per file. Feed eligible paths to a long-lived or batch invocation and stream machine-readable output.

Selective validation is a later profiler slice:

```text
JHOVE
```

It is not part of the first physical inventory gate.

---

## 6. Source snapshot consistency

The profiler must not scan a changing operational database without a consistency boundary.

### Required flow

1. Validate MailVault archive layout.
2. Detect active MailVault writer/lock state.
3. Open source SQLite in read-only mode.
4. Create a consistent local SQLite snapshot using the SQLite backup API.
5. Close the source database.
6. Perform the profiler run against the local snapshot.
7. Resolve blob paths against the original read-only archive root.
8. Record snapshot provenance.

Do not use `immutable=1` automatically. It is only valid when the application can prove the source database and side files cannot change.

The snapshot manifest records:

```text
MailVault schema version
source database path fingerprint
source database size and timestamps
snapshot database SHA-256
source archive root identity
messages count
message parts count
blob count
run creation time
adapter version
```

If source counts change during snapshot creation, the run fails before inventory processing.

---

## 7. Path safety

Every path loaded from MailVault is untrusted metadata.

For every raw/blob path:

1. reject NUL and invalid path encoding;
2. resolve relative paths against the configured archive root;
3. canonicalize the parent path safely;
4. ensure the resolved target remains under the archive root;
5. do not follow unsafe reparse points/symlinks outside the root;
6. never pass filenames through a shell;
7. identify objects by SHA-256, not display filename.

A path violation becomes a finding and is never opened.

---

## 8. Physical inventory model

### Collection

One registered MailVault archive.

### Source snapshot

One consistent database snapshot used by a profiler run.

### Source message

Minimal message evidence required for attachment history:

```text
message id
archive UUID
provider message identity
provider thread identity
RFC Message-ID
subject
header date
raw SHA-256
```

### Source MIME part

A row corresponding to the real `message_parts` record:

```text
message id
part path
parent part path
role
declared MIME
detected MIME
content disposition
content ID
original filename
safe filename
size
SHA-256
blob path
defects
```

### Content object

One SHA-256-addressed binary.

### Occurrence

One MIME-part occurrence pointing to one content object.

### Filename variant

Aggregated original/decoded/normalized filenames for one content object.

### Format assertion

One versioned tool result:

```text
tool
tool version
signature database
PUID
format name
format version
MIME
identification basis
warning
execution timestamp
```

### Finding

A versioned, explainable abnormal condition.

### Processing event

Append-only provenance for discovery, verification and identification.

---

## 9. Required profiler schema

Core tables:

```text
collections
source_snapshots
profiler_runs
run_stages
run_checkpoints
source_messages
source_parts
content_objects
content_occurrences
filename_variants
format_assertions
processing_events
findings
schema_migrations
```

Critical uniqueness rules:

```text
collections:
  unique archive identity

source_messages:
  unique (snapshot_id, source_message_id)

source_parts:
  unique (snapshot_id, source_message_id, part_path)

content_objects:
  unique (collection_id, sha256)

content_occurrences:
  unique (snapshot_id, source_message_id, part_path)

format_assertions:
  unique (content_object_id, tool_name, tool_version, signature_version)
```

The profiler stores stable source IDs and source evidence. It does not replace them with generated-only identifiers.

---

## 10. Pipeline

### Stage A — Preflight

Checks:

- archive root;
- database presence;
- schema compatibility;
- source lock state;
- destination free space;
- profiler database migration;
- bundled sidecar integrity;
- write permission only in profiler workspace;
- source remains read-only.

Progress unit:

```text
checks completed / checks planned
```

### Stage B — Source snapshot

Uses SQLite backup API.

Progress unit:

```text
database pages copied / total pages
```

### Stage C — Metadata inventory

Stream these source tables in stable primary-key order:

```text
messages
message_parts
blobs
```

Do not issue N+1 queries per MIME part.

Use joined or staged streaming queries and prepared inserts.

Progress units are exact source counts:

```text
messages: 17,296
MIME parts: 54,450
blob rows: 13,684
```

### Stage D — Reconciliation

Build:

```text
21,946 attachment occurrences
13,592 unique attachment hashes
```

Required reconciliations:

- attachment occurrence → message;
- attachment occurrence → content object;
- content object → blob row when expected;
- blob row → at least one source part or explained non-attachment role;
- size consistency;
- source path availability;
- known exclusion state;
- zero-byte handling;
- duplicate and filename-variant aggregates.

The 92-row difference between `13,684 blob rows` and `13,592 unique attachment hashes` must be explained by source role/state. It must never be silently forced to zero.

### Stage E — File-system stat

For unique content objects only:

```text
exists
regular file
size
last-write metadata
path containment
readability
```

This stage uses a bounded worker pool. It does not hash the file by default.

### Stage F — Optional fixity

Modes:

```text
trust-mailvault
verify-stale
verify-all
```

Default first profiler run:

```text
trust-mailvault
```

because the archive has already completed a full MailVault verification with zero hash and size mismatches, apart from the known excluded object.

`verify-stale` hashes only objects that are unverified, changed in size/metadata, or lack valid verification evidence.

`verify-all` performs a full SHA-256 pass.

Progress is byte-accurate:

```text
bytes hashed / eligible bytes
objects hashed / eligible objects
```

### Stage G — Format identification

Eligible unique binaries are passed to a batched Siegfried invocation.

Progress:

```text
objects identified / eligible objects
bytes identified / eligible bytes
```

Store every assertion, including unknown and ambiguous results.

### Stage H — Aggregate and publish

Build:

- primary display filename;
- filename variants;
- occurrence count;
- message count;
- thread count;
- sender-domain count;
- first/last seen;
- same-hash/different-name;
- same-name/different-hash;
- zero-byte;
- missing;
- excluded;
- extension/signature mismatch;
- unknown format.

Publish only after consistency checks pass.

---

## 11. Concurrency model

Use bounded queues:

```text
snapshot reader
    â†“ bounded channel
normalizer/reconciler
    â†“ bounded channel
file-stat workers
    â†“ bounded channel
format identification coordinator
    â†“ bounded channel
single database writer
```

Rules:

- one SQLite writer connection;
- UI uses separate read-only connections;
- file-system worker count is configurable and benchmarked;
- hashing worker count is independent of metadata worker count;
- format sidecar has bounded input and output buffers;
- no unbounded task spawning;
- no full inventory retained in RAM;
- cancellation is cooperative and checkpoint-safe.

The application records queue wait time and worker utilization so bottlenecks can be diagnosed rather than guessed.

---

## 12. Checkpoint and resume

Checkpoint identity:

```text
collection
source snapshot
pipeline version
stage
stable cursor
tool versions
configuration fingerprint
```

Examples:

```text
metadata cursor:
  message_parts.id

file-stat cursor:
  content_objects.id

format cursor:
  content_objects.id
```

A checkpoint is committed in the same database transaction as the completed result batch.

After a crash:

- committed batches remain valid;
- uncommitted work is retried;
- no duplicate occurrences are created;
- progress resumes from the durable cursor;
- a tool-version change invalidates only the relevant downstream stage.

Pause:

1. stop accepting new work;
2. allow current bounded tasks to finish;
3. flush writer batch;
4. save checkpoint;
5. transition run to `paused`.

Cancel does not delete previously committed profiler evidence. It marks the run cancelled and leaves a resumable or discardable partial workspace.

---

## 13. Progress contract

Never invent a percentage.

The backend emits structured events:

```text
run_id
stage
stage_state
completed_items
total_items
completed_bytes
total_bytes
instant_throughput
smoothed_throughput
elapsed_ms
eta_ms or null
active_workers
queue_depth
warnings
errors
current_object_display
checkpoint_sequence
```

Rules:

- `total_items` comes from the source snapshot or stage eligibility query.
- ETA is null until enough stable samples exist.
- ETA is reset when phase or workload characteristics change.
- Current filename is display-only and throttled.
- UI event emission is throttled independently from durable checkpoint frequency.
- The first run shows exact per-stage progress, not a fabricated cross-stage overall percentage.
- A single overall ETA may be shown only after benchmark weights or prior-run timings exist, and must be labeled estimated.

---

## 14. Search and UI

### Collections

- archive identity;
- source version/schema;
- last complete snapshot;
- physical counts;
- integrity state;
- profiler version.

### Run view

- stage timeline;
- item and byte progress;
- throughput;
- ETA;
- worker/queue health;
- warnings/errors;
- pause/resume/cancel;
- durable checkpoint indicator.

### Inventory

Virtualized table with cursor pagination:

```text
primary filename
actual format
PUID
extension
size
occurrences
messages
threads
first seen
last seen
availability
integrity
finding count
```

Never transfer all rows to React.

### Content-object detail

- SHA-256;
- canonical source path state;
- actual format assertions;
- all filename variants;
- all message occurrences;
- subjects/senders/dates/threads;
- processing events;
- findings.

### Findings

Saved filters:

```text
missing
known excluded
zero byte
same hash / different filenames
same filename / different hashes
extension / signature mismatch
unknown format
unreadable
path violation
format tool failure
```

### Accessibility and internationalization

- keyboard navigation;
- visible focus;
- screen-reader labels;
- reduced-motion support;
- LTR/RTL-safe layout;
- English first for open source;
- Persian translation supported through message catalogs, not hard-coded components.

---

## 15. Index strategy

Profiler SQLite indexes must match real queries.

Required indexes:

```text
content_objects(collection_id, sha256)
content_objects(collection_id, size_bytes)
content_objects(collection_id, actual_mime)
content_occurrences(content_object_id)
content_occurrences(source_message_id)
source_messages(provider_thread_value)
source_messages(header_date)
filename_variants(normalized_filename)
findings(code, severity)
processing_events(content_object_id, event_type)
```

FTS5 external-content index:

```text
primary filename
filename variants
subject
sender domain
SHA-256
format name
PUID
```

Hash lookup remains a normal indexed equality query, not FTS.

---

## 16. Performance strategy

The payload archive is large, but the first profiler workload is not automatically a 20–30 GB sequential read.

Fast mode reads:

- MailVault SQLite metadata;
- file-system metadata for unique objects;
- only the file regions/paths required by the format identifier.

It does not re-read all raw EML objects.

Full SHA-256 verification reads eligible unique blobs, approximately 6.00 GiB according to the supplied attachment inventory, not the 9.27 GiB attachment-occurrence total.

Primary performance risks:

1. repeated reads of exact duplicate content;
2. process-per-file format identification;
3. N+1 SQLite queries;
4. too many random file opens on HDD;
5. unbounded UI events;
6. long SQLite write transactions;
7. hashing already-verified objects unnecessarily;
8. scanning raw EML when source MIME metadata already exists.

---

## 17. Benchmark plan

No performance constant is frozen before this benchmark.

### Dataset

Use the real archive and a sanitized reproducible test fixture derived from its structural distributions.

### Modes

```text
metadata-only
fast-profile
verify-stale
verify-all
format-identification
full-profiler
resume-after-forced-stop
```

### Worker matrix

```text
1
2
4
8
```

Use separate matrices for file stat, hashing and format identification.

### Database batch matrix

Candidate batch sizes are benchmark inputs, not defaults:

```text
100
500
1,000
2,000
5,000
```

### Storage scenarios

Record actual device class:

```text
internal SSD
external SSD
HDD if available
cold cache
warm cache
```

### Metrics

```text
wall time
CPU time
bytes read
objects per second
MiB per second
peak RSS
SQLite WAL peak size
checkpoint duration
resume replay count
format sidecar startup cost
writer queue saturation
UI event rate
warnings/errors
```

### Correctness gates during benchmark

Every benchmark run must produce the same canonical counts and findings. Faster but inconsistent output is a failed benchmark.

---

## 18. Security

- source archive opened read-only;
- profiler workspace separate from archive root;
- no attachment execution;
- no web rendering of attachment content in 0.1;
- no shell command construction;
- sidecars invoked with argument arrays;
- sidecar executable and signature data hashes recorded;
- timeouts and output-size limits;
- path-containment enforcement;
- logs exclude bodies, credentials and full sensitive paths by default;
- crash reports redact source identifiers unless explicitly exported;
- suspicious extensions are findings even when prior exporter flags missed them;
- `.shs` and `.reg` are explicitly treated as unsafe metadata cases;
- archive/container expansion is outside the first release.

---

## 19. Observability

Structured local logs:

```text
timestamp
level
run_id
stage
event
object_id or redacted hash prefix
duration
result
error_code
```

Internal metrics:

```text
queue depth
queue wait
worker busy time
database commit time
snapshot copy time
bytes read
objects processed
retry count
sidecar restart count
```

OpenTelemetry export is optional and disabled by default. The application must not send telemetry without explicit user configuration.

---

## 20. Testing

### Unit

- path containment;
- filename normalization;
- state transitions;
- progress arithmetic;
- ETA behavior;
- checkpoint cursor encoding;
- hash parsing;
- format assertion parsing.

### Integration

- real MailVault schema fixture;
- read-only snapshot;
- idempotent rerun;
- same hash/different name;
- same name/different hash;
- missing blob;
- excluded blob;
- zero-byte object;
- malformed MIME metadata;
- unsupported schema version;
- WAL/source database present;
- sidecar timeout;
- sidecar malformed output.

### Crash recovery

Force termination:

- during source snapshot;
- between worker result and DB commit;
- during DB commit;
- during format sidecar output;
- before publish;
- after publish before UI acknowledgement.

### Property testing

- arbitrary filenames and Unicode;
- arbitrary relative paths;
- duplicate occurrence ordering;
- progress monotonicity;
- checkpoint replay.

### Performance regression

CI uses synthetic fixtures. Real-archive benchmark results are stored as release evidence but must not publish sensitive filenames or addresses.

---

## 21. Release slices

### 0.1-A — Repository and source contract

- Rust workspace;
- Tauri shell;
- schema migrations;
- MailVault adapter interface;
- archive preflight;
- read-only SQLite snapshot;
- structured logging;
- run state machine;
- CI and packaging skeleton.

### 0.1-B — Physical inventory

- streaming messages/parts/blobs;
- content objects;
- occurrences;
- reconciliation;
- findings;
- idempotency;
- checkpoint/resume;
- exact progress.

### 0.1-C — Desktop inventory UI

- collections;
- run progress;
- virtualized inventory;
- content-object detail;
- findings filters;
- Persian/English localization boundary.

### 0.1-D — Exact format identification

- pinned Siegfried sidecar;
- PRONOM assertions;
- batched streaming invocation;
- extension/signature mismatch;
- unknown/ambiguous formats;
- tool provenance.

### 0.1-E — Benchmark and release hardening

- real-archive benchmark;
- tuned worker and batch defaults;
- forced-crash validation;
- installer signing plan;
- SBOM;
- dependency/license audit;
- reproducible release notes.

---

## 22. 0.1 acceptance gate

The release is green only when:

- source archive is never modified;
- source snapshot is consistent and reproducible;
- 17,296 messages reconcile;
- 54,450 MIME parts reconcile;
- 21,946 attachment occurrences reconcile;
- 13,592 unique attachment SHA-256 values reconcile;
- 13,684 blob rows reconcile or every difference is explicitly categorized;
- 12,115 message relations are preserved as source metadata or explicitly deferred without loss;
- 1 known excluded object remains excluded;
- zero-byte occurrences are visible and not treated as valid non-empty documents;
- rerun creates zero duplicate canonical records;
- pause/resume and forced crash continue from durable checkpoints;
- progress counters are exact;
- format results record tool and signature versions;
- UI remains responsive during the full real-archive run;
- no attachment bytes are executed or rendered;
- release benchmark and reconciliation report are produced.

---

## 23. Immediate next action

Before production code begins, obtain the canonical current MailVault source tree that matches the validated archive.

Then perform a code-level adapter audit covering:

```text
repository schema DDL
indexes and foreign keys
schema_meta values
archive-path construction
single-run locks
view rebuild snapshot logic
verify implementation
known exclusion representation
current migrations
2.0.6 release state
```

The first code commit should be `0.1-A`, not the scanner implementation.
