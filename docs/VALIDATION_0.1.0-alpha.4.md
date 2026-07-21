# Validation evidence — 0.1.0-alpha.4

## Scope

This document separates completed evidence from work that still requires Windows CI or a private
real-archive execution. It must not be read as a fabricated claim that exact format identification
has already been run across the production collection.

## Source examined

- user-supplied `mailvault-collection-profiler` Alpha 3 source;
- private Alpha 3 real-profile workspace;
- Alpha 4 implementation candidate in this package;
- target application version `0.1.0-alpha.4`;
- profiler schema migration `0006_exact_format_identification.sql`.

## Research decisions

Alpha 4 uses a pinned Siegfried/PRONOM sidecar rather than an in-process magic-byte table or DROID
runtime because it provides machine-readable batch identification and persistent PRONOM IDs while
remaining practical to bundle in a Windows desktop application. The design records tool/signature
identity and retains all matches.

Pinned contract:

```text
Siegfried 1.11.6
PRONOM v124
container expansion disabled
```

## Completed static and frontend gates

| Gate | Result |
|---|---|
| TypeScript strict type-check | passed |
| Vite production build | passed |
| Rust source Tree-sitter syntax parse | passed, 34 Rust files |
| Release configuration parser | passed after MSI mapping correction |
| Documentation/privacy/link gate | passed in packaged candidate |
| Screenshot dimensions and deterministic names | passed |
| npm production audit | passed, 0 vulnerabilities |
| npm complete audit | passed, 0 vulnerabilities |

Tree-sitter proves syntactic parsability only; it is not a substitute for Rust type checking.

## Real Alpha 3 database migration test

A private copy of the real Alpha 3 profiler database was migrated from schema 5 to schema 6.

| Check | Result |
|---|---|
| source database SHA-256 before/after test | unchanged |
| migrated database `PRAGMA quick_check(1)` | `ok` |
| `PRAGMA foreign_key_check` | no violations |
| content objects preserved | 13,684 |
| content occurrences preserved | 22,068 |
| findings preserved | 1,484 |
| synthetic format projection | passed |
| baseline-scoped format projection isolation | passed |

Source database SHA-256 used in the private migration test:

```text
d73b040cc7a136a80d0fcab6d8194fa66c2496958bfe4e6d69f06b849d1766b8
```

This hash is evidence for the provided private profiler database, not a public MailVault content
hash.

## Implemented safety tests in code

The source includes unit/integration coverage for:

- PRONOM version parsing;
- stronger signature evidence outranking extension-only evidence;
- exclusion of extension-only alternatives from false ambiguity;
- migration idempotency and expected migration count;
- format projection, baseline isolation and cursor query behavior;
- checkpoint/run completion constraints;
- physical-profile source immutability inherited from Alpha 3.

These Rust tests require semantic compilation in CI before release.

## Windows runtime evidence supplied by the maintainer

The following gates were executed on Windows PowerShell 5.1 with Rust 1.97.1 tooling:

| Gate | Result |
|---|---|
| pinned Siegfried installation | passed |
| independent Siegfried verification | passed |
| Siegfried version | `1.11.6` |
| PRONOM signature | `v124` |
| Windows-safe relative signature JSON contract | passed |
| `cargo test -p profiler-format-siegfried --locked` | passed, 5 tests |
| `cargo fmt --all -- --check` | passed |

The first strict workspace Clippy run reached semantic compilation and reported structural/style
findings in the new exact-format modules. The implementation was then refactored without lint
suppression: options are borrowed, run-start parameters are grouped, large functions are decomposed,
large read buffers are heap-backed, and SQLite row/projection work is split into focused helpers.
This refactor still requires a fresh Windows `cargo clippy` execution before it can be recorded as
passed.

## Not yet completed

These results are deliberately **not claimed** until the updated source is rerun:

- strict workspace Clippy after the structural refactor;
- complete Rust workspace test suite;
- native Tauri compile or installer build;
- real 13,684-object Siegfried identification run;
- real throughput, ETA accuracy, format distribution or peak memory.

## Release CI requirements

A publishable tag must pass the configured Windows workflow:

1. install Rust `1.97.1` and Node `24`;
2. install and verify pinned Siegfried resources;
3. run formatting, Clippy and all Rust tests with `--locked`;
4. run TypeScript checks and production build;
5. run documentation/privacy gate;
6. compile Tauri desktop targets;
7. build NSIS and MSI installers;
8. generate `SHA256SUMS.txt` and artifact attestations where supported.

## Runtime gate after CI

Before calling Alpha 4 **runtime green**, execute the private runbook against the real workspace and
record:

- exact tool and signature hashes;
- elapsed time and peak memory;
- identified, unknown, ambiguous, mismatch and tool-error counts;
- PUID/format distribution;
- checkpoint/resume evidence;
- source non-mutation evidence;
- sanitized aggregate export.

Until that evidence exists, this package is an implementation/release candidate, not a completed
real-archive Alpha 4 benchmark.
