# Implementation status — 0.1.0-alpha.4

## Implemented

- read-only MailVault v3 preflight and SQLite snapshot;
- physical inventory, content/occurrence separation and file-stat checks;
- findings, append-only review, sanitized export and explorer UI;
- exact format core/storage/engine/sidecar/CLI/desktop modules;
- pinned Siegfried/PRONOM acquisition, verification and Tauri resource bundling;
- schema migration 6 and format filtering/progress/resume contract;
- publication-ready English/Persian documentation and sanitized screenshots.

## Validated

- frontend type/build;
- Rust syntax parse;
- real Alpha 3 profiler DB migration and count preservation;
- SQLite integrity after migration;
- synthetic format projection;
- Windows PowerShell 5.1 sidecar installation and independent verification;
- pinned Siegfried `1.11.6` and PRONOM `v124` runtime identity;
- Windows-safe home-relative signature argument contract;
- five `profiler-format-siegfried` unit tests;
- workspace Rust formatting check.

## Required before runtime-green release

- rerun strict workspace Clippy after the structural refactor;
- run the complete Rust workspace test suite on CI;
- Windows Tauri build and installers;
- verified sidecar install in CI;
- private real-archive exact-format run;
- benchmark, resume-interruption and source-nonmutation evidence;
- sanitized aggregate format report.

## Deferred

Physical-profile pause/resume, full fixity, container expansion, JHOVE, extraction, OCR, semantic
search, procurement classification and RMS integration.
