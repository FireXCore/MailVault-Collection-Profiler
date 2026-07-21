# Roadmap

The roadmap is capability-gated. A listed future capability is not implemented merely because its
domain enum or schema placeholder exists.

## Current: `0.1-C` physical inventory and explorer

Delivered:

- read-only source contract and preflight;
- consistent SQLite snapshot;
- metadata inventory and attachment reconciliation;
- bounded file-stat inspection;
- durable findings and checkpoints;
- inventory, findings and content-object desktop exploration;
- CLI and runtime evidence wrapper.

## Required closure before the next public capability

- complete the canonical private 20–30 GB benchmark;
- record elapsed time, throughput, warning and error counts;
- reconcile observed counts against the documented baseline;
- tune worker and batch defaults from measured storage behavior;
- publish only sanitized aggregate evidence.

## Planned next slice: exact format identification

Candidate scope:

- frozen Siegfried binary provenance;
- frozen PRONOM signature provenance;
- resumable format-identification batches;
- no duplicate payload rereads where identity permits reuse;
- structured assertions and format findings.

## Deferred

- reopening existing workspaces;
- interactive pause/resume/cancel;
- full fixity modes;
- container expansion;
- JHOVE validation;
- OCR;
- semantic extraction and classification;
- graph analysis;
- procurement-specific classification;
- automatic updates;
- multi-platform signed distribution.
## 0.1.0-alpha.3 — completed source scope

- Workspace reopen and compatibility inspection.
- Backed-up schema migration.
- Run catalog after restart.
- Append-only finding review and history integrity.
- Sanitized review exports.

## Next candidate

`0.1.0-alpha.4` should focus on resumable execution controls and durable run recovery, without expanding into archive repair or semantic processing.
