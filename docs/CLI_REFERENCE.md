# CLI reference

The CLI executable is `mailvault-profiler.exe`. Structured progress is written to `stderr`; final
machine-readable results are written to `stdout`.

## Source and physical-profile commands

```text
preflight --archive <path> [--json]
snapshot --archive <path> --workspace <path> [--run-id <id>]
profile --archive <path> --workspace <path> [options]
workspace inspect --workspace <path> [--json]
runs list --workspace <path> [--json]
findings list|show|review|clear|note ...
export sanitized-summary ...
```

## `formats probe`

Validates the executable/signature pair and prints versioned tool identity.

```powershell
mailvault-profiler.exe formats probe `
  --siegfried <sf.exe> `
  --signature <default.sig> `
  [--workers 0] `
  [--json]
```

The command fails when the pinned Siegfried or PRONOM version is not observed.

## `formats identify`

```powershell
mailvault-profiler.exe formats identify `
  --workspace <workspace> `
  --run <physical-profile-run-id> `
  [--siegfried <sf.exe>] `
  [--signature <default.sig>] `
  [--batch-size 2048] `
  [--workers 0] `
  [--timeout-seconds 900] `
  [--resume true|false] `
  [--allow-migration]
```

Notes:

- `--allow-migration` is required to upgrade an older workspace schema;
- `--workers 0` selects the conservative auto policy;
- batch size must be 1–10,000;
- timeout must be at least 30 seconds;
- resume requires an identical configuration fingerprint;
- `stdout` contains `FormatIdentificationResult` JSON;
- `stderr` contains `ProgressEvent` JSONL.

## `formats summary`

```powershell
mailvault-profiler.exe formats summary `
  --workspace <workspace> `
  --run <physical-profile-run-id> `
  [--json]
```

Returns totals for identified, unknown, ambiguous, empty, unavailable, tool errors, extension
mismatches, PUIDs, objects and bytes.

## `formats list`

```powershell
mailvault-profiler.exe formats list `
  --workspace <workspace> `
  --run <physical-profile-run-id> `
  [--state identified|unknown|ambiguous|empty|skipped_unavailable|tool_error] `
  [--puid fmt/276] `
  [--mismatch-only] `
  [--search <text>] `
  [--limit 100] `
  [--json]
```

The desktop API additionally supports cursor continuation by SHA-256. CLI Alpha 4 returns the first
requested page.

## Exit behavior

`0` means the command completed successfully. Non-zero exits serialize an `ErrorReport` to stderr.
A partially committed format run is not reported as complete and may be resumed when eligible.
