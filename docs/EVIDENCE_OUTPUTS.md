# Runtime evidence outputs

`run-real-archive-profile.ps1` creates one UTC timestamped evidence directory:

```text
D:\MailVault-Profiler-Evidence\profile-YYYYMMDDTHHMMSSZ\
```

## Files

### `preflight.json`

Complete structured preflight report, including source metrics and compatibility checks.

### `preflight.stderr.log`

Diagnostics written during preflight. It may be empty on a clean run.

### `profile-result.json`

Final structured result from the complete physical profile. It references the profiler run,
snapshot, inventory, reconciliation, file-stat summaries and publication state.

### `profile-progress.jsonl`

One JSON `ProgressEvent` per line. Suitable for timeline reconstruction and performance analysis.

### `run-manifest.json`

Wrapper-level manifest containing:

- schema version;
- profiler version;
- start, finish and elapsed time;
- process exit code;
- archive and workspace leaf names;
- configured batch sizes and workers;
- operating system and processor count;
- byte length and SHA-256 of each captured output file.

## What is not copied

The evidence wrapper does not copy:

- EML files;
- attachment payloads;
- the canonical MailVault database;
- object-store files;
- credentials.

The profiler workspace contains a consistent database snapshot and derived profiler database. Do
not treat the workspace as safe for public sharing.

## Sensitivity

Even without payloads, evidence can expose:

- local directory names;
- archive and workspace leaf names;
- filenames and filename variants;
- message subjects;
- sender domains;
- content SHA-256 values;
- counts and timing information;
- operating-system details.

Sanitize evidence before attaching it to an issue or publishing benchmark results. Prefer aggregate
counts and synthetic fixtures. Never upload a real profiler database to GitHub.

## Integrity verification

```powershell
$manifest = Get-Content .\run-manifest.json -Raw | ConvertFrom-Json
foreach ($file in $manifest.files) {
    $actual = (Get-FileHash $file.name -Algorithm SHA256).Hash.ToLowerInvariant()
    [pscustomobject]@{
        File = $file.name
        Expected = $file.sha256
        Actual = $actual
        Matches = $actual -eq $file.sha256
    }
}
```

## Review data is not original run evidence

Review events and projections live in `profiler.sqlite3`. They do not rewrite `preflight.json`, `profile-result.json`, `profile-progress.jsonl`, `run-manifest.json`, snapshots or an existing `SHA256SUMS.txt`. Sanitized review export is a new derived artifact and should be hashed separately.
