# تحویل انتشار GitHub — `0.1.0-alpha.4`

این سند ترتیب امن انتقال Candidate آلفا ۴ به مخزن عمومی را مشخص می‌کند. آلفا ۴ قابلیت
**Exact Format Identification** را به Profiler اضافه می‌کند، اما تا قبل از سبز شدن Windows CI و
اجرای خصوصی روی آرشیو واقعی، `runtime green` محسوب نمی‌شود.

## خروجی آماده‌شده

مخزن اکنون شامل این موارد است:

1. موتور Read-only فهرست‌برداری فیزیکی Alpha 3؛
2. ماژول مستقل `profiler-format-siegfried`؛
3. قرارداد Pin شده Siegfried `1.11.6` و PRONOM `v124`؛
4. ثبت PUID، نام و نسخه فرمت، MIME، Basis، Warning و همه Matchها؛
5. انتخاب Primary Assertion به‌صورت Deterministic بدون حذف Ambiguity؛
6. Batch محدود، Timeout، سقف خروجی، Adaptive Failure Isolation و Resume؛
7. Workspace Lock مستقل برای جلوگیری از اجرای هم‌زمان Format Stage؛
8. Migration شماره `0006` و Queryهای Baseline-scoped؛
9. CLIهای `formats probe/identify/summary/list`؛
10. صفحه **Exact formats** در Desktop UI؛
11. Sanitized Export Schema 2؛
12. مستندات انگلیسی و فارسی، Runbook، Security، Privacy و Release Notes؛
13. نه تصویر Sanitized با وضوح بالا و Social Preview؛
14. CI برای Rust، Frontend، Windows Desktop و Documentation/Privacy؛
15. Release Workflow برای MSI/NSIS، SHA-256 و Attestation.

## وضعیت Validation همین بسته

در محیط آماده‌سازی فعلی این Gateها اجرا و سبز شده‌اند:

```text
Release configuration check
Documentation / privacy / local-link check
Rust Tree-sitter syntax parse: 34 files
TypeScript strict type-check
Vite production build
npm production audit: 0 vulnerabilities
npm complete audit: 0 vulnerabilities
Real Alpha 3 database migration: schema 5 → 6
SQLite quick_check / foreign_key_check
Baseline-scoped format projection isolation
```

در این محیط Rust toolchain موجود نبود. بنابراین موارد زیر هنوز ادعا نمی‌شوند:

```text
cargo check
rustfmt semantic gate
Clippy
Rust unit/integration tests
Native Tauri compile
MSI/NSIS build
Live Siegfried installation/probe
Real 13,684-object format run
```

مدرک کامل: `docs/VALIDATION_0.1.0-alpha.4.md`.

## مرحله ۱: ساخت Branch

بسته Source را در یک مسیر عمومی و بدون اطلاعات خصوصی استخراج کن و داخل Repository فعلی Branch
جدید بساز:

```powershell
git checkout main
git pull --ff-only
git checkout -b feat/alpha4-exact-format-identification
```

فایل‌های بسته را روی Repository جایگزین کن. پوشه‌های `node_modules`، `target`، `dist`، دیتابیس‌های
Profiler و Runtime Evidence نباید Commit شوند.

## مرحله ۲: Gate کامل Windows

داخل **x64 Native Tools Command Prompt for Visual Studio** یا PowerShellی که Toolchainهای لازم را
می‌بیند:

```powershell
cd E:\github\mailvault-collection-profiler
npm ci
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install-siegfried.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
npm run tauri:desktop:bundle
```

همچنین Tag Mapping را قبل از Release کنترل کن:

```powershell
npm run check:release-config
node .\scripts\check-tag-version.cjs v0.1.0-alpha.4
```

هیچ Tag یا Release تا قبل از سبز شدن این Gate ساخته نشود.

## مرحله ۳: Commit و Pull Request

```powershell
git add .
git status --short
git commit -m "feat: add exact PRONOM format identification"
git push -u origin feat/alpha4-exact-format-identification
```

Pull Request باید تمام Required Checkهای Branch Protection را پاس کند. Diff اصلی شامل ۱۸ فایل
جدید و ۵۱ فایل اصلاح‌شده است.

## مرحله ۴: Smoke Test خصوصی

پس از ساخت Release CLI، Runbook زیر را روی یک Copy/Backup از Workspace آلفا ۳ اجرا کن:

```text
docs/FORMAT_IDENTIFICATION_RUNBOOK.md
```

حداقل شواهد لازم:

- Probe دقیق Siegfried `1.11.6` و PRONOM `v124`؛
- Hash ابزار و Signature؛
- Migration موفق Workspace؛
- اجرای کامل یا Resume موفق Format Stage؛
- تعداد Identified، Unknown، Ambiguous، Mismatch و Tool Error؛
- `quick_check` و `foreign_key_check`؛
- Source non-mutation evidence؛
- Sanitized aggregate export.

Raw Profiler DB، Snapshot، Path، Filename، Subject، Domain و Progress JSONL عمومی نشوند.

## مرحله ۵: Merge و Tag

فقط پس از سبز شدن CI و Smoke Test:

```powershell
git checkout main
git pull --ff-only
git tag -a v0.1.0-alpha.4 -m "MailVault Collection Profiler 0.1.0-alpha.4"
git push origin v0.1.0-alpha.4
```

Workflow `release.yml` یک Draft Release می‌سازد. Draft باید شامل NSIS، MSI و
`SHA256SUMS.txt` باشد. قبل از Publish، Installerها را روی Windows تمیز نصب و باز کن و Bundled
Siegfried Resources را Probe کن.

## فایل‌های GitHub

Repository Description:

```text
Local-first, read-only physical inventory and exact file-format evidence for MailVault archives.
```

Social Preview:

```text
docs/assets/social-preview.png
```

Release Notes:

```text
docs/releases/v0.1.0-alpha.4.md
```

## مرز ادعا

بعد از CI می‌توان ادعا کرد که Source و Installer Build Gate سبز است. فقط بعد از اجرای واقعی
می‌توان آمار Format Distribution، Throughput، ETA، Peak Memory و Runtime Green بودن آلفا ۴ را
منتشر کرد.
