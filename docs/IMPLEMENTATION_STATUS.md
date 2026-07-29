# Implementation status — 0.1.0-alpha.6

## Implemented

- read-only MailVault schema-v3 preflight and SQLite Online Backup snapshot;
- stable collection binding with archive identity aliases and no path-based fallback;
- full metadata reconciliation from every immutable source snapshot;
- cumulative SHA-256 content projection with current/inactive state;
- selective file-stat reuse for successful unchanged objects;
- automatic retry for missing, mismatched, unavailable and I/O-error objects;
- automatic exact-format identification in the physical profile pipeline;
- Siegfried 1.11.6 and PRONOM v124 tool/signature evidence;
- same-run resume after interrupted snapshot, inventory, file-stat or format stages;
- initial, incremental and rebuild generations in one workspace;
- bounded successful-snapshot retention and orphan cleanup;
- continuous CLI watch with source-revision polling and retryable-error backoff;
- schema migration 6 to 7;
- Windows quality and real-runtime acceptance scripts.

## Trusted predecessor evidence

Alpha 4 completed the Windows quality gate with 36 Rust tests and processed 13,684/13,684 real
content objects with 13,636 identified, 46 unknown, zero ambiguous and zero tool errors. Alpha 6
preserves that exact-format implementation and adds the continuous lifecycle around it.

## Required before GitHub publication

- `cargo fmt`, strict workspace Clippy and all Rust tests;
- TypeScript, Vite and native Tauri checks;
- MSI and NSIS build;
- real initial and immediate no-change profile;
- real IMAP delta and duplicate-SHA reuse;
- force-kill resume in all four stages;
- 100-cycle bounded-growth soak;
- independent rebuild equivalence;
- unchanged canonical MailVault SHA-256;
- sanitized public evidence and SHA256SUMS.

## Permanently outside this profiler

OCR, text extraction, embeddings, vector databases, RAG, LLM classification, procurement semantics,
RMS integration, attachment execution and container expansion.
