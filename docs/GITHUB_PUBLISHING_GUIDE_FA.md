# راهنمای کامل انتشار حرفه‌ای پروژه در GitHub

این سند برای مالک مخزن نوشته شده است. خروجی نهایی باید یک مخزن عمومی قابل اعتماد، قابل Build،
قابل Release و بدون افشای اطلاعات آرشیو خصوصی باشد.

## ۱. کنترل نهایی قبل از اولین Push

همه دستورات را داخل **x64 Native Tools Command Prompt for Visual Studio** اجرا کن:

```cmd
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64
cd /d D:\mailvault-collection-profiler
npm ci
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
npm run tauri:desktop:build
```

Gate باید بدون Error تمام شود. سپس وضعیت فایل‌های حساس را کنترل کن:

```cmd
node scripts\check-docs.cjs
npm run check:docs
```

در مخزن نباید این موارد وجود داشته باشد:

- دیتابیس MailVault یا Profiler؛
- فایل EML یا Attachment واقعی؛
- خروجی `target`، `node_modules` یا `dist`؛
- Runtime Evidence خصوصی؛
- مسیرهای واقعی درایو یا User Profile؛
- کلید، Certificate، Password یا Token؛
- Screenshot دارای Taskbar، نام Volume، ایمیل یا نام مشتری.

## ۲. ساخت مخزن GitHub

در حساب `FireXCore` یک مخزن عمومی با این نام بساز:

```text
mailvault-collection-profiler
```

در زمان ساخت مخزن این گزینه‌ها را فعال نکن، چون فایل‌های متناظر از قبل در پروژه وجود دارند:

```text
Add a README
Add .gitignore
Choose a license
```

### About

Description:

```text
Local-first, read-only physical inventory and technical evidence explorer for MailVault archives.
```

Website را فعلاً خالی بگذار مگر صفحه رسمی پروژه آماده باشد.

Topics:

```text
mail-archive
email-forensics
digital-preservation
sqlite
rust
tauri
react
windows
evidence
sha256
local-first
open-source
```

Social preview را از این فایل Upload کن:

```text
docs/assets/social-preview.png
```

## ۳. ایجاد Git و اولین Push

```cmd
cd /d D:\mailvault-collection-profiler

git init
git branch -M main
git add .
git status --short
git commit -m "Release MailVault Collection Profiler 0.1.0-alpha.4"
git remote add origin https://github.com/FireXCore/mailvault-collection-profiler.git
git push -u origin main
```

خروجی `git status --short` قبل از Commit باید فقط فایل‌های مورد انتظار مخزن را نشان دهد. پس از Push:

```cmd
git status --short
```

باید خالی باشد.

## ۳.۱. ساخت Labelهای استاندارد

GitHub CLI را نصب و Login کن:

```cmd
gh auth login
```

سپس Labelهای مورد استفاده Issue Formها، Dependabot و Release Notes را بساز:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\configure-github-labels.ps1 `
  -Repository "FireXCore/mailvault-collection-profiler"
```

این Script با `--force` قابل تکرار است و Labelهای موجود را به تعریف Canonical برمی‌گرداند.

## ۴. تنظیمات General مخزن

مسیر:

```text
Settings → General
```

تنظیمات پیشنهادی:

- Issues: روشن
- Discussions: اختیاری؛ برای Alpha خاموش بماند مگر برنامه پاسخ‌گویی وجود داشته باشد
- Projects: اختیاری
- Wiki: خاموش؛ مستندات Versioned داخل `docs/` نگهداری می‌شوند
- Preserve this repository: روشن در صورت دسترسی
- Allow merge commits: خاموش
- Allow squash merging: روشن
- Allow rebase merging: روشن
- Automatically delete head branches: روشن
- Web-based commit signoff: روشن

## ۵. Ruleset شاخه `main`

مسیر:

```text
Settings → Rules → Rulesets → New branch ruleset
```

نام:

```text
Protect main
```

Target:

```text
main
```

قواعد:

- Restrict deletions
- Block force pushes
- Require a pull request before merging
- Required approvals: حداقل ۱ برای مشارکت خارجی
- Dismiss stale approvals when new commits are pushed
- Require conversation resolution
- Require status checks to pass
- Require branches to be up to date before merging
- Require linear history

Status checkهایی که بعد از اولین اجرای Actions انتخاب می‌شوند:

```text
Rust core
Frontend
Windows desktop
Documentation and privacy scrub
CodeQL
Dependency Review
```

برای کار تک‌نفره می‌توان Bypass را فقط برای Owner نگه داشت، اما Release مستقیم روی `main` نباید روند
عادی باشد.

## ۶. تنظیمات GitHub Actions

مسیر:

```text
Settings → Actions → General
```

- Actions permissions: فقط Actions متعلق به GitHub و Marketplaceهای مورد اعتماد
- Workflow permissions: `Read repository contents and packages permissions`
- Allow GitHub Actions to create and approve pull requests: خاموش

Workflow انتشار به‌صورت صریح فقط برای Job انتشار `contents: write` درخواست می‌کند. بقیه Workflowها
Read-only هستند.

## ۷. تنظیمات امنیت

مسیر:

```text
Settings → Security / Code security
```

فعال کن:

- Dependency graph
- Dependabot alerts
- Dependabot security updates
- Private vulnerability reporting
- Secret scanning
- Push protection
- CodeQL advanced setup از فایل `.github/workflows/codeql.yml`

Dependency Review برای Pull Requestها از Workflow جداگانه اجرا می‌شود. اگر مخزن خصوصی باشد، بعضی
قابلیت‌ها به Plan نیاز دارند.

## ۸. بررسی Community Standards

در صفحه اصلی مخزن:

```text
Insights → Community Standards
```

موارد زیر باید کامل تشخیص داده شوند:

- README
- Code of Conduct
- Contributing
- License
- Security policy
- Issue templates
- Pull request template

## ۹. بررسی Actions پس از اولین Push

مسیر:

```text
Actions
```

این Workflowها باید اجرا یا قابل اجرا باشند:

```text
CI
CodeQL
Dependency Review
```

Release فقط با Tag نسخه اجرا می‌شود.

اگر CI قرمز است، Tag و Release نساز. انتهای Build موفق Frontend به‌تنهایی به معنی Gate سبز نیست.

## ۱۰. آماده‌سازی Release `0.1.0-alpha.4`

نسخه باید در این فایل‌ها هماهنگ باشد:

```text
package.json
apps/desktop/package.json
Cargo.toml
apps/desktop/src-tauri/Cargo.toml
apps/desktop/src-tauri/tauri.conf.json
```

بررسی:

```cmd
npm run check:release-config
node scripts\check-tag-version.cjs v0.1.0-alpha.4
```

Release Notes عمومی در این فایل آماده است:

```text
docs/releases/v0.1.0-alpha.4.md
```

## ۱۱. ساخت Tag و اجرای Release Workflow

```cmd
git checkout main
git pull --ff-only
git status --short
git tag -a v0.1.0-alpha.4 -m "MailVault Collection Profiler 0.1.0-alpha.4"
git push origin v0.1.0-alpha.4
```

Workflow زیر اجرا می‌شود:

```text
.github/workflows/release.yml
```

وظایف آن:

1. تطبیق Tag با Version؛
2. اجرای Release Config و Documentation Gate؛
3. نصب Dependencyها؛
4. Build ویندوز x64؛
5. ساخت NSIS و MSI؛
6. ساخت `SHA256SUMS.txt`؛
7. ساخت Draft GitHub Release؛
8. Upload Installerها و Checksum؛
9. ثبت Build Provenance Attestation در مخزن عمومی.

## ۱۲. بازبینی Draft Release

مسیر:

```text
Releases → Draft release
```

قبل از Publish کنترل کن:

- Tag دقیقاً `v0.1.0-alpha.4` باشد
- گزینه `This is a pre-release` روشن باشد
- Release Notes کامل باشد
- NSIS `-setup.exe` وجود داشته باشد
- MSI وجود داشته باشد
- `SHA256SUMS.txt` وجود داشته باشد
- فایل‌های Debug، PDB، Source Archive خصوصی یا Runtime Evidence Upload نشده باشند
- هیچ Installer به‌عنوان Signed معرفی نشده باشد

## ۱۳. تست Installer دانلودشده

Installer را از خود Draft Release دانلود کن، نه از `target` محلی. سپس:

```powershell
Get-FileHash .\<installer-name>.exe -Algorithm SHA256
Get-Content .\SHA256SUMS.txt
```

در مخزن عمومی دارای Attestation:

```powershell
gh attestation verify .\<installer-name>.exe `
  --repo FireXCore/mailvault-collection-profiler
```

تست Smoke روی سیستم تمیز یا VM:

1. نصب NSIS؛
2. اجرای برنامه؛
3. انتخاب Fixture مصنوعی؛
4. Preflight؛
5. Profile کوتاه؛
6. Uninstall؛
7. کنترل باقی‌نماندن فایل در Source Archive.

## ۱۴. Code Signing ویندوز

نسخه فعلی Unsigned است. برای Release عمومی جدی باید Certificate معتبر تهیه شود. دو مسیر مناسب:

- Certificate سازمانی PFX و Import امن در GitHub Actions Secrets؛
- Azure Artifact Signing / Key Vault با OIDC و بدون نگهداری Password دائمی در Workflow.

هیچ Certificate، PFX، Base64 Certificate یا Password نباید Commit شود. تا قبل از Signing، در Release
Notes باید عبارت `Unsigned development alpha` باقی بماند.

## ۱۵. Publish

بعد از بازبینی:

```text
Publish release
```

پس از Publish:

- لینک Release را در README بررسی کن؛
- Badge نسخه را کنترل کن؛
- Installer و SHA256 را دوباره دانلود و Verify کن؛
- یک Issue تستی با Template باز کن و سپس ببند؛
- Community Standards را دوباره بررسی کن؛
- Tag را جابه‌جا یا بازنویسی نکن.

## ۱۶. اصلاح Release خراب

اگر قبل از Publish مشکل وجود دارد، Draft را حذف کن، Tag را فقط در صورت منتشرنشدن حذف و اصلاح کن:

```cmd
git push --delete origin v0.1.0-alpha.4
git tag -d v0.1.0-alpha.4
```

اگر Release منتشر شده است، Tag را بازنویسی نکن. یک نسخه جدید مانند زیر بساز:

```text
0.1.0-alpha.4
```

Release منتشرشده باید Immutable تلقی شود.
