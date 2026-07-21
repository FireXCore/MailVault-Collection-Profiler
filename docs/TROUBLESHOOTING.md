# Troubleshooting

## npm downloads an internal OpenAI/Artifactory URL

Symptom:

```text
<private-registry-host>
ETIMEDOUT 10.x.x.x:443
```

Cause: a source package was published with build-environment tarball URLs in `package-lock.json`.
The corrected repository uses the public npm registry and `.npmrc`.

Verify:

```powershell
npm config get registry
Select-String package-lock.json -Pattern "private-registry-host"
```

Expected registry:

```text
https://registry.npmjs.org/
```

## `link.exe` not found

Cargo was started from a normal shell without the MSVC environment.

```cmd
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64
where link
where cl
```

## `LNK1181: cannot open input file 'kernel32.lib'`

The Windows SDK library paths are missing or the SDK installation is incomplete.

```cmd
echo %WindowsSdkDir%
echo %WindowsSDKVersion%
echo %LIB%
where /r "C:\Program Files (x86)\Windows Kits\10\Lib" kernel32.lib
```

Repair or add the **Desktop development with C++**, MSVC x64/x86 and Windows SDK components.

## Snapshot durable sync returns `Access is denied`

Older source packages reopened the completed snapshot with a read-only file handle before calling
`sync_all`. Windows `FlushFileBuffers` requires a write-capable handle. Use the corrected source
where the file is opened with `OpenOptions::write(true)` and covered by a Windows regression test.

## Quality gate continues after a Cargo failure

Use the corrected fail-fast `scripts/quality.ps1`. Every native command is checked through
`$LASTEXITCODE`. The script invokes `npm.cmd` directly on Windows to avoid the PowerShell shim under
StrictMode.

## `Channel<ProgressEvent>` does not implement `Debug`

Use the corrected desktop bridge without `#[derive(Debug)]` on `ChannelProgressSink`.

## MSI prerelease identifier error

Tauri app SemVer can remain:

```text
0.1.0-alpha.4
```

WiX/MSI version must remain numeric:

```text
0.1.0.4
```

Run:

```powershell
npm run check:release-config
```

## Active writer lock

Stop MailVault synchronization, import or maintenance through its normal control path. Do not delete
a lock file blindly. A real writer can invalidate snapshot consistency.

## Workspace overlap

Invalid:

```text
D:\MailVault-Demo\Profiler
```

Valid:

```text
D:\MailVault-Demo
D:\MailVault-Profiler-Workspace
```

## Preflight reports a newer schema

The adapter fails closed on an unsupported newer schema. Do not change tests to accept it. Add a
reviewed adapter contract and fixtures for the new source version.

## Low disk space

The workspace needs room for:

- source SQLite snapshot;
- profiler database;
- SQLite WAL and temporary files;
- checkpoints.

```powershell
Get-PSDrive D
```

## SmartScreen warning

The alpha is unsigned. Verify SHA-256 and GitHub provenance. Code signing is a separate release
requirement and cannot be replaced by bypass instructions.

## Desktop cannot reopen a completed workspace

This is a known limitation of the current alpha, not a corrupted workspace. Preserve evidence and
review the current run before closing the application.

## Workspace reopen errors

- `WORKSPACE_MIGRATION_REQUIRED`: reopen read-write and explicitly approve migration.
- `WORKSPACE_LOCKED` or read-only mode: close the other profiler process; do not delete the lock file as a substitute for releasing the OS lock.
- `WORKSPACE_SCHEMA_NEWER_THAN_APPLICATION`: install a compatible newer profiler; do not downgrade the database.
- `REVIEW_HISTORY_INTEGRITY_FAILURE`: keep browsing read-only, preserve the workspace and backup, and investigate before any repair.
- `SANITIZED_EXPORT_FAILED`: choose a new `.json` or `.csv` path outside MailVault.
