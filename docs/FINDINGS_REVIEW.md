# Findings review

Version `0.1.0-alpha.4` converts findings from a read-only list into a durable local review workflow while preserving MailVault and original run evidence.

## Categories

- **Requires attention:** unresolved warning and error findings.
- **Informational evidence:** exact-content and filename-history relationships that are not archive failures.
- **Reviewed:** findings with a current review decision.
- **All:** complete unresolved finding set.

The relationships `SAME_HASH_DIFFERENT_NAMES` and `SAME_NAME_DIFFERENT_HASHES` are informational by default.

## Statuses

| Status | Meaning | Note required |
|---|---|---:|
| Unreviewed | no decision has been recorded | no |
| Acknowledged | reviewed but not yet classified | no |
| Expected | valid and understood archive behavior | no |
| Needs investigation | requires technical or backup verification | yes |
| Resolved externally | corrected outside the profiler | yes |

Clearing a status appends a `status_cleared` event. It never deletes earlier history.

## Append-only history

Review actions are stored as events:

- `status_set`;
- `status_cleared`;
- `note_added`.

Events are ordered per run and finding, linked by SHA-256 and protected by SQLite update/delete triggers. The current state is a projection updated in the same immediate transaction as the event.

## Notes

Notes are trimmed, line endings are normalized, unsupported control characters are rejected, and the maximum size is 4,000 Unicode characters. Notes are not written to normal application logs and are excluded from sanitized export.

## Concurrent access

A workspace has one review writer. A second instance opens as read-only and displays the lock status. Review controls are disabled until the writer releases the operating-system lock.

## Sanitized export

JSON summary contains aggregate inventory, finding and review counts. CSV export contains only short derived tokens, finding code, severity, review status and review timestamp. It excludes absolute paths, filenames, email addresses, subjects, full canonical locators, attachment payloads and raw notes.

```powershell
.\target\release\mailvault-profiler.exe export sanitized-summary `
  --workspace "D:\MailVault-Profiler-Workspace" `
  --run "<run-id>" `
  --output ".\mailvault-profile-sanitized-summary.json"
```

The output path must already have a parent directory, must not exist, and must not be inside a registered MailVault source archive.
