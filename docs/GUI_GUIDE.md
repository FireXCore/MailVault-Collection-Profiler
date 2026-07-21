# Desktop GUI guide

The desktop application has three primary views. Inventory and findings are enabled after a
successful profile completes in the current application session.

## 1. Collection setup

![Collection setup](assets/screenshots/01-collection-setup-preflight.png)

### Archive root

Select the MailVault archive root, not the database file itself. The application derives the
canonical database, raw object store, blob object store and state directory from this root.

### Read-only preflight

Preflight validates:

- required paths and file types;
- supported schema version;
- required tables, columns and indexes;
- source writer lock;
- SQLite integrity and source metrics.

The source contract panel lists every check and its required or advisory level.

### Workspace

The profiler workspace must be separate from the source archive. The action remains disabled until
preflight is compatible and a workspace is selected.

## 2. Live profile progress

![Live profile](assets/screenshots/02-profile-running.png)

Progress events expose:

- current stage and stage state;
- completed and total items;
- completed and total bytes when available;
- instantaneous and smoothed throughput;
- elapsed time and ETA;
- active workers and queue depth;
- warnings, errors and checkpoint sequence;
- current object display when appropriate.

Progress is monotonic within a stage. A later event with lower item, byte or checkpoint counters is
rejected by the core contract.

## 3. Physical inventory

![Inventory explorer](assets/screenshots/03-inventory-explorer.png)

The inventory represents exact binary content identities, not only attachment rows.

### Search

The search field matches:

- primary and historical filenames;
- SHA-256;
- source-detected MIME type;
- message subject;
- sender domain.

### Filters

- Availability: available, missing, unreadable, invalid locator, non-regular.
- Size state: match, mismatch, unavailable.
- Finding code: zero byte, multiple filenames, missing blob, size mismatch.

### Pagination

The explorer uses stable SHA-256 keyset pagination. It does not use unstable offset pagination for
large inventory browsing.

## 4. Findings

![Findings explorer](assets/screenshots/04-findings-explorer.png)

Findings can be filtered by severity and structured code. An object-level finding opens its content
object. Collection-level findings remain reviewable without an object identity.

## 5. Content-object detail

![Content-object detail](assets/screenshots/06-content-object-detail.png)

The detail drawer includes:

- complete SHA-256 identity;
- availability and size state;
- expected size, occurrence count, message count and thread count;
- technical findings;
- filename variant history;
- message occurrence history with source part path.

The drawer intentionally shows metadata only. It does not render or execute attachment payloads.

## Existing workspace and review journey

The start page provides **Profile new archive** and **Open existing workspace**. Inspection shows schema, compatibility, lock mode and run count. Older compatible workspaces require explicit migration confirmation. The run catalog opens previous inventory and findings. The finding drawer exposes status, required-note validation and append-only history. Informational filename/content relationships are separated from attention-required findings.
