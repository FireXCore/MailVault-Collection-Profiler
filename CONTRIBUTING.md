# Contributing

MailVault Collection Profiler processes evidence. Correctness, source immutability and reproducible
failure behavior have priority over convenience.

## Before contributing

Read:

- [Architecture](docs/ARCHITECTURE.md)
- [Security model](docs/SECURITY_MODEL.md)
- [Adapter contract](docs/ADAPTER_CONTRACT.md)
- [Development](docs/DEVELOPMENT.md)
- [Code of conduct](CODE_OF_CONDUCT.md)

## Development setup

```bash
npm ci
npm run check:release-config
npm run check:docs
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --exclude mailvault-profiler-desktop -- -D warnings
cargo test --workspace --all-features --exclude mailvault-profiler-desktop
npm run type-check
npm run build
```

Desktop changes require the Windows gate:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
```

Installer changes require:

```powershell
npm run tauri -- build
```

## Pull request requirements

Explain:

- the invariant being added or changed;
- source-archive mutation risk;
- failure, rollback and recovery behavior;
- progress-accounting impact;
- schema or migration impact;
- compatibility impact;
- privacy and logging impact;
- tests and benchmark evidence.

Every behavior change needs a test at the lowest useful layer. Windows-specific filesystem behavior
must have Windows CI coverage.

## Scope control

Do not add OCR, extraction, container expansion, embeddings, LLMs, graph analysis or procurement
classification to the physical-inventory slices without an approved architecture change.

Do not weaken source compatibility or mutation-protection tests to accept incorrect behavior.

## Fixtures and screenshots

Use synthetic fixtures. Follow [Screenshot policy](docs/SCREENSHOTS.md). Real archive content is not
acceptable in commits, issues or pull requests.
