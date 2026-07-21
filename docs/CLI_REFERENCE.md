# CLI reference

Executable name:

```text
mailvault-profiler.exe
```

Global behavior:

- success exit code: `0`;
- profiler error exit code: `2`;
- structured errors are printed to `stderr`;
- profile progress is printed as JSON Lines to `stderr`;
- final command results are printed to `stdout`.

![CLI workflow](assets/screenshots/05-cli-workflow.png)

## `preflight`

```powershell
mailvault-profiler.exe preflight --archive <PATH> [--json]
```

Options:

| Option | Required | Description |
|---|---:|---|
| `--archive <PATH>` | yes | MailVault archive root |
| `--json` | no | Print the complete report as formatted JSON |

Without `--json`, the command prints the archive, compatibility state, schema, source counts and
each preflight check. An incompatible report returns exit code `2`.

Example:

```powershell
.\target\release\mailvault-profiler.exe preflight `
  --archive "D:\MailVault-Demo" `
  --json > preflight.json
```

## `snapshot`

```powershell
mailvault-profiler.exe snapshot \
  --archive <PATH> \
  --workspace <PATH> \
  [--run-id <ID>]
```

Options:

| Option | Required | Description |
|---|---:|---|
| `--archive <PATH>` | yes | MailVault archive root |
| `--workspace <PATH>` | yes | Separate profiler workspace |
| `--run-id <ID>` | no | Caller-supplied run identity; UUIDv7 generated when omitted |

This command creates only the consistent source snapshot and prints the snapshot result as JSON.
Progress events are emitted to `stderr`.

## `profile`

```powershell
mailvault-profiler.exe profile \
  --archive <PATH> \
  --workspace <PATH> \
  [--batch-size <ROWS>] \
  [--file-stat-workers <COUNT>] \
  [--file-stat-batch-size <OBJECTS>]
```

Options:

| Option | Default | Description |
|---|---:|---|
| `--archive <PATH>` | required | MailVault archive root |
| `--workspace <PATH>` | required | Separate profiler workspace |
| `--batch-size <ROWS>` | `1000` | Metadata inventory batch size |
| `--file-stat-workers <COUNT>` | `0` | `0` selects the conservative provisional automatic policy |
| `--file-stat-batch-size <OBJECTS>` | `512` | Durable file-stat batch/checkpoint size |

Recommended capture:

```powershell
.\target\release\mailvault-profiler.exe profile `
  --archive "D:\MailVault-Demo" `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --batch-size 1000 `
  --file-stat-workers 0 `
  --file-stat-batch-size 512 `
  1> profile-result.json `
  2> profile-progress.jsonl
```

## Progress JSON Lines

Each line is a serialized `ProgressEvent` containing:

```text
runId
sequence
stage
stageState
unit
completedItems / totalItems
completedBytes / totalBytes
elapsedMs
instantThroughput / smoothedThroughput
etaMs
activeWorkers
queueDepth
warnings / errors
currentObjectDisplay
checkpointSequence
```

Stages currently used by the physical profile include:

```text
preflight
source_snapshot
metadata_inventory
reconciliation
file_stat
aggregation
publish
```

`fixity` and `format_identification` exist in the domain vocabulary but are not active capabilities
in `0.1.0-alpha.3`.

## Evidence wrapper

The repository includes a Windows PowerShell wrapper that builds the release CLI, runs JSON
preflight and profile, records elapsed time, hashes output files and writes `run-manifest.json`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\run-real-archive-profile.ps1 `
  -ArchiveRoot "D:\MailVault-Demo" `
  -WorkspaceRoot "D:\MailVault-Profiler-Workspace" `
  -EvidenceRoot "D:\MailVault-Profiler-Evidence" `
  -InventoryBatchSize 1000 `
  -FileStatWorkers 0 `
  -FileStatBatchSize 512
```

Use `-SkipBuild` only when the exact intended release CLI has already been built in the current
working tree.

## Workspace and review commands — alpha.3

Inspect without modifying:

```powershell
.\target\release\mailvault-profiler.exe workspace inspect `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --json
```

List persisted runs:

```powershell
.\target\release\mailvault-profiler.exe runs list `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --json
```

List findings with stable server-side filters:

```powershell
.\target\release\mailvault-profiler.exe findings list `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --run "<run-id>" `
  --category requires_attention `
  --review-status unreviewed `
  --limit 100 `
  --json
```

Show detail and review history:

```powershell
.\target\release\mailvault-profiler.exe findings show `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --run "<run-id>" `
  --finding "<finding-id>" `
  --json
```

Write review state. `--allow-migration` is required only when the selected workspace schema is older:

```powershell
.\target\release\mailvault-profiler.exe findings review `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --run "<run-id>" `
  --finding "<finding-id>" `
  --status needs_investigation `
  --note "Verify against the retained backup." `
  --allow-migration
```

Other write commands are `findings clear` and `findings note`. Only the process holding the workspace writer lock may use them.

Sanitized export:

```powershell
.\target\release\mailvault-profiler.exe export sanitized-summary `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --run "<run-id>" `
  --output ".\sanitized-summary.json"
```

Use a `.csv` output path to export the sanitized finding rows instead of the aggregate JSON document.
