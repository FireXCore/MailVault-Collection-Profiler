# Validation evidence — 0.1.0-alpha.2

**Date:** 2026-07-19
**Release scope:** corrected Windows source package, desktop build, installers and controlled runtime

## Local Windows quality gate

The corrected source was validated inside an x64 Visual Studio Developer Command Prompt with the
pinned Rust, Node.js and Windows SDK toolchain.

```text
Rust formatting                                      passed
Rust core/CLI Clippy, all targets/features           passed
Rust core/CLI tests                                  passed
Windows durable-snapshot regression                  passed
End-to-end physical profile fixture                  passed
Profiler storage and migration tests                 passed
Desktop Tauri Clippy, all targets/features           passed
TypeScript project build                             passed
Vite production build                               passed
npm audit                                            0 vulnerabilities
NSIS bundle                                          built
MSI bundle                                           built
Installed Windows application                        launched
```

The Windows-specific regression proves that a completed SQLite snapshot is durably synchronized
through a write-capable file handle. The end-to-end fixture completes preflight, snapshot,
inventory, reconciliation and physical findings without modifying the source database.

## Real archive runtime evidence observed

A controlled private MailVault archive passed the desktop read-only preflight with:

```text
schema          v3
writer lock     absent
messages        17,296
MIME parts      54,450
blob rows       13,684
relationships   12,115
participants    51,101
```

The installed desktop application started the metadata inventory and reported monotonic structured
progress. The captured runtime view reached `82.2%` of the active metadata stage with one worker and
`0 / 0` warnings/errors at that moment.

This evidence proves the application can open the real schema-v3 archive, create its separate
workspace and execute the live inventory pipeline. It does **not** claim the full run completed or
establish final throughput, memory, elapsed-time or reconciliation totals. Those claims require the
terminal evidence bundle from a completed run.

## Corrected release defects covered

- npm lockfile uses the public npm registry;
- Windows SDK/MSVC setup is documented;
- snapshot durable sync no longer fails with `ERROR_ACCESS_DENIED`;
- PowerShell quality gate fails on the first native-command error and invokes `npm.cmd` directly;
- unsupported `Debug` derivation was removed from the Tauri progress channel;
- MSI maps public version `0.1.0-alpha.2` to numeric WiX version `0.1.0.2`;
- desktop compilation and MSI/NSIS packaging are validated before release.

## Source non-mutation evidence

Synthetic integration coverage asserts the source database remains byte-for-byte unchanged. The
runtime contract also opens the source database read-only, creates snapshots only under the external
workspace and rejects source/workspace overlap.

## Remaining release evidence

Before promoting the alpha beyond controlled evaluation, record a completed private-archive run:

- terminal run state and evidence manifest;
- elapsed time and stage timings;
- selected worker and batch parameters;
- peak memory and workspace/WAL growth;
- final available/missing/unreadable/size-mismatch counts;
- source database hash before and after the run;
- installer hashes and GitHub provenance verification.

No private filenames, addresses, subjects, paths or payloads belong in the public evidence record.
