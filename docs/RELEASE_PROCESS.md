# Release process

This document is the maintainer procedure for producing a reviewable Windows release. Public
release instructions for users are in [Windows installation](INSTALLATION_WINDOWS.md). The detailed
GitHub configuration procedure in Persian is in
[GitHub publishing guide](GITHUB_PUBLISHING_GUIDE_FA.md).

## Release principles

- A release is built from an annotated immutable tag.
- The tag must equal `v` plus the public application SemVer.
- CI, CodeQL, documentation/privacy scrub and the Windows desktop gate must be green.
- The GitHub workflow creates a **draft** release. A maintainer reviews and publishes it manually.
- Installers are unsigned until the release notes explicitly state otherwise.
- No runtime evidence, source archive, profiler database, logs, PDB files or private screenshots are
  attached to a public release.

## 1. Prepare the release branch

```cmd
git checkout main
git pull --ff-only
git status --short
```

The working tree must be clean.

Update these version surfaces together:

```text
package.json
apps/desktop/package.json
Cargo.toml workspace.package.version
apps/desktop/src-tauri/tauri.conf.json
apps/desktop/src-tauri/tauri.conf.json bundle.windows.wix.version
```

The public version may contain a prerelease suffix. MSI uses a separate numeric four-part version.
For `0.1.0-alpha.3`, the MSI version is `0.1.0.3`.

## 2. Write release notes and changelog

Create:

```text
docs/releases/v<version>.md
```

The notes must include:

- release status and intended audience;
- implemented capabilities;
- safety and privacy guarantees;
- compatibility boundary;
- known limitations;
- unsigned-installer warning where applicable;
- upgrade/rebuild instructions;
- exact verification commands.

Update `CHANGELOG.md` with the same material at a shorter level.

## 3. Run the local Windows gate

Open an x64 Visual Studio Developer Command Prompt:

```cmd
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64
cd /d D:\mailvault-collection-profiler
npm ci
node scripts\check-tag-version.cjs v0.1.0-alpha.3
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
npm run tauri:desktop:bundle
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\generate-release-checksums.ps1
```

The quality script validates:

1. release-version coherence;
2. documentation, links, screenshots and privacy markers;
3. Rust formatting;
4. Clippy for core and desktop targets;
5. Rust tests including the Windows snapshot regression;
6. locked npm install;
7. TypeScript and Vite production build.

## 4. Smoke-test local bundles

Install the NSIS artifact on a clean VM or a non-development Windows account. Verify:

1. application starts without development tools;
2. a synthetic schema-v3 fixture passes preflight;
3. workspace overlap is rejected;
4. a short profile completes;
5. source fixture hashes are unchanged;
6. uninstallation succeeds;
7. no archive payload is copied into the evidence folder.

Repeat the install/uninstall check with the MSI package.

## 5. Commit and tag

```cmd
git add .
git commit -m "Release MailVault Collection Profiler 0.1.0-alpha.3"
git push origin main
git tag -a v0.1.0-alpha.3 -m "MailVault Collection Profiler 0.1.0-alpha.3"
git push origin v0.1.0-alpha.3
```

The tag starts `.github/workflows/release.yml`.

## 6. Automated draft release

The workflow:

- validates tag/version consistency;
- runs the complete Windows quality gate;
- builds NSIS and MSI through the official Tauri action;
- creates `SHA256SUMS.txt`;
- creates a draft prerelease;
- uploads installers and checksums;
- creates provenance attestations for public repositories.

## 7. Review the draft

Check:

- exact tag and prerelease status;
- release notes are rendered correctly;
- one x64 NSIS setup executable and one MSI exist;
- filenames contain the application version and architecture;
- checksums match locally downloaded assets;
- no debug symbols, runtime evidence or private source package is attached;
- release text does not claim signing, performance or compatibility that was not verified.

Verification:

```powershell
Get-FileHash .\MailVault-Collection-Profiler_*.exe -Algorithm SHA256
Get-Content .\SHA256SUMS.txt

gh attestation verify .\MailVault-Collection-Profiler_*.exe `
  --repo FireXCore/mailvault-collection-profiler
```

## 8. Publish or roll back

Publish only after installer smoke tests pass. If a defect is found before publication, delete the
draft and tag, fix on `main`, rerun the gates and create a new tag. Never move a published tag.
After publication, fix defects in a new version and retain the original release as immutable history.

## 9. Post-release checks

- Download and install the public asset once more.
- Confirm badges and release links in `README.md` resolve.
- Confirm GitHub's source archives contain no generated directories or private paths.
- Open the next milestone and move deferred work to the roadmap.
- Record the tag, commit SHA, installer SHA-256 and publication time in the release record.
