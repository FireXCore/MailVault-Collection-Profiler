# Getting started

This guide performs one controlled profile of a MailVault archive on Windows without modifying the
canonical source.

## 1. Understand the three directories

Use separate roots:

```text
D:\MailVault-Demo                 canonical source archive
D:\MailVault-Profiler-Workspace   disposable derived profiler data
D:\MailVault-Profiler-Evidence    run manifests and exported logs
```

The workspace and evidence roots must not be equal to, inside, or parents of the archive root.

## 2. Prepare the source archive

Before preflight:

- stop MailVault synchronization, import, verification and maintenance tasks;
- confirm the archive drive is stable and has no pending disconnect;
- do not delete a lock file manually;
- do not move database or object-store files;
- ensure the profiler process can read the archive and write to the workspace.

Expected MailVault layout:

```text
<archive>\database\mailvault.sqlite3
<archive>\objects\raw\sha256\...
<archive>\objects\blobs\sha256\...
<archive>\state\...
```

## 3. Run read-only preflight

Desktop:

1. Open **Collection setup**.
2. Select the archive root.
3. Choose **Run read-only preflight**.

CLI:

```powershell
.\target\release\mailvault-profiler.exe preflight `
  --archive "D:\MailVault-Demo"
```

Proceed only when:

- `compatible` is true;
- schema version is `3`;
- writer lock is absent;
- required path, schema and integrity checks pass.

Warnings require review. Failed required checks block profiling.

## 4. Select the profiler workspace

Choose an empty or dedicated directory outside the source archive:

```text
D:\MailVault-Profiler-Workspace
```

The workspace stores:

- the consistent source database snapshot;
- profiler SQLite database and migrations;
- run checkpoints;
- derived inventory and findings.

It does not need to be preserved as canonical evidence. It can contain sensitive derived metadata.

## 5. Create the physical inventory

Desktop:

1. Select the workspace.
2. Choose **Create physical inventory**.
3. Leave the first-run defaults unchanged.
4. Keep the application and archive drive available until the run reaches its terminal state.

CLI defaults:

```text
inventory batch size: 1000
file-stat workers: 0 (conservative automatic policy)
file-stat batch size: 512
```

Do not increase worker count before measuring the actual storage device. More concurrent file opens
can reduce performance on HDDs, external drives and antivirus-scanned volumes.

## 6. Review the result

### Physical inventory

Search by:

- filename or filename variant;
- full or partial SHA-256;
- source-detected MIME type;
- message subject;
- sender domain.

Filter by availability, size state or finding code. Pagination uses a stable SHA-256 cursor.

### Findings

Review errors first:

- `MISSING_BLOB`;
- `INVALID_BLOB_LOCATOR`;
- unreadable or non-regular objects.

Then review warnings:

- `BLOB_SIZE_MISMATCH`;
- `SAME_HASH_DIFFERENT_NAMES`;
- `SAME_NAME_DIFFERENT_HASHES`;
- zero-byte content evidence.

Opening a finding with a content-object identity displays filename history, message occurrences and
object-level technical evidence.

## 7. Capture runtime evidence

For benchmark or release evidence, use the wrapper:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\run-real-archive-profile.ps1 `
  -ArchiveRoot "D:\MailVault-Demo" `
  -WorkspaceRoot "D:\MailVault-Profiler-Workspace" `
  -EvidenceRoot "D:\MailVault-Profiler-Evidence"
```

Read [Evidence outputs](EVIDENCE_OUTPUTS.md) before sharing any generated file.

## 8. Finish safely

- Record run state, elapsed time, warnings and errors.
- Preserve evidence files only where access is controlled.
- Do not publish filenames, domains, message subjects or local paths without sanitization.
- Keep the canonical archive unchanged.

## Reopen a completed workspace

After restart, choose **Open existing workspace**, select the same workspace directory, inspect compatibility, approve migration only after reviewing the retained-backup notice, select a run, and open it. Profiling does not rerun. Review writes are disabled automatically if another process owns the workspace lock or if history integrity validation fails.
