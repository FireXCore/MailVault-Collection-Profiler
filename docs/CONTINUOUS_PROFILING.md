# Continuous profiling — Alpha 6 final

Alpha 6 is the final feature release of MailVault Collection Profiler. A workspace is bound to one
stable MailVault archive identity. Every run takes a consistent SQLite Online Backup snapshot,
reconciles all metadata from that immutable snapshot, and processes only content whose physical or
exact-format input fingerprint is new, changed, or retryable.

The canonical pipeline is:

`preflight → snapshot → inventory/reconciliation → selective file-stat → selective exact-format → success`

A failed process resumes the same run and snapshot. Successful SHA-256 content is not re-stat-ed or
re-identified. Missing, mismatched, unavailable and tool-error objects remain retryable. Snapshot
retention keeps incomplete runs and the configured number of recent successful snapshots.

`--rebuild` creates a new complete generation after any resumable run is resolved. `--watch` polls the
MailVault source revision and starts work only after the archive changes.
