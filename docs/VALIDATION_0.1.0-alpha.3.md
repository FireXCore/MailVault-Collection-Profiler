# Validation evidence — 0.1.0-alpha.3

**Validation date:** 2026-07-20
**Release state:** Windows quality gate green; real-archive runtime validation green; private raw evidence retained outside the repository.

## Claim boundary

This document records aggregate results from the controlled Windows validation run. The repository
contains no MailVault database, profiler database, attachment payload, raw filename export, email
address, private workstation path or private runtime-evidence archive.

The authoritative private evidence pack contains the full command output, workspace database,
snapshot database and checksum manifest. It is intentionally not published.

## Windows quality gate

The following gate completed successfully on the Windows x64 release workstation:

```cmd
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --exclude mailvault-profiler-desktop --locked -- -D warnings
cargo test --workspace --all-features --exclude mailvault-profiler-desktop --locked
cargo clippy --package mailvault-profiler-desktop --all-targets --all-features --locked -- -D warnings
npm ci
npm run type-check
npm run build
npm run check:docs
npm run check:release-config
```

Recorded test coverage from that run:

| Test surface | Result |
|---|---:|
| MailVault adapter unit tests | 4 passed |
| MailVault preflight integration tests | 2 passed |
| Profiler core unit tests | 6 passed |
| Profiler engine lock-classifier tests | 2 passed |
| End-to-end profile pipeline tests | 4 passed |
| SQLite storage, migration and review-integrity tests | 10 passed |
| Total Rust tests | 28 passed, 0 failed |
| TypeScript type-check | passed |
| Vite production build | passed |
| npm audit | 0 known vulnerabilities |
| Desktop native Clippy/compile gate | passed |

## Real archive profile

The release candidate completed a full read-only profile against a private MailVault schema-v3
archive on Windows x64.

### Preflight

- source contract: compatible;
- source schema: 3;
- SQLite quick check: `ok`;
- writer lock: absent;
- warnings: 0;
- errors: 0.

### Source and snapshot metrics

Source metrics and snapshot metrics matched for every recorded aggregate:

| Metric | Recorded |
|---|---:|
| Accounts | 1 |
| Messages | 17,296 |
| Message occurrences | 17,307 |
| MIME parts | 54,450 |
| Attachment occurrences under the current adapter contract | 18,552 |
| Blob rows | 13,684 |
| Blob bytes | 6,467,253,277 |
| Message relationships | 12,115 |
| Participant rows | 51,101 |

The private evidence pack retains the snapshot SHA-256 and the source/snapshot metric comparison.
The private snapshot fingerprint is not published in this repository.

### Inventory result

| Metric | Recorded |
|---|---:|
| Content objects | 13,684 |
| Content occurrences | 22,068 |
| Zero-byte content objects | 1 |
| Same hash with different names | 375 |
| Same normalized name with different hashes | 1,107 |

### Physical object result

| Result | Count |
|---|---:|
| Total objects | 13,684 |
| Available objects | 13,683 |
| Missing objects | 1 |
| Unreadable objects | 0 |
| Invalid locators | 0 |
| Non-regular objects | 0 |
| Unsafe reparse objects | 0 |
| I/O errors | 0 |
| Size matches | 13,683 |
| Size mismatches | 0 |

### Findings result

| Result | Count |
|---|---:|
| Total findings | 1,484 |
| Errors | 0 |
| Warnings requiring attention | 2 |
| Informational filename/content relationships | 1,482 |
| Zero-byte content warning | 1 |
| Missing blob warning | 1 |

## Workspace reopen and review acceptance

The completed workspace was closed and opened from a new desktop process. The retained run was
loaded without profiling the MailVault source again.

Acceptance evidence:

- workspace schema 5 reported compatible;
- completed run catalog survived process restart;
- inventory and finding counts remained stable;
- a second writer fell back to read-only on Windows lock contention;
- finding review decisions persisted across restart;
- append-only review events remained valid under SHA-256 chain verification;
- tamper-detection tests disabled writes after review-history corruption;
- original source and original runtime evidence were not changed by review activity.

## Sanitized export acceptance

Sanitized JSON and CSV exports were generated and verified to exclude:

- source archive paths;
- profiler workspace paths;
- real filenames;
- email addresses;
- review notes;
- raw object locators.

Exports use aggregate values and non-reversible short tokens where row-level identity is required.

## Baseline reconciliation note

The historical private baseline records 21,946 attachment occurrences across the broader supplied
metadata inventory. The alpha.3 source contract reported 18,552 attachment occurrences during the
validated run. These values are retained as distinct metrics and are not substituted for one
another. The project must continue to reconcile the role and filtering boundaries documented in
[Real archive baseline](REAL_ARCHIVE_BASELINE.md).

## Final validation state

```text
QUALITY GATE GREEN
REAL ARCHIVE PROFILE GREEN
SOURCE / SNAPSHOT METRICS MATCHED
WORKSPACE REOPEN GREEN
REVIEW PERSISTENCE GREEN
WINDOWS LOCK FALLBACK GREEN
SANITIZED EXPORT GREEN
SOURCE MUTATION NONE
PRIVATE RAW EVIDENCE RETAINED
```

`0.1.0-alpha.3` is Runtime Green for the implemented alpha scope. It remains a development
pre-release with unsigned Windows installers and the limitations listed in
[Implementation status](IMPLEMENTATION_STATUS.md).
