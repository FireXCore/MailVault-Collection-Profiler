# تحویل نهایی آماده‌سازی مخزن GitHub

این سند مشخص می‌کند چه چیزی داخل بسته آماده شده و ترتیب اجرای مالک مخزن چیست.

## خروجی آماده‌شده

مخزن اکنون شامل این لایه‌ها است:

1. README عمومی با معرفی، Safety Contract، نصب، GUI، CLI، محدودیت‌ها و تصاویر Sanitized؛
2. مستند نصب Windows و Build از Source؛
3. راهنمای کامل GUI و CLI؛
4. معماری، Security Model، Privacy و Evidence Outputs؛
5. Troubleshooting و FAQ؛
6. راهنمای توسعه و Release Process؛
7. راهنمای فارسی تنظیم GitHub، Ruleset، Actions، Security و انتشار؛
8. Issue Formهای Bug، Feature و Compatibility؛
9. Pull Request Template حرفه‌ای؛
10. CI چهارمرحله‌ای برای Rust، Frontend، Windows Desktop و Privacy Scrub؛
11. CodeQL، Dependency Review و Dependabot؛
12. Release Workflow برای Build خودکار MSI/NSIS، Draft Release، SHA-256 و Attestation؛
13. Social Preview و شش تصویر عمومی بدون مسیر یا اطلاعات خصوصی؛
14. Gate خودکار برای Broken Link، فایل‌های ضروری، Registry خصوصی و اطلاعات حساس؛
15. Release Notes نسخه `0.1.0-alpha.3`.

## مرحله ۱: جایگزینی مخزن محلی

پوشه نهایی را در مسیر عمومی و بدون نام خصوصی استخراج کن:

```text
D:\mailvault-collection-profiler
```

پوشه قبلی را مبنا قرار نده. فایل‌های `target`، `node_modules` و `dist` داخل بسته Release Repository
نیستند و باید محلی ساخته شوند.

## مرحله ۲: Gate نهایی Windows

داخل **x64 Native Tools Command Prompt for Visual Studio**:

```cmd
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64
cd /d D:\mailvault-collection-profiler
npm ci
npm run check:docs
npm run check:release-config
node scripts\check-tag-version.cjs v0.1.0-alpha.3
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
npm run tauri:desktop:bundle
```

هیچ Tag یا Release تا قبل از سبز شدن کامل این Gate ساخته نشود.

## مرحله ۳: ساخت مخزن

Repository:

```text
FireXCore/mailvault-collection-profiler
```

Description:

```text
Local-first, read-only physical inventory and technical evidence explorer for MailVault archives.
```

Social Preview:

```text
docs/assets/social-preview.png
```

## مرحله ۴: Push اولیه

```cmd
git init
git branch -M main
git add .
git status --short
git commit -m "Release MailVault Collection Profiler 0.1.0-alpha.3"
git remote add origin https://github.com/FireXCore/mailvault-collection-profiler.git
git push -u origin main
```

## مرحله ۵: ساخت Labelها

بعد از `gh auth login`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\configure-github-labels.ps1 `
  -Repository "FireXCore/mailvault-collection-profiler"
```

## مرحله ۶: تنظیمات GitHub

راهنمای خط‌به‌خط:

```text
docs/GITHUB_PUBLISHING_GUIDE_FA.md
```

این بخش‌ها باید تنظیم شوند:

- About، Topics و Social Preview؛
- Ruleset شاخه `main`؛
- Required Status Checks؛
- Workflow Permission حداقلی؛
- Dependabot، Secret Scanning و Push Protection؛
- Private Vulnerability Reporting؛
- Community Standards.

## مرحله ۷: Release

```cmd
git tag -a v0.1.0-alpha.3 -m "MailVault Collection Profiler 0.1.0-alpha.3"
git push origin v0.1.0-alpha.3
```

Workflow فایل زیر Draft Release می‌سازد:

```text
.github/workflows/release.yml
```

Draft باید شامل NSIS، MSI و `SHA256SUMS.txt` باشد. تا قبل از Smoke Test روی یک Windows تمیز Publish
نشود.

## مرز ادعا

موارد تأییدشده:

- Quality Gate و تست‌های Rust سبز؛
- Windows Snapshot Regression سبز؛
- Tauri Desktop Compile سبز؛
- MSI و NSIS ساخته شده‌اند؛
- برنامه نصب و اجرا شده است؛
- Preflight آرشیو واقعی Schema v3 سازگار است؛
- اجرای واقعی وارد Metadata Inventory شده است.

موردی که هنوز نباید در README یا Release به‌عنوان قطعی ادعا شود:

```text
Full 20–30 GB benchmark completed
```

این ادعا فقط پس از پایان Run و ثبت Evidence Manifest، زمان، حافظه، شمارنده‌ها و Hash قبل/بعد مجاز است.
