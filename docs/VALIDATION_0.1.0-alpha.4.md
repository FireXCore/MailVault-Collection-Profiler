# Validation evidence — 0.1.0-alpha.4

## Scope

This document records the completed release evidence for exact format identification in
MailVault Collection Profiler `0.1.0-alpha.4`. Private databases, paths, filenames and raw runtime
evidence are not published.

## Pinned format-identification contract

```text
Siegfried 1.11.6
PRONOM v124
container expansion disabled
```

The executable and signature SHA-256 values are recorded in the private acceptance evidence and in
the profiler run metadata.

## Completed Windows quality gate

| Gate | Result |
|---|---|
| Release/version configuration | passed |
| Documentation/privacy/local-link gate | passed |
| Rust formatting | passed |
| Strict Clippy with `-D warnings` | passed |
| Rust tests | passed, 36 |
| TypeScript strict type-check | passed |
| Vite production build | passed |
| Native Tauri desktop compilation | passed |
| Rust syntax validation | passed, 34 source files |
| npm audit | passed, 0 vulnerabilities |
| Pinned Siegfried installation | passed |
| Independent Siegfried verification | passed |
| Siegfried version | `1.11.6` |
| PRONOM signature | `v124` |
| Windows-safe home-relative signature contract | passed |

## Workspace migration

A private copy of the real Alpha 3 profiler workspace was migrated from schema 5 to schema 6.

| Check | Result |
|---|---|
| compatibility after migration | compatible |
| schema version | 6 |
| migration required | false |
| active workspace lock after completion | false |
| last migrated by | `0.1.0-alpha.4` |
| content objects preserved | 13,684 |
| source/snapshot clone hashes | matched |

## Full real-archive exact-format run

| Metric | Result |
|---|---:|
| Baseline run | `019f83f1-1687-7032-be61-5a9a1085ad51` |
| Format run | `019f84e2-037f-7271-b5d7-e814314dd5ba` |
| State | `succeeded` |
| Total objects | 13,684 |
| Eligible objects | 13,682 |
| Completed objects | 13,684 |
| Total bytes | 6,466,878,455 |
| Completed bytes | 6,466,878,455 |
| Identified | 13,636 |
| Unknown | 46 |
| Ambiguous | 0 |
| Empty | 1 |
| Skipped unavailable | 1 |
| Tool errors | 0 |
| Extension mismatches | 51 |
| Distinct PUIDs | 64 |

State accounting is complete:

```text
13,636 identified
+   46 unknown
+    0 ambiguous
+    1 empty
+    1 skipped unavailable
+    0 tool errors
= 13,684 total objects
```

The canonical MailVault database SHA-256 matched before and after the run.

## Evidence boundary

The retained private evidence bundle includes:

- workspace inspection after migration;
- exact-format summary;
- unknown, extension-mismatch and tool-error review queues;
- source SHA-256 before/after evidence;
- acceptance JSON;
- SHA-256 manifest for the evidence files.

Raw archives, databases, filenames, local paths and profiler progress logs are excluded from the
public repository and release.

## Release status

The implementation, complete Windows quality gate, schema migration and full private real-archive
exact-format run are **runtime green**. Public installers remain unsigned and are distributed as an
Alpha prerelease for controlled technical evaluation.
