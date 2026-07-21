# Frequently asked questions

## Does the profiler modify MailVault?

No. The canonical source is read-only. A consistent database snapshot and all derived data are
written to a separate workspace.

## Does it copy attachments?

No. Physical files are located and inspected in place for availability, regular-file state and size.
Payloads are not copied into runtime evidence.

## Does it calculate SHA-256 for every file?

The current slice reconciles canonical SHA-256 identities recorded by MailVault and validates
physical availability and size. Full payload fixity modes are a later roadmap capability.

## Can it reopen a previous workspace?

Yes, through **Open existing workspace** in `0.1.0-alpha.4`.

## Can I run it while MailVault is syncing?

No. A writer lock or active mutation can prevent a consistent source profile. Stop writer activity
before preflight.

## Is the application cloud-connected?

No profiling cloud service or telemetry is required. Dependency and installer downloads naturally
require network access during development or installation.

## Why are attachment occurrences and exact binaries different counts?

The same exact SHA-256 payload can occur in many messages and under different filenames. The
profiler preserves occurrence history while representing physical binary identity once per
collection and hash.

## Why can the profiler report findings and still complete?

Missing files, size mismatches and filename relationships are evidence. They are not necessarily
engine failures. Contract, integrity and publication failures remain fatal.

## Which installer should I use?

Use the NSIS `-setup.exe` for normal interactive installation or MSI for managed deployment. Both
are unsigned in the current alpha.

## Can I reopen a workspace after restarting the application?

Yes, in `0.1.0-alpha.4`. Use **Open existing workspace**. The application inspects compatibility and lock state before loading the run catalog. Older alpha.2 workspaces require an explicit backed-up migration.
