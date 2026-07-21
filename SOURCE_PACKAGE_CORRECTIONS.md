# Source package corrections

This corrected `0.1.0-alpha.2` source package closes defects found during first-run validation on
Windows. The application version remains unchanged because these are source-package corrections,
not a new feature release.

## Corrections

1. **Windows SQLite snapshot durable sync**
   - Replaced read-only `File::open(...).sync_all()` with a write-capable `OpenOptions` handle.
   - Added a Windows-only regression test.
   - Added Windows CI execution of the end-to-end physical profile pipeline.

2. **PowerShell quality gate fail-fast and npm shim compatibility**
   - Every Cargo and npm command now has an explicit native exit-code check.
   - A failed Rust test stops the script before npm/type-check/build can obscure the gate result.
   - Windows runs `npm.cmd` directly so the npm PowerShell shim cannot inherit `Set-StrictMode`
     and fail on missing invocation metadata such as `Statement`.

3. **Public npm registry portability**
   - Replaced internal build-environment tarball URLs in `package-lock.json`.
   - Added project-level `.npmrc` pointing to `https://registry.npmjs.org/`.

4. **Tauri desktop bridge compilation**
   - Removed an invalid `Debug` derive from `ChannelProgressSink`; Tauri's
     `Channel<ProgressEvent>` does not implement `Debug`.
   - Extended the Windows PowerShell quality gate to compile and lint every desktop target after
     the frontend production build.

5. **MSI-safe prerelease version mapping**
   - Preserved the application/source version as `0.1.0-alpha.2`.
   - Set WiX/MSI `ProductVersion` explicitly to `0.1.0.2`, where the fourth numeric field maps
     to alpha build `2`.
   - Added a release-configuration gate that rejects version drift and invalid MSI numeric limits.

## Required Windows validation

Run from an x64 Visual Studio Developer Command Prompt:

```cmd
cd /d D:\mailvault-collection-profiler
cargo test -p profiler-engine --test profile_pipeline -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
```

The gate is successful only when the Rust pipeline test reports `ok` and the script exits with code
zero.
