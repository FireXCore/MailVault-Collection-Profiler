# Getting started

## 1. Install

Download a Windows release, verify `SHA256SUMS.txt` and install the unsigned alpha package. Keep the
archive and workspace separate.

## 2. Create the physical baseline

Run read-only preflight, create the source snapshot and complete the physical inventory. Reopen the
workspace and confirm the expected baseline counts.

## 3. Install or verify exact-format resources

Source builds:

```powershell
.\scripts\install-siegfried.ps1
.\scripts\verify-siegfried.ps1
```

Installed desktop builds resolve the bundled Tauri resources automatically.

## 4. Migrate with a backup

Back up an Alpha 3 workspace before enabling migration. Migration 6 adds exact-format tables and
indexes without changing MailVault.

## 5. Run exact identification

Use the **Exact formats** view or follow the
[exact format runbook](FORMAT_IDENTIFICATION_RUNBOOK.md).

## 6. Review output

Start with:

1. tool errors;
2. ambiguous results;
3. unknown results;
4. extension mismatches;
5. generic formats such as OLE and octet-stream.

Do not treat an extension match as content validation and do not infer procurement meaning from a
PUID.

## 7. Keep evidence private

Profiler databases and raw logs contain sensitive metadata. Publish only sanitized aggregate
exports and release screenshots built from synthetic data.
