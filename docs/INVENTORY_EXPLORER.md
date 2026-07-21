# Physical Inventory Explorer

The explorer is a read-only projection over a completed profiler run. It never accepts an
arbitrary database path from the React webview and never opens the MailVault source database.

## Session boundary

After a profile completes, the Tauri backend stores three values in native process memory:

```text
profiler database path
collection ID
run ID
```

Every explorer command obtains these values from native state. The webview may provide filters,
cursors and a content-object ID, but it cannot select another profiler database, collection or
run. Content-object detail queries are additionally scoped by the active collection ID.

## Inventory paging

Inventory rows are ordered by canonical lowercase SHA-256. Pagination uses the last returned hash
as a stable keyset cursor:

```sql
WHERE sha256 > :after_sha256
ORDER BY sha256
LIMIT :page_size_plus_one
```

The API fetches one extra row to determine `hasMore`; it does not use offset pagination. Uppercase
hexadecimal cursors are normalized to lowercase before querying. Page size is limited to 250 rows.

## Search and filters

The current 0.1-C query supports:

- exact or partial SHA-256;
- filename variants;
- source-detected MIME type;
- message subject;
- sender domain;
- physical availability state;
- physical size state;
- unresolved finding code.

Search and page queries share the same filter predicate. The UI separates draft filters from
applied filters so editing a control cannot silently change the meaning of a subsequent cursor.

The current archive baseline has 13,592 unique attachment hashes and 21,946 attachment
occurrences, so the initial SQL search path is intentionally simple and measurable. FTS5 remains a
future migration only after real-archive query benchmarks prove it necessary.

## Content-object detail

Detail returns:

- canonical SHA-256 identity;
- expected and observed size state;
- occurrence, message and thread counts;
- all normalized filename variants;
- up to 500 ordered email occurrences;
- unresolved findings from the active run.

When more than 500 occurrences exist, the response reports the full count and marks the list as
truncated. No attachment payload is read or rendered.

## Findings explorer

Findings use a stable ID cursor and can be filtered by normalized finding code and severity.
Accepted severities are `info`, `warning` and `error`. Finding-code input is normalized to uppercase
and limited to `A-Z`, digits and underscore.

The summary is calculated for the entire active run, independently of the page filter, so overview
cards do not change merely because a list filter is applied.

## SQLite access

Explorer connections use:

```text
SQLITE_OPEN_READ_ONLY
PRAGMA query_only=ON
PRAGMA foreign_keys=ON
PRAGMA trusted_schema=OFF
```

The connection verifies the profiler `application_id` and exact schema `user_version` before any
query. Schema version 4 adds explorer-oriented indexes for active-run findings, message threads,
message dates, sender domains and MIME grouping.

## Failure behavior

Malformed persisted counters, unknown state values or invalid finding JSON fail the query. They are
not silently converted to zero or an `uninspected` state. This keeps database corruption and
migration defects visible instead of presenting false inventory data.
