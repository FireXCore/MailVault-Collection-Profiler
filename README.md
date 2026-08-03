<div align="center">

![MailVault Collection Profiler](docs/assets/social-preview.png)

# MailVault Collection Profiler

**Local-first, read-only physical inventory and exact file-format evidence for MailVault archives.**

[![CI](https://github.com/FireXCore/mailvault-collection-profiler/actions/workflows/ci.yml/badge.svg)](https://github.com/FireXCore/mailvault-collection-profiler/actions/workflows/ci.yml)
[![CodeQL](https://github.com/FireXCore/mailvault-collection-profiler/actions/workflows/codeql.yml/badge.svg)](https://github.com/FireXCore/mailvault-collection-profiler/actions/workflows/codeql.yml)
[![Release](https://img.shields.io/github/v/release/FireXCore/mailvault-collection-profiler?include_prereleases&label=release)](https://github.com/FireXCore/mailvault-collection-profiler/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/platform-Windows%20x64-2f80ed.svg)](docs/INSTALLATION_WINDOWS.md)

[فارسی](README_FA.md) · [Download](https://github.com/FireXCore/mailvault-collection-profiler/releases) · [Getting started](docs/GETTING_STARTED.md) · [Exact formats](docs/FORMAT_IDENTIFICATION.md) · [CLI](docs/CLI_REFERENCE.md) · [Security](SECURITY.md)

</div>

> **Final feature pre-release:** `0.1.0-alpha.6` combines continuous incremental profiling,
> same-run crash recovery and automatic exact-format identification. Windows builds bundle pinned
> Siegfried `1.11.6` and PRONOM `v124`. MailVault remains read-only and installers are unsigned.

## What the product does

MailVault preserves canonical email evidence. Collection Profiler creates a rebuildable technical
index over that evidence:

```text
MailVault archive (read-only)
  → consistent SQLite snapshot
  → full metadata reconciliation from an immutable snapshot
  → cumulative SHA-256 identity and occurrence history
  → selective file-stat verification and retry
  → automatic exact-format identification with PUID evidence
  → durable incremental generations and searchable desktop/CLI views
```

The profiler does not download mail, mutate MailVault, execute attachments, perform OCR, expand
archives, classify procurement documents or write to RMS.

## Real collection baseline

The architecture and performance envelope are based on the supplied production collection, not
sample-only data:

| Metric | Recorded |
|---|---:|
| Archive scale | approximately 20–30 GB |
| Messages | 17,296 |
| MIME parts | 54,450 |
| Content objects | 13,684 |
| Content occurrences | 22,068 |
| Message relationships | 12,115 |
| Unique blob bytes | 6,467,253,277 |
| Physical inventory findings | 1,484 |
| Physical inventory warnings / errors | 2 / 0 |

Alpha 4 completed the Windows quality gate and exact-format run on all 13,684 real objects. Alpha 6
adds schema migration `0007`, continuous generations, update-aware metadata reconciliation, selective
content reuse, same-run recovery and bounded snapshot retention. Publication still requires the Alpha 6
real-runtime acceptance matrix to pass on Windows.

## Implemented capabilities

### Source and physical inventory

- MailVault schema-v3 capability preflight and writer-lock checks.
- Read-only SQLite Online Backup snapshot with source-change detection.
- Streaming inventory of messages, participants, MIME parts, relationships and blobs.
- Exact SHA-256 content identity separated from every message/filename occurrence.
- Missing, unreadable, invalid-locator, non-regular, zero-byte and size-mismatch findings.
- Same-hash/different-name and same-name/different-hash evidence.
- Cursor-paginated inventory, content-object detail and append-only finding review.
- Sanitized summary and finding export without local paths, filenames, addresses or review notes.

### Exact format identification

- One identification job per unique content object, never per duplicate occurrence.
- Pinned Siegfried `1.11.6` executable and PRONOM `v124` signature database.
- Tool and signature SHA-256, version, creation metadata and identifier details recorded per run.
- All matches retained; one deterministic primary assertion selected without hiding ambiguity.
- PUID, format name, format version, MIME, class, evidence basis and warnings persisted.
- Explicit states: identified, unknown, ambiguous, empty, unavailable and tool error.
- Extension evidence recorded only when a safe filename alias was actually evaluated.
- Bounded batch execution, process timeout, output-size limits and adaptive batch isolation.
- Durable checkpoint/resume keyed by configuration fingerprint.
- Exclusive format-stage workspace lock to prevent concurrent writers.
- No archive/container expansion and no attachment rendering.

![Exact format identification dashboard](docs/assets/screenshots/07-exact-format-identification-ready.png)

![Versioned format assertion detail](docs/assets/screenshots/08-format-assertion-detail.png)

![Alpha 4 product boundary](docs/assets/screenshots/09-alpha4-architecture.png)

## Why the sidecar is pinned

Format assertions are evidence. A result is meaningful only when the executable and signature
registry are identifiable and reproducible. The Windows build therefore obtains a specific upstream
release, verifies the GitHub release-asset SHA-256 digest, records the executable/signature hashes,
probes the observed versions and then bundles them as Tauri resources. Runtime identification fails
closed when the required tool or signature version differs.

See [Exact format identification](docs/FORMAT_IDENTIFICATION.md),
[format runbook](docs/FORMAT_IDENTIFICATION_RUNBOOK.md) and
[third-party notices](THIRD_PARTY_NOTICES.md).

## Install

1. Open [GitHub Releases](https://github.com/FireXCore/mailvault-collection-profiler/releases).
2. Download the Windows x64 NSIS installer or MSI.
3. Verify the artifact using `SHA256SUMS.txt`.
4. Keep the MailVault archive, profiler workspace and runtime evidence in separate directories.
5. Stop MailVault write activity before creating a new source snapshot.

```text
E:\MailVault-E
E:\MailVault-Profiler-Alpha6
E:\MailVault-Profiler-Evidence-Alpha6
```

Full instructions: [Windows installation](docs/INSTALLATION_WINDOWS.md).

## CLI workflow

Run the complete initial or incremental pipeline:

```powershell
.\target\release\mailvault-profiler.exe profile `
  --archive "E:\MailVault-E" `
  --workspace "E:\MailVault-Profiler-Alpha6" `
  --siegfried ".\tools\siegfried\windows-x86_64\sf.exe" `
  --signature ".\tools\siegfried\windows-x86_64\default.sig" `
  --retain-successful-snapshots 2
```

Keep the same workspace current after future MailVault syncs:

```powershell
.\target\release\mailvault-profiler.exe profile `
  --archive "E:\MailVault-E" `
  --workspace "E:\MailVault-Profiler-Alpha6" `
  --watch `
  --poll-seconds 300
```

The standalone `formats` commands remain diagnostic tools. The normal `profile` command now runs exact
format identification automatically before the generation can succeed.

Create or reopen the physical inventory first:

```powershell
.\target\release\mailvault-profiler.exe workspace inspect `
  --workspace "E:\MailVault-Profiler-Alpha6" `
  --json

.\target\release\mailvault-profiler.exe runs list `
  --workspace "E:\MailVault-Profiler-Alpha6" `
  --json
```

Probe the exact-format toolchain:

```powershell
.\target\release\mailvault-profiler.exe formats probe `
  --siegfried ".\tools\siegfried\windows-x86_64\sf.exe" `
  --signature ".\tools\siegfried\windows-x86_64\default.sig" `
  --json
```

Run exact identification against a completed physical baseline:

```powershell
.\target\release\mailvault-profiler.exe formats identify `
  --workspace "E:\MailVault-Profiler-Alpha6" `
  --run "<physical-profile-run-id>" `
  --siegfried ".\tools\siegfried\windows-x86_64\sf.exe" `
  --signature ".\tools\siegfried\windows-x86_64\default.sig" `
  --batch-size 2048 `
  --workers 0 `
  --timeout-seconds 900 `
  --resume true `
  --allow-migration `
  1> format-result.json `
  2> format-progress.jsonl
```

Inspect aggregate and object-level results:

```powershell
.\target\release\mailvault-profiler.exe formats summary `
  --workspace "E:\MailVault-Profiler-Alpha6" `
  --run "<physical-profile-run-id>" `
  --json

.\target\release\mailvault-profiler.exe formats list `
  --workspace "E:\MailVault-Profiler-Alpha6" `
  --run "<physical-profile-run-id>" `
  --state ambiguous `
  --json
```

The CLI writes progress JSONL to `stderr` and the final result to `stdout`.

## Build from source

Requirements:

- Rust `1.97.1` from `rust-toolchain.toml`;
- Node.js `24.x` and npm `11+`;
- Visual Studio/Build Tools with Desktop development with C++;
- Windows SDK and WebView2 Runtime.

```powershell
npm ci
.\scripts\install-siegfried.ps1
.\scripts\quality-alpha6.ps1
npm run tauri:desktop:bundle
```

The installer build embeds the verified sidecar resources. Generated `sf.exe`, `default.sig` and
`tool-manifest.json` are intentionally not committed to source control.

## Validation status

The trusted Alpha 4 predecessor passed 36 Rust tests, strict Clippy, TypeScript, Vite, native Tauri and
the full private exact-format run. Alpha 6 static package gates validate version alignment,
documentation/privacy, JSON/TOML and SQLite schema migration.

Before publishing Alpha 6, run:

```powershell
.\scripts\quality-alpha6.ps1 -BuildInstallers
.\scripts\validate-alpha6-real.ps1 ...
.\scripts\validate-alpha6-delta.ps1 ...
.\scripts\validate-alpha6-crash.ps1 ...
.\scripts\validate-alpha6-soak.ps1 -Cycles 100 ...
.\scripts\validate-alpha6-rebuild-equivalence.ps1 ...
```

See [Alpha 6 validation contract](docs/VALIDATION_0.1.0-alpha.6.md),
[continuous profiling](docs/CONTINUOUS_PROFILING.md) and
[real acceptance runbook](docs/ALPHA6_REAL_ACCEPTANCE_FA.md).

## Documentation

- [Documentation index](docs/INDEX.md)
- [Getting started](docs/GETTING_STARTED.md)
- [Windows installation](docs/INSTALLATION_WINDOWS.md)
- [GUI guide](docs/GUI_GUIDE.md)
- [CLI reference](docs/CLI_REFERENCE.md)
- [Exact format identification](docs/FORMAT_IDENTIFICATION.md)
- [Exact format runbook](docs/FORMAT_IDENTIFICATION_RUNBOOK.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Workspace format](docs/WORKSPACE_FORMAT.md)
- [Security model](docs/SECURITY_MODEL.md)
- [Privacy](docs/PRIVACY.md)
- [Real archive baseline](docs/REAL_ARCHIVE_BASELINE.md)
- [Release process](docs/RELEASE_PROCESS.md)
- [Alpha 6 release notes](docs/releases/v0.1.0-alpha.6.md)

## Scope intentionally deferred

- interrupted physical-profile resume and pause/cancel controls;
- full payload fixity re-hash;
- archive/container expansion;
- JHOVE structural validation;
- text extraction and selective OCR;
- semantic search, embeddings and LLM processing;
- procurement classification and RMS writes;
- public code-signing and automatic updates.

## License

Apache License 2.0. See [LICENSE](LICENSE), [NOTICE](NOTICE) and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
