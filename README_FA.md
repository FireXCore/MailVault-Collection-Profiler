<div dir="rtl" align="right">

<p align="center"><img src="docs/assets/social-preview.png" alt="MailVault Collection Profiler" /></p>

# MailVault Collection Profiler

**موجودی فیزیکی، کنترل کیفیت و تشخیص دقیق فرمت فایل برای آرشیوهای MailVault؛ کاملاً محلی و فقط‌خواندنی.**

[English](README.md) · [دریافت](https://github.com/FireXCore/mailvault-collection-profiler/releases) · [شروع سریع](docs/GETTING_STARTED.md) · [تشخیص دقیق فرمت](docs/FORMAT_IDENTIFICATION.md) · [امنیت](SECURITY.md)

> **نسخه توسعه‌ای:** `0.1.0-alpha.4` مرحله تشخیص دقیق فرمت را به‌صورت Versioned، Resume-safe و
> قابل ممیزی اضافه می‌کند. Build ویندوز، Siegfried `1.11.6` و Signature Database استاندارد
> PRONOM `v124` را به‌صورت Pin‌شده همراه برنامه بسته‌بندی می‌کند. آرشیو MailVault تغییر نمی‌کند.

## این برنامه دقیقاً چه کاری می‌کند؟

MailVault مدرک اصلی ایمیل را نگه می‌دارد. Profiler یک Index فنی و قابل بازسازی روی همان مدرک می‌سازد:

```text
MailVault فقط‌خواندنی
  ← Snapshot سازگار SQLite
  ← Physical Inventory
  ← هویت دقیق SHA-256 و تاریخچه occurrence
  ← کنترل فیزیکی فایل‌ها
  ← تشخیص دقیق فرمت و PUID
  ← Explorer دسکتاپ و CLI
```

Profiler ایمیل دانلود نمی‌کند، MailVault را تغییر نمی‌دهد، فایل را اجرا نمی‌کند، OCR انجام نمی‌دهد،
ZIP/RAR را باز نمی‌کند، سند را Invoice یا Quotation تشخیص نمی‌دهد و چیزی در RMS نمی‌نویسد.

## مبنای واقعی طراحی

| معیار | مقدار ثبت‌شده |
|---|---:|
| حجم آرشیو | حدود ۲۰ تا ۳۰ گیگابایت |
| پیام | 17,296 |
| MIME part | 54,450 |
| Content object | 13,684 |
| Content occurrence | 22,068 |
| Message relationship | 12,115 |
| حجم Blobهای منحصربه‌فرد | 6,467,253,277 بایت |
| Findings مرحله فیزیکی | 1,484 |
| Warning / Error | 2 / 0 |

Alpha 3 روی آرشیو واقعی Windows اعتبارسنجی شد. Alpha 4 همان Baseline را حفظ می‌کند، Migration شماره
`0006` و قرارداد اجرای تشخیص دقیق فرمت را اضافه می‌کند. قبل از انتشار آمار سرعت و توزیع فرمت‌ها،
یک اجرای خصوصی واقعی Alpha 4 هنوز لازم است.

## قابلیت‌های Alpha 4

- اجرای Identification فقط روی فایل‌های منحصربه‌فرد، نه هر occurrence تکراری؛
- Siegfried `1.11.6` و PRONOM `v124` Pin‌شده؛
- ثبت SHA-256 ابزار و Signature، نسخه‌ها، زمان ساخت و Identifierها؛
- نگهداری تمام Matchها و انتخاب Primary Assertion بدون حذف Ambiguity؛
- ذخیره PUID، نام و نسخه فرمت، MIME، Evidence basis، Warning و Format class؛
- وضعیت‌های `identified`، `unknown`، `ambiguous`، `empty`، `skipped_unavailable` و `tool_error`؛
- ثبت Extension mismatch فقط وقتی Alias امن با Extension واقعاً بررسی شده باشد؛
- Batch محدود، Timeout، محدودیت خروجی Process و جداسازی خودکار Batch خراب؛
- Checkpoint/Resume براساس Configuration fingerprint؛
- Lock اختصاصی Workspace برای جلوگیری از دو Writer هم‌زمان؛
- عدم Container expansion، OCR، Preview و Classification تجاری.

<p align="center"><img src="docs/assets/screenshots/07-exact-format-identification-ready.png" alt="داشبورد تشخیص دقیق فرمت" /></p>

<p align="center"><img src="docs/assets/screenshots/08-format-assertion-detail.png" alt="جزئیات Format Assertion" /></p>

<p align="center"><img src="docs/assets/screenshots/09-alpha4-architecture.png" alt="مرز معماری Alpha 4" /></p>

## اجرای CLI

ابتدا Workspace و Run فیزیکی را پیدا کن:

```powershell
.\target\release\mailvault-profiler.exe runs list `
  --workspace "E:\MailVault-Profiler-Alpha4" `
  --json
```

Toolchain را Probe کن:

```powershell
.\target\release\mailvault-profiler.exe formats probe `
  --siegfried ".\tools\siegfried\windows-x86_64\sf.exe" `
  --signature ".\tools\siegfried\windows-x86_64\default.sig" `
  --json
```

Identification را اجرا کن:

```powershell
.\target\release\mailvault-profiler.exe formats identify `
  --workspace "E:\MailVault-Profiler-Alpha4" `
  --run "<physical-profile-run-id>" `
  --siegfried ".\tools\siegfried\windows-x86_64\sf.exe" `
  --signature ".\tools\siegfried\windows-x86_64\default.sig" `
  --batch-size 2048 `
  --workers 0 `
  --timeout-seconds 900 `
  --resume true `
  --allow-migration `
  1> format-result.json `
  2> format-progress.jsonl
```

راهنمای کامل: [تشخیص دقیق فرمت](docs/FORMAT_IDENTIFICATION.md) و
[Runbook اجرایی](docs/FORMAT_IDENTIFICATION_RUNBOOK.md).

## Build از Source

```powershell
npm ci
.\scripts\install-siegfried.ps1
.\scripts\quality.ps1
npm run tauri:desktop:bundle
```

فایل‌های تولیدشده `sf.exe`، `default.sig` و `tool-manifest.json` داخل Git Commit نمی‌شوند؛ Release
Workflow آن‌ها را از Release رسمی دریافت، Hash را بررسی و به‌عنوان Resource داخل Installer قرار می‌دهد.

## وضعیت Validation

در محیط فعلی این موارد Pass شده‌اند:

- TypeScript type-check و Vite production build؛
- Parse نحوی تمام فایل‌های Rust با Tree-sitter؛
- Migration دیتابیس واقعی Alpha 3 از Schema 5 به 6؛
- `quick_check` و `foreign_key_check`؛
- حفظ 13,684 Object، 22,068 Occurrence و 1,484 Finding؛
- ثابت‌ماندن SHA-256 دیتابیس Source؛
- Projection آزمایشی Format Assertion روی Schema مهاجرت‌داده‌شده.

در Container فعلی Rust toolchain وجود نداشت؛ بنابراین ادعای دروغ درباره `cargo check`، Clippy، تست‌های
Rust، Build Native Tauri یا اجرای واقعی Siegfried نشده است. CI ویندوز برای اجرای همین Gateهای کامل
تنظیم شده است.

گزارش: [Validation Alpha 4](docs/VALIDATION_0.1.0-alpha.4.md).

## مستندات اصلی

- [فهرست کامل](docs/INDEX.md)
- [شروع سریع](docs/GETTING_STARTED.md)
- [نصب Windows](docs/INSTALLATION_WINDOWS.md)
- [راهنمای GUI](docs/GUI_GUIDE.md)
- [مرجع CLI](docs/CLI_REFERENCE.md)
- [تشخیص دقیق فرمت](docs/FORMAT_IDENTIFICATION.md)
- [Runbook تشخیص فرمت](docs/FORMAT_IDENTIFICATION_RUNBOOK.md)
- [معماری](docs/ARCHITECTURE.md)
- [مدل امنیتی](docs/SECURITY_MODEL.md)
- [حریم خصوصی](docs/PRIVACY.md)
- [Baseline واقعی](docs/REAL_ARCHIVE_BASELINE.md)
- [Release Notes Alpha 4](docs/releases/v0.1.0-alpha.4.md)

## خارج از Scope فعلی

Resume اجرای Physical Profile قطع‌شده، Full Fixity Hash، Container expansion، JHOVE، استخراج متن،
OCR، Embedding، LLM، تشخیص Invoice/Quotation، اتصال خودکار به RFQ و هر نوع نوشتن در RMS.

## مجوز

Apache License 2.0. فایل‌های [LICENSE](LICENSE)، [NOTICE](NOTICE) و
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) را ببین.

</div>
