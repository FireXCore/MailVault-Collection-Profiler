# پذیرش واقعی MailVault Collection Profiler 0.1.0-alpha.6

این نسخه زمانی قابل انتشار است که Pipeline واحد زیر روی Copy واقعی Archive سبز شود:

`preflight → online backup snapshot → full metadata reconciliation → selective file stat → selective exact format → final reconciliation → succeeded`

## ترتیب اجرا

1. `scripts/quality-alpha6.ps1 -BuildInstallers`
2. `scripts/validate-alpha6-real.ps1`
3. `scripts/validate-alpha6-delta.ps1` بعد از یک IMAP sync واقعی
4. `scripts/validate-alpha6-crash.ps1` برای هر چهار Stage
5. `scripts/validate-alpha6-soak.ps1 -Cycles 100`
6. `scripts/validate-alpha6-rebuild-equivalence.ps1`

## معیار Closure

- `PRAGMA quick_check=ok` و Foreign-key violation صفر؛
- MailVault SHA-256 قبل و بعد یکسان؛
- اجرای بدون تغییر، Object موفق قدیمی را دوباره Stat یا Identify نکند؛
- Object ناموفق در Run بعد Retry شود؛
- Attachment تکراری فقط Occurrence جدید بسازد و Content Object قبلی را reuse کند؛
- Force-kill همان Run را با `resume_count>0` ادامه دهد؛
- حداکثر Snapshotهای موفق مطابق Retention باقی بماند؛
- نتیجه تجمعی Incremental با Rebuild مستقل byte-for-byte در Projection منطقی برابر باشد؛
- MSI و NSIS، Source ZIP و SHA256SUMS از Commit و Tag واحد ساخته شوند.

تا قبل از سبز شدن این Matrix، Release در GitHub نباید منتشر شود.
