# Windows installation and source build

## Supported target

`0.1.0-alpha.4` is validated for Windows x64 using the MSVC Rust target. The project may compile on
other desktop platforms after installing normal Tauri system libraries, but the canonical native
release gate is Windows.

## Install a release build

1. Open the repository **Releases** page.
2. Download one installer:
   - NSIS: `*-setup.exe`;
   - MSI: `*.msi`.
3. Download `SHA256SUMS.txt`.
4. Verify the selected installer:

```powershell
Get-FileHash ".\MailVault-Collection-Profiler-setup.exe" -Algorithm SHA256
Get-Content ".\SHA256SUMS.txt"
```

5. Install the application.

### Unsigned alpha warning

The current alpha installers are not code-signed. Windows SmartScreen can therefore show an
unknown-publisher warning. This is an explicit release limitation. Do not represent an unsigned
artifact as trusted merely because it was downloaded from a release page. Verify its SHA-256 and,
for GitHub-built artifacts, its build attestation when available.

## Runtime prerequisites

- Windows 10 or Windows 11 x64;
- Microsoft Edge WebView2 Runtime;
- access to the MailVault archive;
- write access to a separate profiler workspace;
- sufficient free space for the database snapshot, profiler database, WAL and checkpoints.

## Build prerequisites

Install:

- Git for Windows;
- Rust through `rustup`;
- Node.js 24.x;
- npm 11+;
- Visual Studio 2026 Community or Build Tools;
- **Desktop development with C++** workload;
- MSVC x64/x86 build tools;
- Windows 10/11 SDK;
- Universal CRT SDK;
- WebView2 Runtime.

## Activate the native toolchain

Use **x64 Native Tools Command Prompt for Visual Studio** or run:

```cmd
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64
```

Verify:

```cmd
where link
where cl
where rc
echo %WindowsSdkDir%
echo %WindowsSDKVersion%
echo %LIB%
```

`LIB` must contain Windows SDK `ucrt\x64` and `um\x64` paths. Confirm:

```cmd
where /r "C:\Program Files (x86)\Windows Kits\10\Lib" kernel32.lib
```

## Install dependencies and run the quality gate

```cmd
cd /d D:\mailvault-collection-profiler
npm ci
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
```

The gate performs:

- release configuration validation;
- documentation and privacy-scrub checks;
- Rust formatting;
- Rust Clippy with warnings denied;
- workspace tests excluding the platform-specific Tauri crate;
- frontend dependency installation, type-check and production build;
- Windows desktop Clippy compilation.

## Build installers

```cmd
npm run tauri:desktop:bundle
```

Expected output directories:

```text
target\release\bundle\nsis
target\release\bundle\msi
```

The public SemVer remains `0.1.0-alpha.4`. WiX/MSI uses the numeric package version `0.1.0.4`
because Windows Installer does not accept an arbitrary prerelease label in ProductVersion.

## Run in development

```cmd
npm run tauri -- dev
```

## Common failures

- `link.exe not found`: Visual Studio developer environment was not loaded.
- `cannot open kernel32.lib`: SDK library paths are missing or the SDK installation is incomplete.
- WebView2 window failure: install or repair WebView2 Runtime.
- WiX `light.exe` failure: verify required Windows optional components and WiX extraction.
- MSI prerelease version error: run `npm run check:release-config` and keep the numeric WiX mapping.

See [Troubleshooting](TROUBLESHOOTING.md).


## Exact-format sidecar for source builds

Before compiling a release bundle, run:

```powershell
.\scripts\install-siegfried.ps1
.\scripts\verify-siegfried.ps1
```

The installer build embeds the verified `sf.exe`, `default.sig` and `tool-manifest.json` as Tauri
resources. Do not commit generated sidecar binaries.
