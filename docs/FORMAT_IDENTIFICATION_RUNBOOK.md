# Exact format identification runbook

## Preconditions

- Alpha 3 or newer physical profile completed successfully.
- Workspace is backed up before first schema migration.
- MailVault archive is mounted at the same source root recorded by the workspace.
- No second profiler writer is using the workspace.
- Sufficient free space exists for profiler database growth and temporary batch metadata.
- Pinned Siegfried resources are installed and verified.

## 1. Update and build

```powershell
git checkout main
git pull --ff-only
npm ci
.\scripts\install-siegfried.ps1
.\scripts\verify-siegfried.ps1
cargo build --release -p mailvault-profiler-cli --locked
```

The verifier intentionally passes `default.sig` relative to the configured Siegfried home. This is
required on Windows because the upstream `1.11.6` JSON header does not escape backslashes in an
absolute signature path. A successful probe must therefore produce parseable JSON before any
workspace migration or real-archive run begins.

## 2. Back up the workspace

```powershell
$Workspace = "E:\MailVault-Profiler-Alpha4"
$Backup = "E:\MailVault-Profiler-Alpha4-PreMigration.zip"
Compress-Archive -Path "$Workspace\*" -DestinationPath $Backup -Force
Get-FileHash $Backup -Algorithm SHA256
```

## 3. Inspect and migrate explicitly

```powershell
.\target\release\mailvault-profiler.exe workspace inspect `
  --workspace $Workspace `
  --json
```

The `formats identify` command requires `--allow-migration` when an older workspace schema must be
upgraded. Do not bypass the backup step.

## 4. Find the physical baseline run

```powershell
.\target\release\mailvault-profiler.exe runs list `
  --workspace $Workspace `
  --json
```

Use a completed physical-profile run with the expected 17,296-message baseline.

## 5. Probe the pinned toolchain

```powershell
$Tool = ".\tools\siegfried\windows-x86_64\sf.exe"
$Signature = ".\tools\siegfried\windows-x86_64\default.sig"

.\target\release\mailvault-profiler.exe formats probe `
  --siegfried $Tool `
  --signature $Signature `
  --json |
  Tee-Object -FilePath ".\format-tool-probe.json"
```

Require:

```text
toolVersion = 1.11.6
signatureVersion = v124
executableSha256 present
signatureSha256 present
```

## 6. Run against the real archive

```powershell
$RunId = "<physical-profile-run-id>"
$Evidence = "E:\MailVault-Profiler-Evidence-Alpha4"
New-Item -ItemType Directory -Path $Evidence -Force | Out-Null

.\target\release\mailvault-profiler.exe formats identify `
  --workspace $Workspace `
  --run $RunId `
  --siegfried $Tool `
  --signature $Signature `
  --batch-size 2048 `
  --workers 0 `
  --timeout-seconds 900 `
  --resume true `
  --allow-migration `
  1> "$Evidence\format-result.json" `
  2> "$Evidence\format-progress.jsonl"

$LASTEXITCODE
```

Exit code must be `0` before treating the format run as complete.

## 7. Inspect summary and high-risk queues

```powershell
.\target\release\mailvault-profiler.exe formats summary `
  --workspace $Workspace `
  --run $RunId `
  --json |
  Tee-Object -FilePath "$Evidence\format-summary.json"

.\target\release\mailvault-profiler.exe formats list `
  --workspace $Workspace `
  --run $RunId `
  --state tool_error `
  --json |
  Set-Content "$Evidence\format-tool-errors.json"

.\target\release\mailvault-profiler.exe formats list `
  --workspace $Workspace `
  --run $RunId `
  --state ambiguous `
  --json |
  Set-Content "$Evidence\format-ambiguous.json"

.\target\release\mailvault-profiler.exe formats list `
  --workspace $Workspace `
  --run $RunId `
  --mismatch-only `
  --json |
  Set-Content "$Evidence\format-extension-mismatches.json"
```

These files are private unless separately sanitized.

## 8. Acceptance checks

- run state is `succeeded`;
- completed objects equals total content objects;
- exactly one known missing object remains `skipped_unavailable`;
- exactly one zero-byte object remains `empty`;
- no path escapes or source mutation findings;
- every identified/ambiguous observation has stored tool/signature identity;
- no incomplete batch is counted as complete;
- source MailVault database and blob tree remain unchanged;
- unknown, ambiguous and tool-error counts are reviewed before parser planning.

Do not prescribe expected identified/unknown/ambiguous counts before the real run.

## 9. Resume after interruption

Re-run the same command with identical tool, signature and options. The stage acquires the workspace
lock, verifies the configuration fingerprint and continues after the last durable SHA-256
checkpoint. A changed fingerprint starts a new run; it must not reuse prior completion claims.

## 10. Public evidence

Publish only aggregate sanitized outputs and toolchain provenance. Never publish:

- profiler or snapshot SQLite databases;
- real filenames, subjects, domains or local paths;
- content hashes when they identify private documents;
- raw progress logs;
- review notes;
- tool-error messages containing source paths.
