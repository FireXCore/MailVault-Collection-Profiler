<div dir="rtl">

<div align="center">

# MailVault Collection Profiler

**ابزار محلی، فقط‌خواندنی و قابل‌ممیزی برای ساخت موجودی فیزیکی و بررسی شواهد فنی آرشیوهای MailVault**

[![CI](https://github.com/FireXCore/mailvault-collection-profiler/actions/workflows/ci.yml/badge.svg)](https://github.com/FireXCore/mailvault-collection-profiler/actions/workflows/ci.yml)
[![CodeQL](https://github.com/FireXCore/mailvault-collection-profiler/actions/workflows/codeql.yml/badge.svg)](https://github.com/FireXCore/mailvault-collection-profiler/actions/workflows/codeql.yml)
[![Release](https://img.shields.io/github/v/release/FireXCore/mailvault-collection-profiler?include_prereleases&label=release)](https://github.com/FireXCore/mailvault-collection-profiler/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/platform-Windows%20x64-2f80ed.svg)](docs/INSTALLATION_WINDOWS.md)

[README انگلیسی](README.md) · [دریافت نسخه](https://github.com/FireXCore/mailvault-collection-profiler/releases) · [شروع سریع](docs/GETTING_STARTED.md) · [راهنمای GUI](docs/GUI_GUIDE.md) · [مرجع CLI](docs/CLI_REFERENCE.md) · [امنیت](SECURITY.md)

</div>

> **نسخه آزمایشی توسعه‌ای:** نسخه `0.1.0-alpha.3` برای ارزیابی فنی کنترل‌شده آماده است.
> Installerهای Windows در این نسخه هنوز امضای دیجیتال عمومی ندارند. Workspaceهای قبلی اکنون
> قابل Inspect، Migration و Reopen هستند؛ تصمیم‌های Review به‌صورت Append-only ذخیره می‌شوند و
> خروجی Sanitized در قالب JSON و CSV در دسترس است. Resume اجرای قطع‌شده و تعمیر Archive عمداً
> در این نسخه فعال نیست.

![راه‌اندازی Collection و Preflight فقط‌خواندنی با داده‌های Sanitized](docs/assets/screenshots/01-collection-setup-preflight.png)

## چرا این پروژه ساخته شده است؟

آرشیو MailVault فقط پوشه‌ای از Attachmentها نیست. یک آرشیو واقعی شامل دیتابیس Canonical از نوع
SQLite، پیام‌ها، MIME Partها، Participantها، روابط بین پیام‌ها، Object Storeهای Content-addressed،
نام‌های تاریخی فایل‌ها و فایل‌های فیزیکی است. هرکدام از این لایه‌ها ممکن است با Metadata ثبت‌شده
ناسازگار، مفقود، غیرقابل‌خواندن یا ناقص باشند.

MailVault Collection Profiler بدون تغییر آرشیو Canonical، یک موجودی فنی تکرارپذیر می‌سازد:

```text
Read-only preflight
→ ساخت Snapshot سازگار SQLite در Workspace جدا
→ Inventory جریانی Metadata
→ تطبیق Attachment occurrence و هویت SHA-256
→ بررسی محدود و کنترل‌شده فایل‌های فیزیکی
→ Findings و Checkpointهای ماندگار
→ مرور Inventory، Findings و شواهد در GUI و CLI
```

## قرارداد اصلی ایمنی

- دیتابیس Canonical MailVault فقط در حالت Read-only باز می‌شود؛
- Workspace باید خارج از Archive Root قرار داشته باشد؛
- پیش از پردازش Inventory، یک Snapshot سازگار SQLite داخل Workspace ساخته می‌شود؛
- Locator مربوط به Blob پیش از بازشدن فایل Canonical و از نظر Path containment بررسی می‌شود؛
- شکست containment به Finding تبدیل می‌شود و Profiler مسیر نامعتبر را دنبال نمی‌کند؛
- هیچ Attachmentی اجرا، Render، Extract یا Upload نمی‌شود؛
- هیچ سرویس Cloud، حساب کاربری یا Telemetry اجباری وجود ندارد؛
- خروجی‌های Profiler مشتق‌شده‌اند و ممکن است شامل Filename، Domain یا Path حساس باشند؛
- فایل‌های خام Evidence و دیتابیس‌های SQLite نباید بدون Sanitization منتشر شوند.

جزئیات در [مدل امنیتی](docs/SECURITY_MODEL.md)، [حریم خصوصی](docs/PRIVACY.md) و
[خروجی‌های Evidence](docs/EVIDENCE_OUTPUTS.md) مستند شده است.

## قابلیت‌های پیاده‌سازی‌شده در `0.1.0-alpha.3`

- قرارداد سازگاری MailVault Schema v3 و Preflight فقط‌خواندنی؛
- بررسی مسیرها، Tableها، Columnها، Indexها، Writer Lock و سلامت ساختاری SQLite؛
- ساخت Snapshot با SQLite Online Backup API همراه با Progress و تشخیص تغییر Source؛
- Inventory جریانی پیام‌ها، MIME Partها، Participantها، Relationها و Blobها؛
- حفظ Attachment occurrenceها و هویت دقیق Content بر اساس SHA-256؛
- Normalization نام فایل و نگه‌داری Filename History؛
- تشخیص Same-hash/Different-name و Same-name/Different-hash؛
- بررسی محدود File-stat با انتخاب محافظه‌کارانه تعداد Workerها؛
- تشخیص Missing، Unreadable، Invalid locator، Non-regular object و Size mismatch؛
- Search و Cursor pagination بر اساس Filename، SHA-256، MIME، Subject و Sender domain؛
- Content Object Detail شامل Filename variantها، Message occurrenceها و Findingهای فنی؛
- Progress eventهای ساخت‌یافته برای Row، Object، Page و Byte؛
- برنامه Desktop ویندوز با Tauri و CLI مستقل؛
- Inspect سازگاری Workspace و Migration صریح همراه با Backup نگه‌داری‌شده؛
- Reopen کردن Runهای Completed، Failed یا Interrupted پس از Restart کامل برنامه؛
- Single-writer lock برای Workspace با Read-only fallback در Session هم‌زمان؛
- Review eventهای Append-only با Hash chain مبتنی بر SHA-256؛
- وضعیت‌های Review شامل `acknowledged`، `expected`، `needs_investigation` و `resolved_externally`؛
- الزام Note برای وضعیت‌های Investigation و Resolved externally؛
- جداسازی Findings به Requires attention، Informational evidence، Reviewed و All findings؛
- Export Sanitized در قالب JSON و CSV بدون Path، Filename، Email address و Review note؛
- تست‌های Integration برای Restart persistence، Lock fallback، Export Sanitized و عدم تغییر Source.

## نمای محصول

تمام تصاویر مستندات با داده‌های Sanitized یا Synthetic ساخته شده‌اند. هیچ آرشیو خصوصی، پیام،
Attachment یا دیتابیس واقعی در Repository نگه‌داری نمی‌شود. سیاست کامل تصاویر در
[سیاست Screenshotها](docs/SCREENSHOTS.md) قرار دارد.

### شروع، Preflight و انتخاب Workspace

![صفحه شروع و Preflight فقط‌خواندنی](docs/assets/screenshots/01-collection-setup-preflight.png)

### اجرای Profile و Progress ساخت‌یافته

![اجرای Profile با Progress ساخت‌یافته](docs/assets/screenshots/02-profile-running.png)

### بازکردن Runهای قبلی پس از Restart

![فهرست Runهای Workspace در alpha.3](docs/assets/screenshots/alpha3/03-runs.png)

### موجودی دقیق Binaryها

![Inventory صفحه‌بندی‌شده Content Objectها](docs/assets/screenshots/03-inventory-explorer.png)

### بررسی Findings نیازمند توجه

![Findings نیازمند بررسی](docs/assets/screenshots/alpha3/06-findings.png)

### Findings Explorer با Filterهای فنی

![Findings بر اساس Severity و Finding code](docs/assets/screenshots/04-findings-explorer.png)

### جزئیات Content Object و تاریخچه occurrence

![جزئیات Content Object و Filename history](docs/assets/screenshots/06-content-object-detail.png)

### Workflow امن CLI

![Workflow Sanitized در PowerShell](docs/assets/screenshots/05-cli-workflow.png)

## دریافت و نصب در Windows

1. صفحه [GitHub Releases](https://github.com/FireXCore/mailvault-collection-profiler/releases) را باز کن؛
2. Installer ویندوز x64 با پسوند `-setup.exe` یا بسته MSI را دریافت کن؛
3. Hash فایل را با `SHA256SUMS.txt` همراه Release مقایسه کن؛
4. برنامه را نصب کن؛ نسخه Alpha بدون امضای عمومی ممکن است پیام SmartScreen نشان دهد؛
5. Archive، Workspace و Runtime Evidence را در سه مسیر جدا نگه دار.

ساختار پیشنهادی:

```text
D:\MailVault-Demo
D:\MailVault-Profiler-Workspace
D:\MailVault-Profiler-Evidence
```

راهنمای کامل: [نصب Windows](docs/INSTALLATION_WINDOWS.md).

## اولین Profile با برنامه Desktop

1. عملیات Sync، Import یا Maintenance مربوط به MailVault را متوقف کن؛
2. Archive Root را انتخاب کن؛
3. گزینه **Run read-only preflight** را اجرا کن؛
4. سازگاری Source، Schema `v3` و نبود Writer lock فعال را بررسی کن؛
5. یک Workspace خالی و خارج از Archive Root انتخاب کن؛
6. Physical Inventory را ایجاد کن؛
7. بخش‌های **Physical inventory**، **Findings review** و Content Object Detail را بررسی کن.

پس از پایان Profile می‌توان Workspace را از صفحه Start دوباره باز کرد. پیش از بازشدن Run، برنامه
Schema، نیاز به Migration، وضعیت Lock و سلامت Review history را بررسی می‌کند. Migration از Schema
قدیمی فقط با تأیید صریح انجام می‌شود و پیش از آن Backup دیتابیس Workspace نگه‌داری خواهد شد.

## شروع سریع CLI

Preflight:

```powershell
.\target\release\mailvault-profiler.exe preflight `
  --archive "D:\MailVault-Demo"
```

Preflight ماشین‌خوان:

```powershell
.\target\release\mailvault-profiler.exe preflight `
  --archive "D:\MailVault-Demo" `
  --json
```

Profile کامل:

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

اجرای Evidence-grade:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\run-real-archive-profile.ps1 `
  -ArchiveRoot "D:\MailVault-Demo" `
  -WorkspaceRoot "D:\MailVault-Profiler-Workspace" `
  -EvidenceRoot "D:\MailVault-Profiler-Evidence"
```

CLI، Progress eventهای ساخت‌یافته را روی `stderr` و نتیجه نهایی Profile را روی `stdout` می‌نویسد.
جزئیات کامل در [مرجع CLI](docs/CLI_REFERENCE.md) قرار دارد.

## بازکردن و Review کردن Workspace موجود

Inspect و فهرست Runها:

```powershell
.\target\release\mailvault-profiler.exe workspace inspect `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --json

.\target\release\mailvault-profiler.exe runs list `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --json
```

ثبت Review برای یک Finding:

```powershell
.\target\release\mailvault-profiler.exe findings review `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --run "<run-id>" `
  --finding "<finding-id>" `
  --status needs_investigation `
  --note "Verify the physical object against the retained backup."
```

Export خلاصه Sanitized:

```powershell
.\target\release\mailvault-profiler.exe export sanitized-summary `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --run "<run-id>" `
  --output ".\mailvault-profile-sanitized-summary.json"
```

وضعیت Review داخل Profiler Database ذخیره می‌شود و MailVault Source یا Evidence اصلی را تغییر
نمی‌دهد. جزئیات در [ساختار Workspace](docs/WORKSPACE_FORMAT.md) و
[Review یافته‌ها](docs/FINDINGS_REVIEW.md) آمده است.

## نتیجه Validation واقعی `0.1.0-alpha.3`

نسخه Alpha.3 روی یک آرشیو خصوصی واقعی در Windows x64 اجرا و از ابتدا تا Reopen، Review و Export
اعتبارسنجی شده است. فقط Aggregateها در Repository منتشر می‌شوند و Evidence خام خصوصی باقی می‌ماند.

| معیار | مقدار ثبت‌شده در اجرای Alpha.3 |
|---|---:|
| Account | 1 |
| پیام | 17,296 |
| Message occurrence | 17,307 |
| MIME part | 54,450 |
| Attachment occurrence در قرارداد فعلی Adapter | 18,552 |
| Blob row / Content object | 13,684 |
| Content occurrence | 22,068 |
| Message relationship | 12,115 |
| Participant row | 51,101 |
| Blob bytes ثبت‌شده | 6,467,253,277 |
| Findings | 1,484 |
| Errors | 0 |
| Warnings | 2 |
| Missing object | 1 |
| Unreadable object | 0 |
| Size mismatch | 0 |

نتایج مهم:

- Source metrics و Snapshot metrics یکسان بودند؛
- 13,683 Object از 13,684 Object فیزیکی در دسترس بود؛
- یک Blob مفقود و یک Zero-byte content object شناسایی شد؛
- 375 رابطه Same-hash/Different-name ثبت شد؛
- 1,107 رابطه Same-name/Different-hash ثبت شد؛
- Workspace پس از Restart دوباره باز شد؛
- Reviewها و Hash chain پس از Restart باقی ماندند؛
- Session دوم در زمان Lock فعال به حالت Read-only منتقل شد؛
- Export Sanitized فاقد Path، Filename، Email و Review note خصوصی بود؛
- Source MailVault تغییر نکرد.

Baseline تاریخی خصوصی، 21,946 Attachment occurrence را در دامنه گسترده‌تر Metadata ثبت کرده است؛
اجرای Alpha.3 مقدار 18,552 را طبق قرارداد فعلی Adapter گزارش می‌کند. این دو عدد نباید به‌عنوان یک
Metric واحد جایگزین یکدیگر شوند و Reconciliation آن‌ها در
[Baseline آرشیو واقعی](docs/REAL_ARCHIVE_BASELINE.md) نگه‌داری می‌شود.

گزارش کامل: [Validation نسخه Alpha.3](docs/VALIDATION_0.1.0-alpha.3.md).

## Build از Source

پیش‌نیازها:

- Rust `1.97.1` مطابق `rust-toolchain.toml`؛
- Node.js نسخه `24.x` الزامی است؛
- npm نسخه `11+`؛
- Visual Studio 2026 یا Build Tools با workload **Desktop development with C++**؛
- Windows 10/11 SDK و WebView2 Runtime.

Build را در x64 Visual Studio Developer Command Prompt اجرا کن:

```cmd
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64
cd /d D:\mailvault-collection-profiler
npm ci
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\quality.ps1
npm run tauri:desktop:bundle
```

جزئیات: [Development](docs/DEVELOPMENT.md) و [نصب Windows](docs/INSTALLATION_WINDOWS.md).

## مستندات

| مستند | کاربرد |
|---|---|
| [فهرست مستندات](docs/INDEX.md) | نقشه کامل مستندات کاربر، امنیت، فنی و Maintainer |
| [شروع سریع](docs/GETTING_STARTED.md) | اجرای امن از نصب تا Review یافته‌ها |
| [نصب Windows](docs/INSTALLATION_WINDOWS.md) | پیش‌نیاز Installer و Build از Source |
| [راهنمای GUI](docs/GUI_GUIDE.md) | راه‌اندازی Collection، Inventory، Findings و Object detail |
| [مرجع CLI](docs/CLI_REFERENCE.md) | Commandها، Optionها، Streamهای خروجی و Exit behavior |
| [خروجی‌های Evidence](docs/EVIDENCE_OUTPUTS.md) | فایل‌های Evidence wrapper و قواعد نگه‌داری |
| [ساختار Workspace](docs/WORKSPACE_FORMAT.md) | Layout، Schema، Migration، Lock و Reopen |
| [Review یافته‌ها](docs/FINDINGS_REVIEW.md) | وضعیت‌ها، Note policy و Append-only history |
| [معماری](docs/ARCHITECTURE.md) | مرز Crateها و جریان داده |
| [مدل امنیتی](docs/SECURITY_MODEL.md) | Trust boundaryها، Invariantها و Threat handling |
| [حریم خصوصی](docs/PRIVACY.md) | پردازش محلی و Metadata مشتق‌شده حساس |
| [رفع اشکال](docs/TROUBLESHOOTING.md) | خطاهای شناخته‌شده Windows، Rust، npm، Tauri و Archive |
| [وضعیت پیاده‌سازی](docs/IMPLEMENTATION_STATUS.md) | قابلیت‌های تکمیل‌شده و موارد Deferred |
| [نقشه راه](docs/ROADMAP.md) | قابلیت‌های برنامه‌ریزی‌شده و خارج از Scope فعلی |
| [فرایند Release](docs/RELEASE_PROCESS.md) | Release و Verification برای Maintainer |
| [راهنمای انتشار در GitHub](docs/GITHUB_PUBLISHING_GUIDE_FA.md) | تنظیم Repository، Ruleset، CI و Release |
| [تحویل آماده‌سازی مخزن](docs/REPOSITORY_RELEASE_HANDOFF_FA.md) | ترتیب Push، Labelها، Gateها و Release |

## وضعیت پروژه و محدودیت‌ها

در این Release پیاده‌سازی نشده است:

- Resume اجرای Profiling قطع‌شده و کنترل‌های Pause/Cancel در UI؛
- Full payload SHA-256 fixity pass؛
- شناسایی دقیق Format با Siegfried/PRONOM؛
- Container expansion و اعتبارسنجی JHOVE؛
- OCR، Semantic extraction، Embedding، LLM یا Classification تجاری؛
- Auto-update برنامه؛
- امضای دیجیتال عمومی Installerهای Windows.

برای وضعیت دقیق به [وضعیت پیاده‌سازی](docs/IMPLEMENTATION_STATUS.md) و
[نقشه راه](docs/ROADMAP.md) مراجعه کن.

## مشارکت، پشتیبانی و گزارش امنیتی

پیش از تغییر قرارداد Evidence، Snapshot behavior، Canonical locator یا Migrationهای Profiler،
[CONTRIBUTING.md](CONTRIBUTING.md) را مطالعه کن. در Issueها هیچ Archive واقعی، فایل EML، Attachment،
Profiler database یا Log حساس را پیوست نکن.

- پشتیبانی: [SUPPORT.md](SUPPORT.md)
- گزارش محرمانه آسیب‌پذیری: [SECURITY.md](SECURITY.md)
- آیین‌نامه رفتاری: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)

## مجوز

این پروژه تحت Apache License 2.0 منتشر می‌شود. فایل‌های [LICENSE](LICENSE) و [NOTICE](NOTICE)
مرجع حقوقی هستند.

</div>
