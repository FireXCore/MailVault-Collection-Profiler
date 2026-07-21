# Roadmap

## Completed baseline

### Alpha 3

- physical inventory and explorer;
- findings review and workspace reopen;
- real 17,296-message runtime evidence.

### Alpha 4 implementation candidate

- exact format identification with pinned Siegfried/PRONOM;
- PUID and all-match evidence;
- bounded/resumable batch execution;
- exact-format UI and CLI;
- schema 6 and complete documentation.

Alpha 4 becomes runtime green only after Windows CI and the private real-archive format run pass.

## Next profiler slices

- benchmark-informed format worker/batch defaults;
- richer technical characterization derived from exact formats;
- optional JHOVE structural validation for supported formats;
- privacy-safe aggregate format exports;
- improved run control and interruption testing.

## Separate future layer

Safe text extraction, selective OCR and procurement classification should consume profiler
manifests as a separate document-corpus/intelligence boundary. They should not mutate MailVault or
weaken the profiler's technical evidence contract.
